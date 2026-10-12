use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RenderMode {
    Auto,
    Text,
    Image,
}

#[derive(Debug, Parser)]
#[command(version, about = "Offline Markdown + Mermaid terminal reader")]
pub struct Cli {
    /// Markdown file, directory, or - for standard input
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Reload the current file after edits (including atomic saves)
    #[arg(short, long)]
    pub watch: bool,
    #[arg(long, value_enum, default_value = "auto")]
    pub render: RenderMode,
    /// Report terminal capabilities and rendering diagnostics
    #[arg(long)]
    pub doctor: bool,
    /// Additional local font for diagram images (TTF/OTF)
    #[arg(long)]
    pub font: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub internal_render_worker: bool,
}
