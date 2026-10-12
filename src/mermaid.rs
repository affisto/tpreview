use anyhow::{Result, bail};
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
    let kind = mmdflux::detect_diagram(source);
    if !matches!(kind, Some("flowchart" | "sequence")) {
        bail!("unsupported diagram type; supported: flowchart, sequenceDiagram");
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
        mmdflux::payload::Diagram::Flowchart(g) if g.nodes.len() > 120 || g.edges.len() > 240 => {
            bail!("flowchart limit: 120 nodes / 240 edges")
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
