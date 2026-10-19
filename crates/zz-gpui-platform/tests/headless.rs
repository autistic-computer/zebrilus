#![cfg(all(target_os = "linux", feature = "test-support"))]

use std::sync::Arc;
use zz_gpui::{HeadlessAppContext, Window, blue, div, prelude::*, px, red, size, white};
use zz_gpui_platform::{current_headless_renderer, wgpu::CosmicTextSystem};

struct Stripes;

impl Render for Stripes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .pl(px(4.))
            .child(div().w(px(8.)).h_full().bg(red()))
            .child(div().w(px(8.)).h_full().bg(white()))
            .child(div().w(px(8.)).h_full().bg(blue()))
    }
}

#[test]
fn window_screenshot_shows_the_laid_out_scene() -> anyhow::Result<()> {
    let mut cx = HeadlessAppContext::with_platform(
        Arc::new(CosmicTextSystem::new_without_system_fonts("IBM Plex Sans")),
        Arc::new(()),
        current_headless_renderer,
    );
    let window = cx.open_window(size(px(32.), px(8.)), |_, cx| cx.new(|_| Stripes))?;
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;

    let image = cx.capture_screenshot(window.into())?;
    assert_eq!(image.dimensions(), (64, 16));
    for (x, expected) in [
        (3, [0, 0, 0, 255]),
        (12, [255, 0, 0, 255]),
        (23, [255, 0, 0, 255]),
        (24, [255, 255, 255, 255]),
        (39, [255, 255, 255, 255]),
        (40, [0, 0, 255, 255]),
        (55, [0, 0, 255, 255]),
        (60, [0, 0, 0, 255]),
    ] {
        assert_eq!(image.get_pixel(x, 8).0, expected, "pixel at x = {x}");
    }
    Ok(())
}
