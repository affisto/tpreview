use base64::Engine;
use std::process::{Command, Stdio};
use tpreview::{graphics, mermaid};
use unicode_width::UnicodeWidthStr;

#[test]
fn class_and_state_diagrams_render_with_existing_native_engine() {
    for (source, labels) in [
        (
            "classDiagram\nclass User {\n  +String name\n  +login()\n}\nUser --> Session : owns",
            vec!["User", "Session", "name", "login", "owns"],
        ),
        (
            "stateDiagram-v2\n[*] --> Ready\nReady --> Running : start\nRunning --> [*]",
            vec!["Ready", "Running", "start"],
        ),
        ("stateDiagram\n[*] --> Ready\nReady --> [*]", vec!["Ready"]),
    ] {
        let rendered = mermaid::render(source).unwrap();
        assert!(rendered.svg.contains("<svg"));
        for label in labels {
            assert!(
                rendered.text.contains(label),
                "missing {label}: {}",
                rendered.text
            );
        }
    }
}

const ER: &str = r#"erDiagram
    CUSTOMER ||--o{ ORDER : places
    CUSTOMER {
        int id PK "customer identifier"
        string email UK
    }
    ORDER {
        int id PK
        int customer_id FK
    }
"#;

#[test]
fn additional_families_render_in_the_resource_limited_worker() {
    let fixtures: [(&str, bool, &[&str]); 3] = [
        (
            ER,
            true,
            &[
                "CUSTOMER",
                "ORDER",
                "customer_id",
                "places",
                "PK",
                "FK",
                "UK",
            ],
        ),
        (
            "classDiagram\nclass User {\n +String name\n +login()\n}\nUser --> Session : owns",
            false,
            &["User", "Session", "name", "login", "owns"],
        ),
        (
            "stateDiagram-v2\n[*] --> Ready\nReady --> Running : start\nRunning --> [*]",
            false,
            &["Ready", "Running", "start"],
        ),
    ];
    for (source, image, labels) in fixtures {
        let family = source.lines().next().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_tpreview"))
            .arg("--internal-render-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        serde_json::to_writer(
            child.stdin.take().unwrap(),
            &serde_json::json!({"source": source, "image": image, "font": null}),
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{family} worker exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value["error"].is_null(), "{family}: {value}");
        let diagram = &value["diagram"];
        assert!(diagram["image_error"].is_null(), "{family}: {value}");
        let text = diagram["text"].as_str().unwrap();
        for label in labels {
            assert!(text.contains(label), "{family} lost {label}: {text}");
        }
        assert!(!text.contains('\u{1b}'));
        if image {
            let png = base64::engine::general_purpose::STANDARD
                .decode(diagram["png"].as_str().unwrap())
                .unwrap();
            assert!(png.len() > 24, "worker returned an incomplete PNG");
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(&png[12..16], b"IHDR");
            let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
            assert!(width > 0 && height > 0);
            assert_eq!(diagram["width"].as_u64(), Some(u64::from(width)));
            assert_eq!(diagram["height"].as_u64(), Some(u64::from(height)));
        }
    }
}

#[test]
fn er_renders_entities_attributes_keys_comments_and_cardinality() {
    let rendered = mermaid::render(ER).unwrap();
    for label in [
        "CUSTOMER",
        "ORDER",
        "places",
        "customer_id",
        "PK",
        "FK",
        "UK",
        "customer identifier",
    ] {
        assert!(
            rendered.text.contains(label),
            "missing text {label}: {}",
            rendered.text
        );
        assert!(rendered.svg.contains(label), "missing SVG {label}");
    }
    assert!(rendered.text.contains("||"));
    assert!(rendered.text.contains("o{"));
    assert!(!rendered.svg.contains("<foreignObject"));
    assert!(!rendered.text.contains('\u{1b}'));
    let (_, width, height) = graphics::raster(&rendered.svg, None).unwrap();
    assert!(width > 0 && height > 0);
}

#[test]
fn er_preserves_unicode_aliases_and_display_widths() {
    let rendered =
        mermaid::render("erDiagram\nUSER[\"사용자 日本語\"] {\n string name \"계정 이름\"\n}")
            .unwrap();
    for label in ["사용자 日本語", "이름", "계정 이름"] {
        assert!(
            rendered.text.contains(label),
            "missing {label}: {}",
            rendered.text
        );
    }
    let widths: Vec<_> = rendered
        .text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::width)
        .collect();
    assert!(
        widths.iter().all(|width| *width == widths[0]),
        "uneven CJK box: {}",
        rendered.text
    );
}

#[test]
fn er_handles_comments_preamble_direction_and_non_identifying_relationships() {
    let rendered = mermaid::render(
        "---\ntitle: Accounts\n---\n%% comment\nerDiagram\n direction LR\n CUSTOMER |o..|{ ORDER : owns\n ISLAND {}",
    )
    .unwrap();
    for label in ["CUSTOMER", "ORDER", "owns", "ISLAND"] {
        assert!(rendered.text.contains(label));
    }
    assert!(rendered.text.contains('┆') || rendered.text.contains('╌'));
}

#[test]
fn malformed_er_source_is_not_silently_accepted() {
    for source in [
        "erDiagram\nUSER {\n string id",
        "erDiagram\nUSER { string }",
        "erDiagram\nUSER { int id BADKEY }",
        "erDiagram\nUSER { string id \"comment\" garbage }",
        "erDiagram\nA ||--o{ B : \"unfinished",
        "erDiagram\nA ??--o{ B : invalid",
        "erDiagram\nA ||--o{ B : owns\nclick A callback",
    ] {
        assert!(
            mermaid::render(source).is_err(),
            "unexpected success: {source}"
        );
    }
}

#[test]
fn unsupported_er_unicode_attribute_reports_an_error_without_panicking() {
    // The pinned grammar supports Unicode entity aliases and comments, but
    // not an unquoted Unicode attribute token. Its lexer panic is contained.
    let result = std::panic::catch_unwind(|| mermaid::render("erDiagram\nUSER {\n string 이름\n}"));
    let error = result
        .expect("an unsupported token must not escape the parser guard")
        .unwrap_err();
    assert!(error.to_string().contains("parser could not process"));
}

#[test]
fn new_graph_families_have_model_limits() {
    for header in ["classDiagram", "stateDiagram-v2"] {
        let source = format!(
            "{header}\n{}",
            (0..121)
                .map(|i| {
                    if header == "classDiagram" {
                        format!("class N{i}")
                    } else {
                        format!("state N{i}")
                    }
                })
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
    let source = format!(
        "erDiagram\n{}",
        (0..121)
            .map(|i| format!("N{i} {{}}"))
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
fn malformed_class_and_state_source_reports_errors() {
    for source in [
        "classDiagram\nclass User {\n +String name",
        "stateDiagram-v2\nstate Parent {\n[*] --> Child",
    ] {
        assert!(
            mermaid::render(source).is_err(),
            "unexpected success: {source}"
        );
    }
}

#[test]
fn er_preserves_self_relations_cycles_and_parallel_relations() {
    for source in [
        "erDiagram\nEMPLOYEE ||--o{ EMPLOYEE : manages",
        "erDiagram\nA ||--o{ B : first\nB ||--o{ C : second\nC ||--o{ A : third",
        "erDiagram\nA ||--o{ B : first\nA }o..o| B : second",
    ] {
        let rendered = mermaid::render(source).unwrap();
        for label in source
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(" : ").map(|(_, label)| label))
        {
            assert!(
                rendered.text.contains(label),
                "missing {label}: {}",
                rendered.text
            );
        }
    }
}
