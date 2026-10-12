use crate::source::safe;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    Heading(u8),
    Quote,
    List,
    Code(String),
    Table,
    Rule,
    Diagram,
}
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
    pub anchor: String,
    pub links: Vec<(String, String)>,
}
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}
impl Document {
    pub fn parse(input: &str) -> Self {
        let input = safe(input).replace('\t', "    ");
        let mut doc = Self::default();
        let mut text = String::new();
        let mut links = Vec::new();
        let mut kind = Kind::Paragraph;
        let mut section = String::new();
        let mut counts = std::collections::HashMap::<String, usize>::new();
        let mut list: Vec<Option<u64>> = Vec::new();
        let mut quote = 0;
        let mut code = false;
        let mut table = false;
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row = Vec::new();
        let mut link_start: Option<(usize, String)> = None;
        let flush = |doc: &mut Document,
                     text: &mut String,
                     links: &mut Vec<(String, String)>,
                     kind: &Kind,
                     section: &str,
                     counts: &mut std::collections::HashMap<String, usize>| {
            if text.is_empty() && *kind != Kind::Rule {
                return;
            }
            let key = format!("{section}/{kind:?}");
            let n = counts.entry(key.clone()).or_default();
            let anchor = if matches!(kind, Kind::Heading(_)) {
                section.to_string()
            } else {
                format!("{key}/{n}")
            };
            *n += 1;
            doc.blocks.push(Block {
                kind: kind.clone(),
                text: std::mem::take(text).trim_end_matches('\n').into(),
                anchor,
                links: std::mem::take(links),
            });
        };
        let options =
            Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
        for event in Parser::new_ext(&input, options) {
            match event {
                Event::Start(Tag::CodeBlock(info)) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    let lang = match info {
                        CodeBlockKind::Fenced(s) => {
                            s.split_whitespace().next().unwrap_or("").to_string()
                        }
                        _ => String::new(),
                    };
                    kind = if lang.eq_ignore_ascii_case("mermaid") {
                        Kind::Diagram
                    } else {
                        Kind::Code(lang)
                    };
                    code = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    code = false;
                    kind = Kind::Paragraph;
                }
                Event::Text(s) | Event::Code(s) => text.push_str(&s),
                Event::SoftBreak => text.push(if code { '\n' } else { ' ' }),
                Event::HardBreak => text.push('\n'),
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::Heading(level as u8);
                }
                Event::End(TagEnd::Heading(_)) => {
                    let base = slug(&text);
                    let count = counts.entry(format!("heading/{base}")).or_default();
                    section = if *count == 0 {
                        base
                    } else {
                        format!("{base}-{count}")
                    };
                    *count += 1;
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::Paragraph;
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    quote += 1;
                    kind = Kind::Quote;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    quote -= 1;
                    kind = Kind::Paragraph;
                }
                Event::Start(Tag::List(start)) => {
                    list.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    list.pop();
                }
                Event::Start(Tag::Item) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::List;
                    text.push_str(&"  ".repeat(list.len().saturating_sub(1)));
                    if let Some(Some(n)) = list.last_mut() {
                        text.push_str(&format!("{n}. "));
                        *n += 1;
                    } else {
                        text.push_str("• ");
                    }
                }
                Event::End(TagEnd::Item) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::Paragraph;
                }
                Event::TaskListMarker(done) => text.push_str(if done { "[x] " } else { "[ ] " }),
                Event::Start(Tag::Paragraph) => {
                    if !matches!(kind, Kind::List) {
                        kind = if quote > 0 {
                            Kind::Quote
                        } else {
                            Kind::Paragraph
                        };
                    }
                }
                Event::End(TagEnd::Paragraph) if !table => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    link_start = Some((text.len(), dest_url.to_string()));
                }
                Event::End(TagEnd::Link) => {
                    if let Some((start, dest)) = link_start.take() {
                        links.push((text[start..].to_owned(), dest));
                    }
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    text.push_str(&format!("[image: {dest_url}] "));
                }
                Event::Rule => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::Rule;
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    kind = Kind::Paragraph;
                }
                Event::Start(Tag::Table(_)) => {
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    table = true;
                    rows.clear();
                }
                Event::Start(Tag::TableCell) => {
                    text.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    row.push(std::mem::take(&mut text));
                }
                Event::End(TagEnd::TableRow | TagEnd::TableHead) => {
                    rows.push(std::mem::take(&mut row));
                }
                Event::End(TagEnd::Table) => {
                    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
                    let widths: Vec<_> = (0..columns)
                        .map(|i| {
                            rows.iter()
                                .filter_map(|r| r.get(i))
                                .map(|s| s.width())
                                .max()
                                .unwrap_or(0)
                        })
                        .collect();
                    let aligned_size = widths
                        .iter()
                        .map(|w| w + 5)
                        .sum::<usize>()
                        .saturating_mul(rows.len() + 1);
                    if aligned_size > 8 * 1024 * 1024 {
                        text.push_str(
                            "[Table too large to align; complete cell contents follow]\n",
                        );
                        for r in &rows {
                            text.push_str(&r.join(" | "));
                            text.push('\n');
                        }
                    } else {
                        for (n, r) in rows.iter().enumerate() {
                            text.push('│');
                            for (i, w) in widths.iter().enumerate() {
                                let cell = r.get(i).map(String::as_str).unwrap_or("");
                                text.push_str(&format!(
                                    " {cell}{} │",
                                    " ".repeat(w.saturating_sub(cell.width()))
                                ));
                            }
                            text.push('\n');
                            if n == 0 {
                                text.push_str(
                                    &widths
                                        .iter()
                                        .map(|w| "─".repeat(w + 2))
                                        .collect::<Vec<_>>()
                                        .join("┼"),
                                );
                                text.push('\n');
                            }
                        }
                    }
                    kind = Kind::Table;
                    flush(
                        &mut doc,
                        &mut text,
                        &mut links,
                        &kind,
                        &section,
                        &mut counts,
                    );
                    table = false;
                    kind = Kind::Paragraph;
                }
                Event::Html(s) | Event::InlineHtml(s) => text.push_str(&s),
                _ => {}
            }
        }
        flush(
            &mut doc,
            &mut text,
            &mut links,
            &kind,
            &section,
            &mut counts,
        );
        doc
    }
    pub fn diagrams(&self) -> Vec<(usize, String, String)> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == Kind::Diagram)
            .map(|(i, b)| (i, b.anchor.clone(), b.text.clone()))
            .collect()
    }
}
pub fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .collect()
}
