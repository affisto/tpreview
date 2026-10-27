use std::collections::HashMap;
use tpreview::{
    document::{Document, Kind},
    layout::{self, Layout, Row},
};
use unicode_width::UnicodeWidthStr;

fn table_rows(layout: &Layout, doc: &Document) -> Vec<Row> {
    layout
        .rows
        .iter()
        .filter(|row| doc.blocks[row.block].kind == Kind::Table && !row.text().is_empty())
        .cloned()
        .collect()
}

fn build(doc: &Document, width: usize) -> Layout {
    Layout::build(doc, &HashMap::new(), width, false, (8, 16))
}

#[test]
fn table_borders_connect_and_markdown_column_alignment_is_respected() {
    let doc = Document::parse(
        "| Left | Center | Right |\n| :--- | :---: | ---: |\n| longest | longest | longest |\n| x | x | x |\n",
    );
    let rows = table_rows(&build(&doc, 80), &doc);
    let texts: Vec<_> = rows.iter().map(Row::text).collect();
    let top = texts.first().unwrap();
    let bottom = texts.last().unwrap();
    assert!(top.starts_with('┌') && top.ends_with('┐') && top.contains('┬'));
    assert!(bottom.starts_with('└') && bottom.ends_with('┘') && bottom.contains('┴'));
    assert!(
        texts.iter().any(|line| {
            line.starts_with('├') && line.ends_with('┤') && line.contains('┼')
        })
    );
    assert!(texts.iter().all(|line| line.width() == top.width()));
    assert!(rows.iter().all(|row| !row.wide));

    let body = texts.iter().find(|line| line.contains('x')).unwrap();
    let cells: Vec<_> = body.split('│').skip(1).take(3).collect();
    assert_eq!(cells, [" x       ", "    x    ", "       x "]);
}

#[test]
fn long_cells_wrap_without_losing_ascii_cjk_or_combining_emoji_graphemes() {
    let value = "abcdefghijklmnopqrstuvwxyz한글日本語👩‍💻e\u{301}ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let doc = Document::parse(&format!("| Value |\n| --- |\n| {value} |\n"));
    for width in [12, 18, 30, 100] {
        let rows = table_rows(&build(&doc, width), &doc);
        assert!(rows.iter().all(|row| row.text().width() <= width));
        assert!(rows.iter().all(|row| !row.wide));
        let body: String = rows
            .iter()
            .map(Row::text)
            .filter(|line| line.starts_with('│'))
            .skip(1)
            .map(|line| line.trim_matches('│').trim().to_owned())
            .collect();
        assert_eq!(body, value, "cell content changed at width {width}");
    }
}

#[test]
fn wrapped_link_segments_keep_their_correct_targets_and_display_ranges() {
    let label = "같은👩‍💻e\u{301}LongLinkLabel日本語";
    let doc = Document::parse(&format!(
        "| First | Second |\n| :--- | ---: |\n| [{label}](one.md) | [{label}](two.md) |\n"
    ));
    let block = doc.blocks.iter().find(|b| b.kind == Kind::Table).unwrap();
    assert_eq!(block.links.len(), 2);
    for width in [8, 24, 80] {
        let rows = table_rows(&build(&doc, width), &doc);
        let mut labels = HashMap::<String, String>::new();
        for row in rows {
            for (range, index) in &row.links {
                assert!(range.start < range.end);
                assert!(range.end <= row.text().width());
                let segment = layout::crop(&row.line, range.start, range.len());
                let text: String = segment.spans.iter().map(|s| s.content.as_ref()).collect();
                labels
                    .entry(block.links[*index].target.clone())
                    .or_default()
                    .push_str(&text);
            }
        }
        assert_eq!(labels.get("one.md").map(String::as_str), Some(label));
        assert_eq!(labels.get("two.md").map(String::as_str), Some(label));
    }
}

#[test]
fn resizing_preserves_escaped_pipes_and_fitted_tables_ignore_horizontal_pan() {
    let doc = Document::parse("| Key | Value |\n| --- | --- |\n| alpha | left\\|right |\n");
    for width in [24, 40, 80, 24] {
        let rows = table_rows(&build(&doc, width), &doc);
        assert!(
            rows.iter()
                .all(|row| row.text().width() <= width && !row.wide)
        );
        let visible: String = rows
            .iter()
            .map(|row| {
                // Match UI panning: a prior wide code/diagram block may leave a nonzero offset.
                let cropped = layout::crop(&row.line, if row.wide { 8 } else { 0 }, width);
                cropped
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(visible.contains("left|right"));
        assert!(visible.contains("alpha"));
    }
}

#[test]
fn very_narrow_tables_retain_cells_including_wide_graphemes() {
    let doc = Document::parse("| A | B |\n| --- | --- |\n| 가나다 | 日本語👩‍💻 |\n");
    for width in [1, 2, 5, 8] {
        let rows = table_rows(&build(&doc, width), &doc);
        let text: String = rows.iter().map(Row::text).collect();
        for value in ["가", "나", "다", "日", "本", "語", "👩‍💻"] {
            assert!(text.contains(value), "missing {value} at width {width}");
        }
        assert!(
            rows.iter()
                .all(|row| row.text().width() <= width || row.wide)
        );
    }
}

#[test]
fn explicit_cell_breaks_preserve_links_and_inline_code_remains_literal() {
    let doc = Document::parse(
        "| Text | Code | Empty |\n| --- | --- | --- |\n| [one<br>two](next.md) | `<br>` | |\n",
    );
    let rows = table_rows(&build(&doc, 60), &doc);
    let texts: Vec<_> = rows.iter().map(Row::text).collect();
    let one = texts.iter().position(|line| line.contains("one")).unwrap();
    let two = texts.iter().position(|line| line.contains("two")).unwrap();
    assert_eq!(two, one + 1);
    assert!(texts[one].contains("<br>"));
    assert!(texts.iter().all(|line| line.width() == texts[0].width()));
    let block = doc.blocks.iter().find(|b| b.kind == Kind::Table).unwrap();
    let mut linked = String::new();
    for row in rows {
        for (range, index) in &row.links {
            assert_eq!(block.links[*index].target, "next.md");
            let segment = layout::crop(&row.line, range.start, range.len());
            linked.extend(segment.spans.iter().map(|span| span.content.as_ref()));
        }
    }
    assert_eq!(linked, "onetwo");
}

#[test]
fn sparse_long_cells_do_not_amplify_into_unbounded_padded_rows() {
    let columns = 40;
    let cell_size = 10_000;
    let mut source = format!(
        "|{}|\n|{}|\n",
        vec!["H"; columns].join("|"),
        vec!["---"; columns].join("|")
    );
    let long = "x".repeat(cell_size);
    for row in 0..columns {
        source.push('|');
        for column in 0..columns {
            source.push_str(if column == row { &long } else { "." });
            source.push('|');
        }
        source.push('\n');
    }
    let doc = Document::parse(&source);
    let layout = build(&doc, 200);
    let bytes: usize = layout.rows.iter().map(|row| row.text().len()).sum();
    assert!(bytes < 8 * 1024 * 1024, "table expanded to {bytes} bytes");
    let contents: usize = layout
        .rows
        .iter()
        .map(|row| row.text().bytes().filter(|b| *b == b'x').count())
        .sum();
    assert_eq!(contents, columns * cell_size);
}
