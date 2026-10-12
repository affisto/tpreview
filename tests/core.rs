use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};
use tpreview::{
    app::App,
    document::{Document, Kind},
    layout::{self, DiagramState, Layout},
    mermaid, source,
    terminal::{Capabilities, Decoder, InputEvent},
    worker::{Completion, Diagram},
};
use unicode_width::UnicodeWidthStr;

#[test]
fn markdown_extracts_only_actual_mermaid_fences() {
    let d = Document::parse(
        "# 한글 日本語\n\n`mermaid`\n\n```rust\nmermaid\n```\n\n~~~mermaid\nflowchart LR\nA-->B\n~~~\n\n```mermaid\nsequenceDiagram\nA->>B: hi\n```",
    );
    assert_eq!(d.diagrams().len(), 2);
    assert!(matches!(d.blocks[0].kind, Kind::Heading(1)));
    assert_eq!(d.blocks[0].anchor, "한글-日本語");
}
#[test]
fn real_engine_preserves_flow_and_sequence_labels() {
    for source in [
        "flowchart LR\nA[한국어] -->|연결| B[日本語]",
        "sequenceDiagram\nparticipant A as 사용자\nparticipant B as サーバー\nA->>B: 요청\nB-->>A: 応答",
    ] {
        let r = mermaid::render(source).unwrap();
        assert!(r.svg.contains("<svg"));
        assert!(!r.text.contains('\x1b'));
        if source.starts_with("flowchart") {
            assert!(
                r.text.contains("한국어") && r.text.contains("日本語") && r.text.contains("연결")
            );
        } else {
            assert!(
                r.text.contains("요청") && r.text.contains("応答") && r.text.contains("사용자")
            );
        }
    }
}
#[test]
fn unsupported_and_partial_syntax_never_silently_succeeds() {
    for source in [
        "pie\n\"A\":1",
        "flowchart LR\nA[unfinished",
        "sequenceDiagram\nA->>B: hi\nthis is not valid",
        "flowchart TD\nsubgraph X\nA-->B",
        "flowchart LR\nA-->B\nclick A \"https://example.org\"",
    ] {
        assert!(
            mermaid::render(source).is_err(),
            "unexpected success: {source}"
        );
    }
}
#[test]
fn verified_extended_sequence_subset() {
    let d=mermaid::render("sequenceDiagram\nautonumber\nactor A as User\nparticipant B as Server\nNote over A,B: Review\nalt ready\nA->>B: Request\nelse later\nB-->>A: Retry\nend").unwrap();
    for label in [
        "User", "Server", "Review", "ready", "later", "Request", "Retry",
    ] {
        assert!(d.text.contains(label), "missing {label}: {}", d.text);
    }
}
#[test]
fn layout_has_work_limits_before_graph_layout() {
    let source = format!(
        "flowchart TD\n{}",
        (0..125)
            .map(|i| format!("N{i}-->N{}", i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        mermaid::render(&source)
            .unwrap_err()
            .to_string()
            .contains("limit")
    );
}
#[test]
fn cjk_graphemes_wrap_and_wide_crop_stays_aligned() {
    let text = "한글日本語 e\u{301} 👩‍💻 hello";
    let lines = layout::wrap(text, 8);
    assert!(lines.iter().all(|s| s.width() <= 8));
    assert_eq!(lines.concat(), text);
    let line = ratatui::text::Line::raw("가나다abc");
    let crop = layout::crop(&line, 1, 5);
    let out: String = crop.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(out, " 나다");
    assert_eq!(out.width(), 5);
}
#[test]
fn tables_align_cjk_and_code_remains_unwrapped() {
    let doc = Document::parse(
        "| Name | 値 |\n|---|---|\n| 한글 | 日本語 |\n| a | 1 |\n\n```rust\nlet very_long_line = 12345678901234567890;\n```",
    );
    let table = doc.blocks.iter().find(|b| b.kind == Kind::Table).unwrap();
    let widths: Vec<_> = table
        .text
        .lines()
        .filter(|s| s.starts_with('│'))
        .map(str::width)
        .collect();
    assert!(widths.iter().all(|n| *n == widths[0]));
    let l = Layout::build(&doc, &HashMap::new(), 12, false, (8, 16));
    assert!(
        l.rows
            .iter()
            .any(|r| r.wide && r.text().contains("12345678901234567890"))
    );
}
#[test]
fn control_sequences_are_inert_everywhere() {
    let d = Document::parse("# a\x1b[2J\n\n```\n\x1b]52;c;secret\x07\n```\n\n[x](evil%1b.md)");
    assert!(
        d.blocks
            .iter()
            .all(|b| !b.text.chars().any(|c| c.is_control() && c != '\n'))
    );
    assert!(!source::safe("\x1b[2J\u{009b}\u{202e}").contains('\x1b'));
}
#[test]
fn positions_follow_unchanged_block_after_insertion_and_resize() {
    let first = Document::parse(
        "# 시작\n\nIntro\n\n## Keep\n\n현재 위치 currently reading this paragraph which wraps across several lines.\n\nEnd",
    );
    let before = Layout::build(&first, &HashMap::new(), 24, false, (8, 16));
    let block = first
        .blocks
        .iter()
        .position(|b| b.text.starts_with("현재"))
        .unwrap();
    let anchor = before.anchor(&first, before.starts[block] + 1);
    let next = Document::parse(
        "# 시작\n\nInserted\n\nIntro\n\n## Keep\n\nNew paragraph\n\n현재 위치 currently reading this paragraph which wraps across several lines.\n\nEnd",
    );
    let after = Layout::build(&next, &HashMap::new(), 18, false, (8, 16));
    let restored = after.restore(&next, anchor.as_ref());
    assert!(
        next.blocks[after.rows[restored].block]
            .text
            .starts_with("현재")
    );
}
#[test]
fn partial_probe_replies_and_keys_have_separate_channels() {
    let mut d = Decoder::default();
    d.feed(b"j\x1b_Gi=31;");
    assert!(matches!(d.next_event(),Some(InputEvent::Key(k)) if k.code==KeyCode::Char('j')));
    assert!(d.next_event().is_none());
    d.feed(b"OK\x1b\\\x1b[6;20;10tk");
    assert!(matches!(d.next_event(), Some(InputEvent::Reply(_))));
    assert!(matches!(d.next_event(), Some(InputEvent::Reply(_))));
    assert!(matches!(d.next_event(),Some(InputEvent::Key(k))if k.code==KeyCode::Char('k')));
    // A late reply is consumed even after startup; bracketed paste cannot execute q or Ctrl+C.
    d.feed(b"\x1b[200~q\x03\x1b[201~\x1b_Gi=31;OK\x1b\\n");
    assert!(matches!(d.next_event(), Some(InputEvent::Reply(_))));
    assert!(matches!(d.next_event(),Some(InputEvent::Key(k))if k.code==KeyCode::Char('n')));
}
#[test]
fn decoder_handles_split_utf8() {
    let mut d = Decoder::default();
    let bytes = "한".as_bytes();
    d.feed(&bytes[..1]);
    assert!(d.next_event().is_none());
    d.feed(&bytes[1..]);
    assert!(matches!(d.next_event(),Some(InputEvent::Key(k))if k.code==KeyCode::Char('한')));
}
#[test]
fn links_decode_relative_paths_and_reject_network() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().join("a.md");
    std::fs::write(&current, "").unwrap();
    let target = temp.path().join("다음 문서.md");
    std::fs::write(&target, "").unwrap();
    let (path, fragment) = source::resolve_link(Some(&current), "다음%20문서.md#제목").unwrap();
    assert_eq!(path, target.canonicalize().unwrap());
    assert_eq!(fragment.as_deref(), Some("제목"));
    for link in [
        "https://example.org/a.md",
        "javascript:alert(1)",
        "//host/a.md",
        "file:///a.md",
        "script.sh",
    ] {
        assert!(source::resolve_link(Some(&current), link).is_err());
    }
}
#[test]
fn directory_exclusions_and_size_limits() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir(t.path().join("node_modules")).unwrap();
    std::fs::write(t.path().join("node_modules/a.md"), "").unwrap();
    std::fs::write(t.path().join("hello.MD"), "").unwrap();
    assert_eq!(source::markdown_files(t.path()).len(), 1);
    assert!(
        source::read_limited(vec![b'a'; source::MAX_DOCUMENT as usize + 1].as_slice()).is_err()
    );
}
fn cap() -> Capabilities {
    Capabilities {
        image: false,
        cell: (8, 16),
        reason: "test".into(),
        tmux: false,
    }
}
#[test]
fn stale_completion_cannot_overwrite_current_document() {
    let mut app = App::new(
        None,
        std::env::current_dir().unwrap().join("examples"),
        "# Empty".into(),
        false,
        false,
        cap(),
        None,
    );
    app.doc = Document::parse("```mermaid\nflowchart LR\nA-->B\n```");
    app.version = 9;
    let (block, anchor, source) = app.doc.diagrams()[0].clone();
    app.diagrams.insert(
        block,
        DiagramState {
            source: source.clone(),
            pending: true,
            ..Default::default()
        },
    );
    let make = |version, label: &str| Completion {
        version,
        block,
        anchor: anchor.clone(),
        source: source.clone(),
        result: Ok(Arc::new(Diagram {
            text: label.into(),
            png: None,
            width: 0,
            height: 0,
            image_error: None,
        })),
        cached: false,
    };
    assert!(app.apply(make(9, "new")));
    assert!(!app.apply(make(8, "old")));
    assert_eq!(app.diagrams[&block].good.as_ref().unwrap().text, "new");
    let failure = Completion {
        version: 9,
        block,
        anchor,
        source,
        result: Err("unfinished syntax".into()),
        cached: false,
    };
    app.apply(failure);
    assert_eq!(app.diagrams[&block].good.as_ref().unwrap().text, "new");
    assert!(
        app.layout
            .rows
            .iter()
            .any(|r| r.text().contains("이전 정상"))
    );
}
#[test]
fn search_navigation_moves_to_matches_and_wraps() {
    let mut app = App::new(
        None,
        std::env::current_dir().unwrap().join("examples"),
        "# One\n\nneedle first\n\n# Two\n\nneedle second".into(),
        false,
        false,
        cap(),
        None,
    );
    for c in "/needle".chars() {
        app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert_eq!(app.matches.len(), 2);
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let first = app.scroll;
    app.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(app.scroll > first);
    app.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(app.scroll, first);
}
#[test]
fn non_tty_cli_outputs_readable_diagrams_without_escape_codes() {
    let output = Command::new(env!("CARGO_BIN_EXE_tpreview"))
        .args(["--render", "image", "examples/demo.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains('\x1b'));
    for label in [
        "Draft",
        "Publish",
        "Edit document",
        "Preview updated",
        "日本語",
    ] {
        assert!(text.contains(label), "missing {label}");
    }
}
#[test]
fn piped_markdown_and_errors_are_safe_noninteractive_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tpreview"))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# Piped\n\n```mermaid\npie\nA:1\n```\n\x1b[2J")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("unsupported") && text.contains("pie"));
    assert!(!text.contains('\x1b'));
}
#[test]
fn worker_raster_is_real_png_and_has_dimensions() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tpreview"))
        .arg("--internal-render-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(
        child.stdin.take().unwrap(),
        &serde_json::json!({"source":"sequenceDiagram\nA->>B: Hello","image":true,"font":null}),
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["error"].is_null(), "{value}");
    assert!(value["diagram"]["width"].as_u64().unwrap() > 10);
    assert!(
        value["diagram"]["png"]
            .as_str()
            .unwrap()
            .starts_with("iVBOR")
    );
}
#[test]
fn kitty_cleanup_only_deletes_owned_image_ids() {
    use tpreview::graphics::{Images, Placement};
    let mut images = Images::default();
    let mut bytes = Vec::new();
    images
        .display(
            &[Placement {
                id: 123,
                png: "AAAA".into(),
                x: 1,
                y: 2,
                cols: 3,
                rows: 4,
                crop: (0, 0, 10, 20),
            }],
            &mut bytes,
        )
        .unwrap();
    images.clear(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\x1b_Ga=d,d=I,i=123,q=2"));
    assert!(!text.contains("d=A"));
}

#[test]
fn unchanged_diagrams_reuse_the_same_cached_result_after_prose_edit() {
    let mut app = App::new(
        None,
        std::env::current_dir().unwrap().join("examples"),
        "# Start".into(),
        false,
        false,
        cap(),
        None,
    );
    app.doc = Document::parse("# Heading\n\n```mermaid\nflowchart LR\nA-->B\n```");
    let (block, _, source) = app.doc.diagrams()[0].clone();
    let result = Arc::new(Diagram {
        text: "cached relation".into(),
        png: None,
        width: 0,
        height: 0,
        image_error: None,
    });
    app.diagrams.insert(
        block,
        DiagramState {
            source,
            good: Some(result.clone()),
            ..Default::default()
        },
    );
    app.replace(
        "# Heading\n\nNew prose.\n\n```mermaid\nflowchart LR\nA-->B\n```".into(),
        true,
    );
    let state = app.diagrams.values().next().unwrap();
    assert!(state.cached && !state.pending);
    assert!(Arc::ptr_eq(state.good.as_ref().unwrap(), &result));
}
#[test]
fn ui_handles_small_sizes_and_panel_overlay_without_panics() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = App::new(
        None,
        std::env::current_dir().unwrap().join("examples"),
        "# 작은 화면\n\n日本語".into(),
        false,
        false,
        cap(),
        None,
    );
    for (width, height) in [(100, 32), (32, 12), (12, 5), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        app.panel = Some(tpreview::app::Panel::Toc);
        app.resize(width as usize, height as usize);
        terminal
            .draw(|f| {
                tpreview::ui::draw(f, &app);
            })
            .unwrap();
    }
}
#[test]
fn adversarial_table_does_not_expand_padding_quadratically() {
    let source = format!(
        "| {} | b |\n|---|---|\n{}",
        "a".repeat(10_000),
        "| x | y |\n".repeat(1000)
    );
    let doc = Document::parse(&source);
    let table = doc.blocks.iter().find(|b| b.kind == Kind::Table).unwrap();
    assert!(table.text.contains("too large to align"));
    assert!(table.text.len() < 100_000);
    assert!(table.text.contains(&"a".repeat(10_000)));
}

#[test]
fn named_pipe_is_rejected_without_waiting_for_a_writer() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("blocked.md");
    let name = CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(
        source::read_file(&path)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
}

#[test]
fn truncated_terminal_reply_cannot_swallow_ctrl_c() {
    let mut decoder = Decoder::default();
    decoder.feed(b"\x1b_Gi=31;truncated");
    assert!(decoder.next_event().is_none());
    decoder.feed(b"\x03");
    assert!(
        matches!(decoder.next_event(),Some(InputEvent::Key(k)) if k.code==KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
    );
}
