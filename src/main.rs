use anyhow::{Context, Result};
use clap::Parser;
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};
use tpreview::{
    app::App,
    cli::{Cli, RenderMode},
    document::{Document, Kind},
    layout::{DiagramState, Layout},
    source,
    terminal::{Capabilities, Guard, Input},
    ui, worker,
};

fn main() {
    if let Err(e) = run() {
        if e.downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
        {
            return;
        }
        eprintln!("tpreview: {}", source::safe(&format!("{e:#}")));
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    if cli.internal_render_worker {
        return worker::worker_main();
    }
    // Restore raw/alternate modes even if an unexpected panic reaches the UI thread.
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        if io::stdout().is_terminal() {
            let _ = crossterm::execute!(
                io::stdout(),
                crossterm::cursor::Show,
                crossterm::terminal::LeaveAlternateScreen
            );
        }
        old(info);
    }));
    if cli.doctor {
        let mut input = if io::stdout().is_terminal() {
            Input::open().ok()
        } else {
            None
        };
        let guard = if input.is_some() {
            Some(Guard::enter(false)?)
        } else {
            None
        };
        let cap = Capabilities::detect(input.as_mut(), cli.render)?;
        drop(guard);
        print!("{}", cap.report());
        let fonts = tpreview::graphics::font_options(cli.font.as_deref())?;
        println!("local font faces: {}", fonts.fontdb.faces().count());
        println!(
            "CJK font candidates: {}",
            fonts
                .fontdb
                .faces()
                .flat_map(|f| f.families.iter())
                .filter(|(name, _)| [
                    "CJK", "Gothic", "PingFang", "Hiragino", "Malgun", "Apple SD"
                ]
                .iter()
                .any(|p| name.contains(p)))
                .map(|(n, _)| n.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Ok(());
    }
    let stdin = cli.path == Path::new("-");
    let directory = !stdin && cli.path.is_dir();
    let root = if directory {
        cli.path.canonicalize()?
    } else if stdin {
        std::env::current_dir()?
    } else {
        cli.path
            .canonicalize()?
            .parent()
            .unwrap_or(Path::new("."))
            .to_owned()
    };
    let path: Option<PathBuf> = if stdin {
        None
    } else if directory {
        source::markdown_files(&root).into_iter().next()
    } else {
        Some(cli.path.canonicalize()?)
    };
    let source = if stdin {
        source::read_limited(io::stdin().lock())?
    } else if let Some(path) = &path {
        source::read_file(path)?
    } else {
        "# No Markdown files\n\nThis directory contains no .md or .markdown files. Press q to quit."
            .into()
    };
    let input = if io::stdout().is_terminal() {
        Input::open().ok()
    } else {
        None
    };
    let Some(mut input) = input else {
        return print_document(&source, cli.font);
    };
    let guard = Guard::enter(true).context("cannot enter raw terminal mode")?;
    let cap = Capabilities::detect(Some(&mut input), cli.render)?;
    let mut app = App::new(path, root, source, directory, cli.watch, cap, cli.font);
    if cli.render == RenderMode::Image && !app.cap.image {
        app.message = format!("image requested; {}", app.cap.reason);
    }
    ui::run(app, input, guard)
}
fn print_document(source: &str, font: Option<PathBuf>) -> Result<()> {
    let doc = Document::parse(source);
    let mut diagrams = std::collections::HashMap::new();
    for (ordinal, (i, _, source)) in doc.diagrams().into_iter().enumerate() {
        let result = if ordinal < 32 {
            worker::isolated(&source, false, font.clone(), None)
        } else {
            Err(anyhow::anyhow!("document limit: 32 diagrams"))
        };
        let mut state = DiagramState {
            source,
            ..Default::default()
        };
        match result {
            Ok(d) => state.good = Some(std::sync::Arc::new(d)),
            Err(e) => state.error = Some(source::safe(&e.to_string())),
        };
        diagrams.insert(i, state);
    }
    let layout = Layout::build(&doc, &diagrams, 100, false, (8, 16));
    let mut out = io::stdout().lock();
    for row in layout.rows {
        writeln!(out, "{}", row.text())?;
    }
    // Keep destinations available in plain output as well as in the interactive link selector.
    for block in &doc.blocks {
        if block.kind != Kind::Diagram {
            for (label, target) in &block.links {
                writeln!(out, "[{label}]: {target}")?;
            }
        }
    }
    Ok(())
}
