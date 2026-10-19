use crate::{
    document::{Document, Kind},
    worker::Diagram,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const INK: Color = Color::Rgb(213, 221, 232);
pub const MUTED: Color = Color::Rgb(130, 145, 161);
pub const ACCENT: Color = Color::Rgb(95, 211, 188);
pub const WARN: Color = Color::Rgb(240, 183, 112);
#[derive(Debug, Clone, Default)]
pub struct DiagramState {
    pub source: String,
    pub good: Option<Arc<Diagram>>,
    pub error: Option<String>,
    pub pending: bool,
    pub cached: bool,
}
#[derive(Debug, Clone)]
pub struct Row {
    pub line: Line<'static>,
    pub block: usize,
    pub wide: bool,
    pub image: Option<(usize, usize, usize)>,
    /// Display-cell ranges and the link index within this row's document block.
    pub links: Vec<(std::ops::Range<usize>, usize)>,
}
impl Row {
    pub fn text(&self) -> String {
        self.line.spans.iter().map(|s| s.content.as_ref()).collect()
    }
}
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub rows: Vec<Row>,
    pub starts: Vec<usize>,
}
static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
impl Layout {
    pub fn build(
        doc: &Document,
        diagrams: &HashMap<usize, DiagramState>,
        width: usize,
        image: bool,
        cell: (u16, u16),
    ) -> Self {
        let width = width.max(1);
        let mut result = Self::default();
        for (i, block) in doc.blocks.iter().enumerate() {
            result.starts.push(result.rows.len());
            let mut add = |line: Line<'static>, wide: bool, img: Option<(usize, usize, usize)>| {
                result.rows.push(Row {
                    line,
                    block: i,
                    wide,
                    image: img,
                    links: Vec::new(),
                })
            };
            match &block.kind {
                Kind::Diagram => {
                    let state = diagrams.get(&i);
                    let caption = if state.is_some_and(|s| s.error.is_some() && s.good.is_some()) {
                        "◇ Mermaid · 이전 정상 결과 표시 중 / previous valid result"
                    } else if state.is_some_and(|s| s.pending) {
                        "◇ Mermaid · rendering…"
                    } else {
                        "◇ Mermaid · Enter to inspect"
                    };
                    add(
                        Line::styled(caption, Style::default().fg(ACCENT)),
                        false,
                        None,
                    );
                    if let Some(error) = state.and_then(|s| s.error.as_ref()) {
                        for line in wrap(&format!("! {error}"), width) {
                            add(Line::styled(line, Style::default().fg(WARN)), false, None);
                        }
                    }
                    if let Some(d) = state.and_then(|s| s.good.as_ref()) {
                        if image && d.png.is_some() {
                            let cols = width
                                .min((d.width as usize).div_ceil(cell.0 as usize))
                                .max(1);
                            let rows = ((d.height as f64 / d.width as f64)
                                * (cols as f64 * cell.0 as f64)
                                / cell.1 as f64)
                                .ceil()
                                .max(1.) as usize;
                            for y in 0..rows {
                                add(Line::raw(""), true, Some((y, rows, cols)));
                            }
                        } else {
                            for line in d.text.lines() {
                                add(
                                    Line::styled(line.to_owned(), Style::default().fg(ACCENT)),
                                    true,
                                    None,
                                );
                            }
                        }
                        if let Some(e) = &d.image_error {
                            for line in wrap(&format!("Image unavailable: {e}"), width) {
                                add(Line::styled(line, Style::default().fg(WARN)), false, None);
                            }
                        }
                    }
                    if state.is_none_or(|s| s.good.is_none() || s.error.is_some()) {
                        for line in block.text.lines() {
                            add(
                                Line::styled(line.to_owned(), Style::default().fg(MUTED)),
                                true,
                                None,
                            );
                        }
                    }
                }
                Kind::Code(lang) | Kind::Frontmatter(lang) => {
                    add(
                        Line::styled(
                            format!(
                                "┌ {}{lang} · ←/→ to pan",
                                if matches!(block.kind, Kind::Frontmatter(_)) {
                                    "Frontmatter · "
                                } else {
                                    ""
                                }
                            ),
                            Style::default().fg(MUTED),
                        ),
                        true,
                        None,
                    );
                    let syntax = SYNTAX
                        .find_syntax_by_token(lang)
                        .unwrap_or_else(|| SYNTAX.find_syntax_plain_text());
                    let mut highlighter =
                        HighlightLines::new(syntax, &THEMES.themes["base16-ocean.dark"]);
                    for (n, line) in block.text.lines().enumerate() {
                        let spans = if n < 2000 && line.len() < 8192 {
                            highlighter.highlight_line(line, &SYNTAX).ok().map(|s| {
                                s.into_iter()
                                    .map(|(style, s)| {
                                        Span::styled(
                                            s.to_owned(),
                                            Style::default().fg(Color::Rgb(
                                                style.foreground.r,
                                                style.foreground.g,
                                                style.foreground.b,
                                            )),
                                        )
                                    })
                                    .collect()
                            })
                        } else {
                            None
                        };
                        add(
                            Line::from(spans.unwrap_or_else(|| vec![Span::raw(line.to_owned())])),
                            true,
                            None,
                        );
                    }
                    add(Line::styled("└", Style::default().fg(MUTED)), true, None);
                }
                Kind::Table => {
                    for line in block.text.lines() {
                        add(
                            Line::styled(line.to_owned(), Style::default().fg(INK)),
                            true,
                            None,
                        );
                    }
                }
                Kind::Rule => add(
                    Line::styled("─".repeat(width), Style::default().fg(MUTED)),
                    false,
                    None,
                ),
                kind => {
                    let style = match kind {
                        Kind::Heading(_) => {
                            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                        }
                        Kind::Quote => Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                        _ => Style::default().fg(INK),
                    };
                    let prefix = match kind {
                        Kind::Heading(level) => format!("{} ", "#".repeat(*level as usize)),
                        Kind::Quote => "│ ".into(),
                        _ => String::new(),
                    };
                    for line in wrap(&format!("{prefix}{}", block.text), width) {
                        add(Line::styled(line, style), false, None);
                    }
                }
            }
            add(Line::raw(""), false, None);
            if !block.links.is_empty() {
                let prefix = match block.kind {
                    Kind::Heading(level) => format!("{} ", "#".repeat(level as usize)),
                    Kind::Quote => "│ ".into(),
                    _ => String::new(),
                };
                let rendered = format!("{prefix}{}", block.text);
                let mut offset = 0;
                let end = result.rows.len() - 1;
                for row in &mut result.rows[result.starts[i]..end] {
                    let text = row.text();
                    let mut spans = Vec::new();
                    let mut cursor = 0;
                    for (index, link) in block.links.iter().enumerate() {
                        let start = (link.range.start + prefix.len()).max(offset);
                        let end = (link.range.end + prefix.len()).min(offset + text.len());
                        if start < end {
                            let start = start - offset;
                            let end = end - offset;
                            row.links
                                .push((text[..start].width()..text[..end].width(), index));
                            spans.push(Span::raw(text[cursor..start].to_owned()));
                            spans.push(Span::styled(
                                text[start..end].to_owned(),
                                Style::default()
                                    .fg(ACCENT)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                            cursor = end;
                        }
                    }
                    spans.push(Span::raw(text[cursor..].to_owned()));
                    row.line.spans = spans;
                    offset += text.len();
                    if rendered.as_bytes().get(offset) == Some(&b'\n') {
                        offset += 1;
                    }
                }
            }
        }
        result
    }
    pub fn anchor(&self, doc: &Document, scroll: usize) -> Option<(String, usize, String)> {
        let row = self.rows.get(scroll)?;
        let b = doc.blocks.get(row.block)?;
        Some((
            b.anchor.clone(),
            scroll.saturating_sub(self.starts[row.block]),
            b.text.clone(),
        ))
    }
    pub fn restore(&self, doc: &Document, anchor: Option<&(String, usize, String)>) -> usize {
        let Some((key, offset, text)) = anchor else {
            return 0;
        };
        let block = doc
            .blocks
            .iter()
            .position(|b| &b.text == text && &b.anchor == key)
            .or_else(|| doc.blocks.iter().position(|b| &b.text == text))
            .or_else(|| doc.blocks.iter().position(|b| &b.anchor == key))
            .or_else(|| {
                doc.blocks.iter().position(|b| {
                    matches!(b.kind, Kind::Heading(_)) && key.starts_with(&format!("{}/", b.anchor))
                })
            });
        block
            .map(|b| {
                let start = self.starts[b];
                let end = self.starts.get(b + 1).copied().unwrap_or(self.rows.len());
                start + offset.min(&end.saturating_sub(start + 1))
            })
            .unwrap_or(0)
    }
}
/// Unicode grapheme wrapping. Never slices UTF-8 or a wide grapheme in half.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for g in para.graphemes(true) {
            let w = g.width();
            if used + w > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push_str(g);
            used += w;
        }
        lines.push(line);
    }
    lines
}
pub fn crop(line: &Line<'static>, offset: usize, width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut pos = 0;
    let end = offset.saturating_add(width);
    for span in &line.spans {
        let mut s = String::new();
        for g in span.content.graphemes(true) {
            let w = g.width();
            let next = pos + w;
            if pos >= offset && next <= end {
                s.push_str(g);
            } else if pos < end && next > offset {
                s.push_str(&" ".repeat(next.min(end).saturating_sub(pos.max(offset))));
            }
            pos = next;
            if pos >= end {
                break;
            }
        }
        if !s.is_empty() {
            spans.push(Span::styled(s, span.style));
        }
        if pos >= end {
            break;
        }
    }
    Line::from(spans)
}
