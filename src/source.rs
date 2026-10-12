use anyhow::{Context, Result, bail};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
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
    if file.contains(':') || file.starts_with("//") {
        bail!("only local Markdown links are opened");
    }
    let file = percent_decode(file)?;
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
    if !is_markdown(&path) {
        bail!("link target must be .md or .markdown");
    }
    Ok((
        path.canonicalize()?,
        anchor.map(|s| percent_decode(&s)).transpose()?,
    ))
}
fn percent_decode(s: &str) -> Result<String> {
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
