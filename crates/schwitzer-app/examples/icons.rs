//! Renders assets/logo/mark.svg into the Windows (.ico) and macOS (.icns) icon files beside it: cargo run -p schwitzer-app --example icons.

use std::fs;
use std::path::Path;

const PNG_SIZES: [u32; 9] = [16, 24, 32, 48, 64, 128, 256, 512, 1024];
const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
const ICNS_SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];

/// Render the SVG at `size`×`size` as straight (non-premultiplied) RGBA.
fn render_rgba(tree: &resvg::usvg::Tree, size: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("pixmap")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let logo = crate_dir.join("assets/logo");
    let previews = crate_dir.join("../../target/icons");
    fs::create_dir_all(&previews)?;

    let svg = fs::read(logo.join("mark.svg"))?;
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())?;

    let mut ico = ico::IconDir::new(ico::ResourceType::Icon);
    let mut icns = icns::IconFamily::new();
    for size in PNG_SIZES {
        let rgba = render_rgba(&tree, size)?;
        let image = ico::IconImage::from_rgba_data(size, size, rgba.clone());
        image.write_png(fs::File::create(previews.join(format!("{size}.png")))?)?;
        if ICO_SIZES.contains(&size) {
            ico.add_entry(ico::IconDirEntry::encode(&image)?);
        }
        if ICNS_SIZES.contains(&size) {
            let image = icns::Image::from_data(icns::PixelFormat::RGBA, size, size, rgba)?;
            icns.add_icon(&image)?;
        }
    }
    ico.write(fs::File::create(logo.join("schwitzer.ico"))?)?;
    icns.write(fs::File::create(logo.join("schwitzer.icns"))?)?;
    eprintln!(
        "icons written to {} (previews in {})",
        logo.display(),
        previews.display()
    );
    Ok(())
}
