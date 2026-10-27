use crate::document::Link;
use pulldown_cmark::Alignment;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const MAX_PADDING: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<Vec<(String, Vec<Link>)>>,
    pub alignments: Vec<Alignment>,
}

pub struct TableRow {
    pub text: String,
    /// Byte ranges in this rendered row, paired with the original block link index.
    pub links: Vec<(Range<usize>, usize)>,
    pub header: bool,
}

impl TableRow {
    fn plain(text: String) -> Self {
        Self {
            text,
            links: Vec::new(),
            header: false,
        }
    }
}

impl Table {
    /// Keep a stable, viewport-independent representation for anchors and link actions.
    pub fn document_text(&self) -> (String, Vec<Link>) {
        let rows = self.render(None);
        let mut links: Vec<_> = self
            .rows
            .iter()
            .flatten()
            .flat_map(|(_, links)| links.iter().cloned())
            .collect();
        let mut text = String::new();
        let mut seen = vec![false; links.len()];
        for row in rows {
            let offset = text.len();
            for (range, index) in row.links {
                if !seen[index] {
                    links[index].range.start = offset + range.start;
                    seen[index] = true;
                }
                links[index].range.end = offset + range.end;
            }
            text.push_str(&row.text);
            text.push('\n');
        }
        (text, links)
    }

    pub fn render(&self, available: Option<usize>) -> Vec<TableRow> {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return Vec::new();
        }
        let mut desired = vec![3; columns];
        let mut minimum = vec![3; columns];
        for row in &self.rows {
            for (i, (cell, _)) in row.iter().enumerate() {
                desired[i] = desired[i].max(cell.lines().map(str::width).max().unwrap_or(0));
                minimum[i] =
                    minimum[i].max(cell.graphemes(true).map(str::width).max().unwrap_or(0));
            }
        }
        let overhead = columns.saturating_mul(3).saturating_add(1);
        let mut widths = desired.clone();
        if let Some(available) = available {
            let budget = available.saturating_sub(overhead);
            if minimum.iter().sum::<usize>() > budget {
                return self.stacked(available, false);
            }
            if desired.iter().sum::<usize>() > budget {
                // Cap the widest columns first, keeping short columns at their natural width.
                let (mut low, mut high) = (0, *desired.iter().max().unwrap());
                while low < high {
                    let cap = low + (high - low).div_ceil(2);
                    let used: usize = desired
                        .iter()
                        .zip(&minimum)
                        .map(|(want, min)| (*want).min(cap).max(*min))
                        .sum();
                    if used <= budget {
                        low = cap;
                    } else {
                        high = cap - 1;
                    }
                }
                widths = desired
                    .iter()
                    .zip(&minimum)
                    .map(|(want, min)| (*want).min(low).max(*min))
                    .collect();
                let mut spare = budget - widths.iter().sum::<usize>();
                for (width, want) in widths.iter_mut().zip(&desired) {
                    if spare > 0 && *width < *want {
                        *width += 1;
                        spare -= 1;
                    }
                }
            }
        }
        // Estimate padded output before allocating it, including wrapped row amplification
        // and the three-byte UTF-8 borders. Retain all values if a grid would be too large.
        let fragments: Vec<Vec<Vec<Range<usize>>>> = self
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(i, (cell, _))| cell_ranges(cell, widths[i]))
                    .collect()
            })
            .collect();
        let height = fragments
            .iter()
            .map(|row| row.iter().map(Vec::len).max().unwrap_or(1))
            .fold(self.rows.len().saturating_add(2), usize::saturating_add);
        let line_width = overhead.saturating_add(widths.iter().sum::<usize>());
        if height.saturating_mul(line_width).saturating_mul(3) > MAX_PADDING {
            return self.stacked(available.unwrap_or(usize::MAX), true);
        }
        let border = |left: char, middle: char, right: char| {
            TableRow::plain(format!(
                "{left}{}{right}",
                widths
                    .iter()
                    .map(|w| "─".repeat(w + 2))
                    .collect::<Vec<_>>()
                    .join(&middle.to_string())
            ))
        };
        let mut output = vec![border('┌', '┬', '┐')];
        let mut first_link = 0;
        for (row_index, (row, cells)) in self.rows.iter().zip(fragments).enumerate() {
            let height = cells.iter().map(Vec::len).max().unwrap_or(1);
            for y in 0..height {
                let mut rendered = TableRow {
                    text: "│".into(),
                    links: Vec::new(),
                    header: row_index == 0,
                };
                let mut link_index = first_link;
                for (i, width) in widths.iter().enumerate() {
                    let cell = row.get(i);
                    let range = cells.get(i).and_then(|lines| lines.get(y));
                    let value = cell
                        .zip(range)
                        .map(|((text, _), range)| &text[range.clone()])
                        .unwrap_or("");
                    let padding = width.saturating_sub(value.width());
                    let before = match self.alignments.get(i) {
                        Some(Alignment::Right) => padding,
                        Some(Alignment::Center) => padding / 2,
                        _ => 0,
                    };
                    rendered.text.push_str(&" ".repeat(before + 1));
                    let offset = rendered.text.len();
                    rendered.text.push_str(value);
                    if let Some((_, links)) = cell {
                        if let Some(range) = range {
                            add_links(&mut rendered, links, range, offset, link_index);
                        }
                        link_index += links.len();
                    }
                    rendered.text.push_str(&" ".repeat(padding - before + 1));
                    rendered.text.push('│');
                }
                output.push(rendered);
            }
            first_link += row.iter().map(|(_, links)| links.len()).sum::<usize>();
            if row_index + 1 < self.rows.len() {
                output.push(border('├', '┼', '┤'));
            }
        }
        output.push(border('└', '┴', '┘'));
        output
    }

    /// When columns cannot fit, show each cell vertically. Never discard a value.
    fn stacked(&self, width: usize, oversized: bool) -> Vec<TableRow> {
        let width = width.max(1);
        let mut output = Vec::new();
        if oversized {
            let notice = "[Table too large to align; complete cell contents follow]";
            output.extend(
                cell_ranges(notice, width)
                    .into_iter()
                    .map(|range| TableRow::plain(notice[range].into())),
            );
        }
        let mut link_index = 0;
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                output.push(TableRow::plain(if width == usize::MAX {
                    String::new()
                } else {
                    "─".repeat(width.min(100))
                }));
            }
            for (column, (cell, links)) in row.iter().enumerate() {
                if column > 0 {
                    output.push(TableRow::plain(String::new()));
                }
                for range in cell_ranges(cell, width) {
                    let mut rendered = TableRow {
                        text: cell[range.clone()].into(),
                        links: Vec::new(),
                        header: i == 0,
                    };
                    add_links(&mut rendered, links, &range, 0, link_index);
                    output.push(rendered);
                }
                link_index += links.len();
            }
        }
        output
    }
}

fn add_links(
    row: &mut TableRow,
    links: &[Link],
    fragment: &Range<usize>,
    offset: usize,
    base: usize,
) {
    for (i, link) in links.iter().enumerate() {
        let start = fragment.start.max(link.range.start);
        let end = fragment.end.min(link.range.end);
        if start < end {
            row.links.push((
                offset + start - fragment.start..offset + end - fragment.start,
                base + i,
            ));
        }
    }
}

/// Preserve graphemes and source byte offsets, including explicit cell line breaks.
fn cell_ranges(text: &str, width: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut base = 0;
    for line in text.split('\n') {
        let mut start = 0;
        let mut used = 0;
        let mut word_break = None;
        for (pos, grapheme) in line.grapheme_indices(true) {
            let size = grapheme.width();
            if used + size > width.max(1) && pos > start {
                let end = word_break.filter(|end| *end > start).unwrap_or(pos);
                ranges.push(base + start..base + end);
                start = end;
                used = line[start..pos].width();
                word_break = None;
            }
            used += size;
            if grapheme.chars().all(char::is_whitespace) {
                word_break = Some(pos + grapheme.len());
            }
        }
        ranges.push(base + start..base + line.len());
        base += line.len() + 1;
    }
    ranges
}
