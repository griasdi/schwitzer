//! The Schwitzer mark (`assets/logo/mark.svg`, chosen in `design/logo/`) as the window icon.

use iced::window;

const MARK_SVG: &[u8] = include_bytes!("../assets/logo/mark.svg");
/// Rendered size of the window icon; window managers scale it down.
const ICON_SIZE: u32 = 256;

/// The window icon for taskbars and window switchers (X11, Windows, macOS). Wayland has no
/// window icons; it shows the icon of a desktop entry, which the Linux archive does not ship.
pub fn window_icon() -> Option<window::Icon> {
    let rgba = render(ICON_SIZE)
        .inspect_err(|e| tracing::warn!("window icon: {e}"))
        .ok()?;
    window::icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE)
        .inspect_err(|e| tracing::warn!("window icon: {e}"))
        .ok()
}

/// The mark at `size`×`size` as straight (non-premultiplied) RGBA. `examples/icons.rs` has
/// the same renderer: an example cannot import modules of this bin-only crate.
fn render(size: u32) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_data(MARK_SVG, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("pixmap")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
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
