use anyhow::{Context, Result, bail};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use walkdir::WalkDir;

pub const MAX_DOCUMENT: u64 = 2 * 1024 * 1024;

pub fn read_limited(reader: impl Read) -> Result<String> {
    let mut data = Vec::new();
    reader.take(MAX_DOCUMENT + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_DOCUMENT {
        bail!("document exceeds 2 MiB limit");
    }
    let text = String::from_utf8(data).context("document must be UTF-8")?;
    if text.lines().count() > 30_000 {
        bail!("document exceeds 30,000 line limit");
    }
    Ok(text)
}
pub fn read_file(path: &Path) -> Result<String> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("cannot open {}", safe(&path.display().to_string())))?;
    if !f.metadata()?.is_file() {
        bail!("input must be a regular file");
    }
    read_limited(f)
}
/// Never pass document-originated C0/C1 or bidi override bytes to the terminal.
pub fn safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' => '\n',
            '\t' => '\t',
            c if c.is_control()
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                '\u{fffd}'
            }
            c => c,
        })
        .collect()
}
pub fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .max_depth(16)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | "node_modules" | "target" | ".venv" | "vendor" | "dist")
            )
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_markdown(e.path()))
        .take(5000)
        .map(|e| e.into_path())
        .collect();
    paths.sort();
    paths
}
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}
pub fn resolve_link(current: Option<&Path>, dest: &str) -> Result<(PathBuf, Option<String>)> {
    let (file, anchor) = dest
        .split_once('#')
        .map_or((dest, None), |(a, b)| (a, Some(b.to_owned())));
    let file = file
        .strip_prefix("file://localhost/")
        .map(|p| format!("/{p}"))
        .or_else(|| file.strip_prefix("file:///").map(|p| format!("/{p}")))
        .unwrap_or_else(|| file.to_owned());
    if (file.contains(':') && !file.starts_with('/')) || file.starts_with("//") {
        bail!("use an HTTP/HTTPS URL or a local document path");
    }
    let file = percent_decode(&file)?;
    if file.chars().any(char::is_control) {
        bail!("invalid control character in path");
    }
    let path = if file.is_empty() {
        current
            .context("stdin has no file for this link")?
            .to_path_buf()
    } else {
        current
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .join(file)
    };
    Ok((
        path.canonicalize()?,
        anchor.map(|s| percent_decode(&s)).transpose()?,
    ))
}
pub fn is_web_link(dest: &str) -> bool {
    dest.get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
        || dest
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
}
pub fn open_browser(dest: &str) -> Result<std::sync::mpsc::Receiver<Result<(), String>>> {
    if !is_web_link(dest)
        || dest.chars().any(|c| c.is_control() || c.is_whitespace())
        || dest
            .split_once("://")
            .is_none_or(|(_, rest)| rest.is_empty() || rest.starts_with(['/', '#', '?']))
    {
        bail!("invalid HTTP/HTTPS URL");
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // Pass a single argument, never a shell command or document-provided executable.
    let mut child = Command::new(program)
        .arg(dest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!("cannot start {program}; open this URL in your browser: {dest}")
        })?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = child.wait().map_err(|e| e.to_string()).and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(format!("{program} could not open the browser ({status})"))
            }
        });
        let _ = tx.send(result);
    });
    Ok(rx)
}
pub fn percent_decode(s: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'%' {
            let hex = s
                .as_bytes()
                .get(i + 1..i + 3)
                .context("invalid URL escape")?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex)?, 16)?);
            i += 3;
        } else {
            bytes.push(s.as_bytes()[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8(bytes)?)
}
