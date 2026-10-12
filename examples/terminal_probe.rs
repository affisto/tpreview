use tpreview::{
    cli::RenderMode,
    graphics::{self, Images, Placement},
    mermaid,
    terminal::{Capabilities, Guard, Input, InputEvent},
};
fn main() -> anyhow::Result<()> {
    let mut input = Input::open()?;
    let _guard = Guard::enter(true)?;
    let cap = Capabilities::detect(Some(&mut input), RenderMode::Auto)?;
    let mut images = Images::default();
    let rendered = mermaid::render("flowchart LR\nA[Terminal] --> B[Preview]")?;
    if cap.image {
        let (png, w, h) = graphics::raster(&rendered.svg, None)?;
        images.display(
            &[Placement {
                id: 41001,
                png,
                x: 2,
                y: 2,
                cols: 50,
                rows: 10,
                crop: (0, 0, w, h),
            }],
            &mut std::io::stdout(),
        )?;
    } else {
        print!(
            "\r\n{}\r\n{}",
            cap.reason,
            rendered.text.replace('\n', "\r\n")
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Some(InputEvent::Key(_)) = input.poll(std::time::Duration::from_millis(50))? {
            break;
        }
    }
    Ok(())
}
