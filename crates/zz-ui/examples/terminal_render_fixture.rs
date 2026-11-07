use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, HeadlessAppContext, IntoElement, ParentElement as _, Render,
    Styled as _, Window, canvas, div, px, size,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use serde_json::{Value, json};
use zz_terminal::{
    CellWidth, Color, Cursor, CursorStyle, OverlayKind, OverlaySpan, PackedCell, SessionStatus,
    TerminalAppearance, TerminalViewport,
};
use zz_ui::terminal::{RowRenderCache, TerminalRenderInput};

mod terminal_capture;

struct Fixture {
    viewport: Arc<TerminalViewport>,
    appearance: TerminalAppearance,
    cache: Rc<RefCell<RowRenderCache>>,
    geometry: Rc<RefCell<Value>>,
    font_size: f32,
    sub_row: f32,
    retained: bool,
}

impl Render for Fixture {
    #[expect(
        clippy::disallowed_methods,
        reason = "the terminal fixture fixes its independent font size and appearance colors"
    )]
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let font_size = self.font_size;
        let sub_row = self.sub_row;
        let retained = self.retained;
        let viewport = self.viewport.clone();
        let appearance = self.appearance.clone();
        let prepare = self.cache.clone();
        let paint = self.cache.clone();
        let geometry = self.geometry.clone();
        div()
            .size_full()
            .font_family("JetBrains Mono")
            .text_size(px(font_size))
            .bg(ala::rgb(0x0011_2233))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let revisions = [1, 2, 3, 4];
                        let state = prepare.borrow_mut().prepaint(
                            TerminalRenderInput {
                                viewport: &viewport,
                                row_revisions: &revisions,
                                revision_epoch: 1,
                                history: None,
                                images: None,
                                local_scroll_target: retained.then_some(0),
                                scroll_pixel_offset: px(sub_row),
                                overscroll: px(0.),
                                extra_height: px(0.),
                                command_output: !retained,
                                appearance: &appearance,
                                appearance_hash: 1,
                                text_opacity: 1.,
                                focused: true,
                                cursor_blink_visible: true,
                                marked_text: None,
                                rows_above: None,
                                rows_below: None,
                            },
                            bounds,
                            window,
                            cx,
                        );
                        let g = state.geometry;
                        let probe = window.text_system().shape_line(
                            "m".into(),
                            px(font_size),
                            &[ala::TextRun { len: 1, font: ala::font("JetBrains Mono"), color: ala::Hsla::default(), background_color: None, underline: None, strikethrough: None }],
                            None,
                        );
                        let gap = window.text_system().line_gap(probe.runs[0].font_id, px(font_size));
                        *geometry.borrow_mut() = json!({
                            "advance": f32::from(probe.width),
                            "ascent": f32::from(probe.ascent),
                            "descent": f32::from(probe.descent),
                            "line_gap": f32::from(gap),
                            "baseline": f32::from((g.line_height - probe.ascent - probe.descent) / 2. + probe.ascent),
                            "cell_width": f32::from(g.cell_width),
                            "line_height": f32::from(g.line_height),
                            "columns": g.grid.columns,
                            "rows": g.grid.rows,
                            "cell_width_px": g.grid.cell_width_px,
                            "cell_height_px": g.grid.cell_height_px,
                            "grid_origin": [f32::from(g.grid_bounds.origin.x), f32::from(g.grid_bounds.origin.y)],
                            "content_offset": f32::from(g.content_offset),
                        });
                        state
                    },
                    move |bounds, mut state, window, cx| {
                        paint.borrow_mut().paint(&mut state, bounds, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let font_size: f32 = std::env::args().nth(2).as_deref().unwrap_or("13").parse()?;
    let scale: f32 = std::env::args().nth(3).as_deref().unwrap_or("1").parse()?;
    let sub_row_arg = std::env::args().nth(4);
    let retained = sub_row_arg.is_some();
    let sub_row: f32 = sub_row_arg.as_deref().unwrap_or("0").parse()?;
    let height: f32 = std::env::args().nth(5).as_deref().unwrap_or("96").parse()?;
    if !sub_row.is_finite()
        || !(0.0..=128.0).contains(&sub_row)
        || !height.is_finite()
        || !(1.0..=512.0).contains(&height)
    {
        return Err("terminal fixture scroll or height exceeds its limits".into());
    }
    if !font_size.is_finite()
        || !(7.0..=48.0).contains(&font_size)
        || !scale.is_finite()
        || !(0.5..=3.0).contains(&scale)
    {
        return Err("terminal fixture size or scale exceeds its limits".into());
    }
    let physical_width = (256.0 * scale).round() as i32;
    let physical_height = (height * scale).round() as i32;
    fs::create_dir_all(&directory)?;
    let appearance = TerminalAppearance {
        font_families: vec!["JetBrains Mono".into()],
        padding_left: 0.,
        padding_right: 0.,
        padding_top: 0.,
        padding_bottom: 0.,
        minimum_contrast: 1.,
        foreground: Color::rgb(221, 238, 255),
        background: Color::rgb(17, 34, 51),
        ..TerminalAppearance::default()
    };
    let mut viewport =
        TerminalViewport::blank_with_appearance(32, 4, SessionStatus::Running, &appearance);
    for (row, text) in [
        "Terminal 0123456789",
        "!= => -> == <= >= ffi",
        "selection and link",
        "copy mode cursor",
    ]
    .into_iter()
    .enumerate()
    {
        for (column, value) in text.chars().enumerate() {
            Arc::make_mut(&mut viewport.cells)[row * 32 + column] =
                PackedCell::new(u32::from(value), 0, CellWidth::Narrow);
        }
    }
    viewport.overlays = Arc::from([
        OverlaySpan::new(2, 0, 9, OverlayKind::Selection),
        OverlaySpan::new(2, 14, 18, OverlayKind::LinkHover),
        OverlaySpan::new(3, 10, 11, OverlayKind::CopyCursor),
    ]);
    viewport.cursor = Some(Cursor::new(
        31,
        0,
        true,
        false,
        false,
        CursorStyle::Underline,
        Color::rgb(37, 171, 213),
    ));
    let font_system = Arc::new(CosmicTextSystem::new_without_system_fonts("JetBrains Mono"));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut cx =
        HeadlessAppContext::with_platform(font_system, Arc::new(penna::Assets), move || {
            let native = current_headless_renderer()?
                .ok_or_else(|| std::io::Error::other("native headless GPU renderer unavailable"))?;
            Ok(Some(Box::new(terminal_capture::Capture {
                native,
                scene: captured.clone(),
                size: size(
                    ala::DevicePixels(physical_width),
                    ala::DevicePixels(physical_height),
                ),
            })))
        });
    let font = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/JetBrainsMono-Regular.ttf");
    let alternate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../mjui/bench/assets/fonts/JetBrainsMono-Regular.ttf");
    let bytes = fs::read(&font).or_else(|_| fs::read(&alternate))?;
    cx.text_system().add_fonts(vec![Cow::Owned(bytes)])?;
    cx.update(penna::init);
    let viewport = Arc::new(viewport);
    let input = json!({
        "appearance": appearance,
        "columns": viewport.columns,
        "rows": viewport.rows,
        "cells": viewport.cells.iter().map(|cell| [cell.glyph(), u32::from(cell.style_id()), u32::from(cell.flags())]).collect::<Vec<_>>(),
        "overlays": viewport.overlays.iter().map(|span| [span.row, span.start, span.end, span.kind_and_flags()]).collect::<Vec<_>>(),
        "cursor": viewport.cursor.map(|cursor| json!([cursor.color().packed(), cursor.column(), cursor.row(), match cursor.style() { CursorStyle::Bar => 0, CursorStyle::Block => 1, CursorStyle::Underline => 2, CursorStyle::BlockHollow => 3 }, u8::from(cursor.visible()), u8::from(cursor.blinking()), u8::from(cursor.at_wide_tail())])),
        "font_size": font_size,
        "scale": scale,
        "sub_row": sub_row,
        "retained": retained,
        "width": physical_width,
        "height": physical_height,
    });
    let geometry = Rc::new(RefCell::new(Value::Null));
    let output = geometry.clone();
    let window = cx.open_window(size(px(256.), px(height)), |window, cx| {
        window.set_scale_factor(scale);
        cx.new(|_| Fixture {
            viewport,
            appearance,
            cache: Rc::new(RefCell::new(RowRenderCache::default())),
            geometry,
            font_size,
            sub_row,
            retained,
        })
    })?;
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
    cx.capture_screenshot(window.into())?
        .save(directory.join("native.png"))?;
    fs::write(
        directory.join("input.json"),
        serde_json::to_vec_pretty(&input)?,
    )?;
    fs::write(
        directory.join("geometry.json"),
        serde_json::to_vec_pretty(&*output.borrow())?,
    )?;
    fs::write(
        directory.join("scene.json"),
        serde_json::to_vec_pretty(&*scene.borrow())?,
    )?;
    Ok(())
}
