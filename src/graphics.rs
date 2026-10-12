//! Deliberately narrow Kitty implementation: direct PNG transmission, owned IDs, explicit cleanup.
use anyhow::{Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use resvg::{tiny_skia, usvg};
use std::io::{self, Write};

pub fn font_options(font: Option<&std::path::Path>) -> Result<usvg::Options<'static>> {
    let mut options = usvg::Options::default();
    options
        .fontdb_mut()
        .load_font_data(include_bytes!("../assets/NotoSans-Regular.ttf").to_vec());
    options.fontdb_mut().load_system_fonts();
    options.fontdb_mut().set_sans_serif_family("Noto Sans");
    options.font_family = "Noto Sans".into();
    if let Some(path) = font {
        options.fontdb_mut().load_font_file(path)?;
    }
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: Box::new(|_, _, _| None),
        resolve_string: Box::new(|_, _| None),
    };
    Ok(options)
}
pub fn raster(svg: &str, font: Option<&std::path::Path>) -> Result<(String, u32, u32)> {
    let options = font_options(font)?;
    let tree = usvg::Tree::from_str(svg, &options)?;
    let size = tree.size().to_int_size();
    if size.width() > 4096
        || size.height() > 4096
        || u64::from(size.width()) * u64::from(size.height()) > 4_000_000
    {
        bail!("diagram raster exceeds 4096 px / 4 megapixel limit");
    }
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("invalid raster size"))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok((
        STANDARD.encode(pixmap.encode_png()?),
        size.width(),
        size.height(),
    ))
}
#[derive(Debug, Clone)]
pub struct Placement {
    pub id: u32,
    pub png: String,
    pub x: u16,
    pub y: u16,
    pub cols: u16,
    pub rows: u16,
    pub crop: (u32, u32, u32, u32),
}
impl Placement {
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        write!(out, "\x1b[{};{}H", self.y + 1, self.x + 1)?;
        let chunks: Vec<_> = self.png.as_bytes().chunks(4096).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let more = usize::from(i + 1 < chunks.len());
            if i == 0 {
                write!(
                    out,
                    "\x1b_Ga=T,f=100,t=d,i={},q=2,C=1,c={},r={},x={},y={},w={},h={},m={};",
                    self.id,
                    self.cols,
                    self.rows,
                    self.crop.0,
                    self.crop.1,
                    self.crop.2,
                    self.crop.3,
                    more
                )?;
            } else {
                write!(out, "\x1b_Gm={more};")?;
            }
            out.write_all(chunk)?;
            out.write_all(b"\x1b\\")?;
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Images {
    ids: Vec<u32>,
}
impl Images {
    pub fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        for id in self.ids.drain(..) {
            write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
        }
        out.flush()
    }
    pub fn display(&mut self, placements: &[Placement], out: &mut impl Write) -> io::Result<()> {
        for p in placements {
            self.ids.push(p.id);
            p.write(out)?;
        }
        out.flush()
    }
}
impl Drop for Images {
    fn drop(&mut self) {
        let _ = self.clear(&mut io::stdout());
    }
}
