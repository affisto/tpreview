use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
fn main() -> anyhow::Result<()> {
    let md = include_str!("demo.md");
    let mut source = String::new();
    let mut active = false;
    let mut index = 0;
    std::fs::create_dir_all("target/probe")?;
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if info.as_ref() == "mermaid" =>
            {
                active = true;
                source.clear();
            }
            Event::Text(text) if active => source.push_str(&text),
            Event::End(TagEnd::CodeBlock) if active => {
                active = false;
                let result = tpreview::mermaid::render(&source)?;
                println!("{}", result.text);
                std::fs::write(format!("target/probe/{index}.svg"), result.svg)?;
                index += 1;
            }
            _ => {}
        }
    }
    anyhow::ensure!(index == 2, "expected two diagrams");
    Ok(())
}
