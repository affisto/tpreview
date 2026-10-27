use anyhow::{Context, Result, bail};
use mmdflux::{OutputFormat, RenderConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rendered {
    pub text: String,
    pub svg: String,
}

pub fn render(source: &str) -> Result<Rendered> {
    if source.len() > 16 * 1024 || source.lines().count() > 160 {
        bail!("diagram limit: 16 KiB / 160 lines");
    }
    if source.lines().any(|line| {
        line.trim_start()
            .strip_prefix("click")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    }) {
        bail!("Mermaid: click statements are not supported");
    }
    let kind = mmdflux::detect_diagram(source);
    if !matches!(kind, Some("flowchart" | "sequence" | "class" | "state")) {
        let engine = merman::Engine::new();
        if guarded(|| {
            engine
                .parse_metadata_sync(source, merman::ParseOptions::strict())
                .map_err(anyhow::Error::from)
        })
        .ok()
        .flatten()
        .is_some_and(|meta| meta.diagram_type == "er")
        {
            return guarded(|| render_er(source, &engine));
        }
        bail!(
            "unsupported diagram type; supported: flowchart, sequenceDiagram, classDiagram, stateDiagram, erDiagram"
        );
    }
    if matches!(kind, Some("class" | "state")) {
        // mmdflux tolerates some incomplete class/state blocks. A strict
        // grammar pass prevents an unfinished edit replacing the last result.
        guarded(|| {
            merman::Engine::new()
                .parse_diagram_for_render_model_sync(source, merman::ParseOptions::strict())
                .map_err(|e| anyhow::anyhow!("Mermaid: {e}"))?
                .context("empty diagram")?;
            Ok(())
        })?;
    }
    let validation: serde_json::Value = serde_json::from_str(&mmdflux::validate_diagram(source))?;
    if validation["valid"] != true
        || validation["diagnostics"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    {
        let reason = validation["diagnostics"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|d| {
                        let message = d["message"].as_str().unwrap_or("invalid syntax");
                        d["line"].as_u64().map_or_else(
                            || message.to_owned(),
                            |line| format!("line {line}: {message}"),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| "invalid Mermaid syntax".into());
        bail!("Mermaid: {reason}");
    }
    // Bound layout work using the engine's parsed model, never a regex approximation.
    let registry = mmdflux::builtins::default_registry();
    let instance = registry
        .create(kind.unwrap())
        .ok_or_else(|| anyhow::anyhow!("engine unavailable"))?;
    let payload = instance
        .parse(source)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_payload()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    match payload {
        mmdflux::payload::Diagram::Flowchart(g)
        | mmdflux::payload::Diagram::Class(g)
        | mmdflux::payload::Diagram::State(g)
            if g.nodes.len() > 120 || g.edges.len() > 240 =>
        {
            bail!("{} limit: 120 nodes / 240 edges", kind.unwrap())
        }
        mmdflux::payload::Diagram::Sequence(s)
            if s.participants.len() > 40 || s.events.len() > 160 =>
        {
            bail!("sequence limit: 40 participants / 160 events")
        }
        _ => {}
    }
    let config = RenderConfig::default();
    let text = mmdflux::render_diagram(source, OutputFormat::Text, &config)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let svg = mmdflux::render_diagram(source, OutputFormat::Svg, &config)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(Rendered {
        text: crate::source::safe(&text),
        svg,
    })
}

fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    // Third-party parsers can panic on unsupported UTF-8 token boundaries.
    // Keep the same visible-error / last-good behavior as ordinary parse errors.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|_| anyhow::anyhow!("Mermaid parser could not process this syntax"))?
}

fn render_er(source: &str, engine: &merman::Engine) -> Result<Rendered> {
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, merman::ParseOptions::strict())
        .map_err(|e| anyhow::anyhow!("Mermaid ER: {e}"))?
        .context("empty ER diagram")?;
    let merman::RenderSemanticModel::Er(model) = &parsed.model else {
        bail!("expected ER diagram model");
    };
    if model.entities.len() > 120
        || model.relationships.len() > 240
        || model
            .entities
            .values()
            .map(|entity| entity.attributes.len())
            .sum::<usize>()
            > 480
    {
        bail!("ER limit: 120 entities / 240 relationships / 480 attributes");
    }
    let mut text = render_er_text(model)?;
    if let Some(title) = &parsed.meta.title {
        text = format!("{title}\n{text}");
    }
    // Both outputs use the same strict parser. The resvg-safe pipeline keeps
    // native SVG text labels instead of browser-only foreignObject elements.
    let svg = merman::render::render_svg_resvg_safe_sync(
        engine,
        source,
        merman::ParseOptions::strict(),
        &merman::render::headless_layout_options(),
        &merman::render::SvgRenderOptions::default(),
    )
    .map_err(|e| anyhow::anyhow!("Mermaid ER SVG: {e}"))?
    .context("empty ER SVG")?;
    Ok(Rendered {
        text: crate::source::safe(&text),
        svg,
    })
}

fn render_er_text(model: &merman::diagrams::er::ErDiagramRenderModel) -> Result<String> {
    let options = merman::ascii::AsciiRenderOptions::unicode();
    match merman::ascii::render_er(model, &options) {
        Ok(text) => Ok(text),
        Err(merman::ascii::AsciiError::UnsupportedFeature {
            diagram_type: "er",
            feature: "cyclic ER relationship layouts",
        }) => {
            // The native text router handles acyclic graphs. For cycles, use
            // one connected entity pair per relationship, preserving every
            // attribute/cardinality/label instead of dropping a back edge.
            let mut text = "ER · cyclic relationships shown separately\n".to_owned();
            for relationship in &model.relationships {
                let mut pair = model.clone();
                pair.relationships = vec![relationship.clone()];
                pair.entities.retain(|_, entity| {
                    entity.id == relationship.entity_a || entity.id == relationship.entity_b
                });
                text.push_str(
                    &merman::ascii::render_er(&pair, &options)
                        .map_err(|e| anyhow::anyhow!("Mermaid ER text: {e}"))?,
                );
                text.push('\n');
            }
            let mut isolated = model.clone();
            isolated.relationships.clear();
            isolated.entities.retain(|_, entity| {
                !model.relationships.iter().any(|relationship| {
                    entity.id == relationship.entity_a || entity.id == relationship.entity_b
                })
            });
            text.push_str(
                &merman::ascii::render_er(&isolated, &options)
                    .map_err(|e| anyhow::anyhow!("Mermaid ER text: {e}"))?,
            );
            Ok(text)
        }
        Err(e) => bail!("Mermaid ER text: {e}"),
    }
}
