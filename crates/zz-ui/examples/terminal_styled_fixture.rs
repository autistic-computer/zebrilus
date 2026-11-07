use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, HeadlessAppContext, IntoElement, ParentElement as _, Render,
    Styled as _, Window, canvas, div, px, size,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use serde_json::{Value, json};
use zz_terminal::{
    ATTR_BOLD, ATTR_FAINT, ATTR_HYPERLINK, ATTR_INVISIBLE, ATTR_ITALIC, ATTR_OVERLINE,
    ATTR_STRIKETHROUGH, CellWidth, Color, Cursor, CursorStyle, FontSyntheticStyle,
    GRAPHEME_TABLE_BIT, OverlayKind, OverlaySpan, PackedCell, PackedStyle, SessionStatus,
    TerminalAppearance, TerminalDictionary, TerminalViewport, UnderlineStyle,
};
use zz_ui::terminal::{RowRenderCache, TerminalRenderInput, terminal_font_for_style};

#[path = "terminal_styled_capture/mod.rs"]
mod terminal_capture;

const COLUMNS: usize = 48;
const ROWS: usize = 8;

struct Fixture {
    viewport: Arc<TerminalViewport>,
    appearance: TerminalAppearance,
    cache: Rc<RefCell<RowRenderCache>>,
    geometry: Rc<RefCell<Value>>,
    font_size: f32,
    marked: Option<String>,
    retained: bool,
    sub_row: f32,
}

impl Render for Fixture {
    #[expect(
        clippy::disallowed_methods,
        reason = "the isolated terminal fixture fixes its rendering inputs"
    )]
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let font_size = self.font_size;
        let viewport = self.viewport.clone();
        let appearance = self.appearance.clone();
        let family = appearance.font_families[0].clone();
        let base_font = terminal_font_for_style(&appearance, &family, false, false);
        let prepare = self.cache.clone();
        let paint = self.cache.clone();
        let geometry = self.geometry.clone();
        let marked = self.marked.clone();
        let retained = self.retained;
        let sub_row = self.sub_row;
        div().size_full().font(base_font).text_size(px(font_size)).bg(ala::rgb(0x0011_2233)).child(
            canvas(
                move |bounds, window, cx| {
                    let revisions = [1, 2, 3, 4, 5, 6, 7, 8];
                    let state = prepare.borrow_mut().prepaint(TerminalRenderInput {
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
                        marked_text: marked.as_deref(),
                        rows_above: None,
                        rows_below: None,
                    }, bounds, window, cx);
                    let g = state.geometry;
                    let font = terminal_font_for_style(&appearance, "JetBrains Mono", false, false);
                    let probe = window.text_system().shape_line("m".into(), px(font_size), &[ala::TextRun {
                        len: 1, font, color: ala::Hsla::default(), background_color: None, underline: None, strikethrough: None,
                    }], None);
                    let gap = window.text_system().line_gap(probe.runs[0].font_id, px(font_size));
                    let faces = (0..4).map(|style| {
                        let font = terminal_font_for_style(&appearance, "JetBrains Mono", style & 1 != 0, style & 2 != 0);
                        let resolved = window.text_system().resolve_font(&font);
                        let mut without_bold = font.clone();
                        without_bold.weight = ala::FontWeight(f32::from(appearance.font_weight));
                        let mut without_italic = font;
                        without_italic.style = ala::FontStyle::Normal;
                        let allowed = appearance.font_synthetic_style.allows(style & 1 != 0, style & 2 != 0);
                        json!({
                            "style": style,
                            "resolved": resolved.0,
                            "without_bold": window.text_system().resolve_font(&without_bold).0,
                            "without_italic": window.text_system().resolve_font(&without_italic).0,
                            "synthetic_bold": allowed && style & 1 != 0 && resolved == window.text_system().resolve_font(&without_bold),
                            "synthetic_italic": allowed && style & 2 != 0 && resolved == window.text_system().resolve_font(&without_italic),
                        })
                    }).collect::<Vec<_>>();
                    let mut glyph_probes = Vec::new();
                    if appearance.font_families.len() > 1 {
                        for text in ["界", "中", "語", "a\u{301}", "कित", "ก้", "क्ष", "a\u{308}\u{301}", "שלום", "مرحبا", "語 ", "αβγ Жизнь שלום مرحبا"] {
                            let line = window.text_system().shape_line(text.into(), px(font_size), &[ala::TextRun {
                                len: text.len(), font: terminal_font_for_style(&appearance, "JetBrains Mono", false, false), color: ala::Hsla::default(), background_color: None, underline: None, strikethrough: None,
                            }], None);
                            glyph_probes.push(json!({
                                "text": text, "width": f32::from(line.width), "ascent": f32::from(line.ascent), "descent": f32::from(line.descent),
                                "glyphs": line.runs.iter().flat_map(|run| run.glyphs.iter().map(move |glyph| json!({"font": run.font_id.0, "glyph": glyph.id.0, "index": glyph.index, "x": f32::from(glyph.position.x), "y": f32::from(glyph.position.y)}))).collect::<Vec<_>>(),
                            }));
                        }
                    }
                    *geometry.borrow_mut() = json!({
                        "advance": f32::from(probe.width), "ascent": f32::from(probe.ascent), "descent": f32::from(probe.descent), "line_gap": f32::from(gap),
                        "baseline": f32::from((g.line_height - probe.ascent - probe.descent) / 2. + probe.ascent),
                        "cell_width": f32::from(g.cell_width), "line_height": f32::from(g.line_height),
                        "columns": g.grid.columns, "rows": g.grid.rows, "faces": faces, "glyph_probes": glyph_probes,
                        "grid_bounds": [f32::from(g.grid_bounds.origin.x), f32::from(g.grid_bounds.origin.y), f32::from(g.grid_bounds.size.width), f32::from(g.grid_bounds.size.height)],
                        "content_offset": f32::from(g.content_offset),
                    });
                    state
                },
                move |bounds, mut state, window, cx| paint.borrow_mut().paint(&mut state, bounds, window, cx),
            ).size_full()
        )
    }
}

fn put(view: &mut TerminalViewport, row: usize, start: usize, text: &str, style: u16) {
    for (index, value) in text.chars().enumerate() {
        if start + index < COLUMNS {
            Arc::make_mut(&mut view.cells)[row * COLUMNS + start + index] =
                PackedCell::new(u32::from(value), style, CellWidth::Narrow);
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let directory = PathBuf::from(args.get(1).ok_or("output directory required")?);
    let case = args.get(2).map_or("regular-only", String::as_str);
    let font_size: f32 = args.get(3).map_or("13", String::as_str).parse()?;
    let scale: f32 = args.get(4).map_or("1", String::as_str).parse()?;
    if !font_size.is_finite()
        || !(7.0..=48.0).contains(&font_size)
        || !scale.is_finite()
        || !(0.5..=3.0).contains(&scale)
    {
        return Err("styled fixture size or scale exceeds limit".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mjui/bench/assets/fonts");
    let jb = root.join("JetBrainsMono-Regular.ttf");
    let dejavu = PathBuf::from("/usr/share/fonts/truetype/dejavu");
    let regular = dejavu.join("DejaVuSansMono.ttf");
    let bold = dejavu.join("DejaVuSansMono-Bold.ttf");
    let italic = dejavu.join("DejaVuSansMono-Oblique.ttf");
    let bold_italic = dejavu.join("DejaVuSansMono-BoldOblique.ttf");
    let mut files = vec![jb.clone()];
    let mut style_paths = [jb.clone(), jb.clone(), jb.clone(), jb];
    let mut family = "JetBrains Mono";
    if matches!(case, "full-faces" | "mixed-faces") {
        family = "DejaVu Sans Mono";
        files = vec![regular.clone(), bold.clone()];
        style_paths = [regular.clone(), bold.clone(), regular, bold];
        if case == "full-faces" {
            files.extend([italic.clone(), bold_italic.clone()]);
            style_paths[2] = italic;
            style_paths[3] = bold_italic;
        }
    }
    let mut appearance = TerminalAppearance {
        font_families: vec![family.into()],
        padding_left: 0.,
        padding_right: 0.,
        padding_top: 0.,
        padding_bottom: 0.,
        minimum_contrast: 1.,
        foreground: Color::rgb(221, 238, 255),
        background: Color::rgb(17, 34, 51),
        ..TerminalAppearance::default()
    };
    if case == "no-synthetic" {
        appearance.font_synthetic_style = FontSyntheticStyle {
            bold: false,
            italic: false,
            bold_italic: false,
        };
    }
    if case == "contrast" {
        appearance.minimum_contrast = 4.5;
    }
    let mut viewport = TerminalViewport::blank_with_appearance(
        COLUMNS as u16,
        ROWS as u16,
        SessionStatus::Running,
        &appearance,
    );
    let mut styles = vec![PackedStyle::new(
        appearance.foreground,
        appearance.background,
        None,
        0,
        UnderlineStyle::None,
    )];
    for attributes in [
        ATTR_BOLD,
        ATTR_ITALIC,
        ATTR_BOLD | ATTR_ITALIC,
        ATTR_FAINT,
        ATTR_INVISIBLE,
        ATTR_STRIKETHROUGH,
        ATTR_OVERLINE,
        ATTR_HYPERLINK,
    ] {
        styles.push(PackedStyle::new(
            appearance.foreground,
            appearance.background,
            None,
            attributes,
            UnderlineStyle::None,
        ));
    }
    for underline in [
        UnderlineStyle::Single,
        UnderlineStyle::Double,
        UnderlineStyle::Curly,
        UnderlineStyle::Dotted,
        UnderlineStyle::Dashed,
    ] {
        styles.push(PackedStyle::new(
            appearance.foreground,
            appearance.background,
            Some(Color::rgb(231, 81, 113)),
            0,
            underline,
        ));
    }
    styles.push(PackedStyle::new(
        appearance.background,
        appearance.foreground,
        None,
        0,
        UnderlineStyle::None,
    ));
    styles.push(PackedStyle::new(
        Color::rgb(35, 49, 62),
        appearance.background,
        None,
        0,
        UnderlineStyle::None,
    ));
    let mut graphemes = Vec::new();
    let mut offsets = vec![0];
    let mut fallback_paths = Vec::new();
    let mut marked = None;
    let retained = case.starts_with("composition-retained");
    let sub_row = if case == "composition-retained-fractional" {
        0.5
    } else {
        0.0
    };
    match case {
        "regular-only" | "full-faces" | "mixed-faces" | "no-synthetic" => {
            for row in 0..4 {
                put(
                    &mut viewport,
                    row,
                    0,
                    "!= => -> == <= >= ffi AV jg()",
                    row as u16,
                );
                put(
                    &mut viewport,
                    row + 4,
                    0,
                    "selection bold italic bitmap",
                    row as u16,
                );
            }
            viewport.overlays = (4..8)
                .map(|row| OverlaySpan::new(row, 0, 14, OverlayKind::Selection))
                .collect::<Vec<_>>()
                .into();
        }
        "decorations" | "contrast" => {
            for row in 0..8 {
                put(
                    &mut viewport,
                    row,
                    0,
                    "Agj qyp 0123456789",
                    (row + 4) as u16,
                );
                put(&mut viewport, row, 23, "AV ffi -> <=", (row + 8) as u16);
            }
            viewport.overlays = Arc::from([
                OverlaySpan::new(0, 0, 6, OverlayKind::Selection),
                OverlaySpan::new(2, 23, 30, OverlayKind::Selection),
            ]);
        }
        "glyphs" => {
            let fallback = [
                ("DejaVu Sans Mono", dejavu.join("DejaVuSansMono.ttf")),
                (
                    "Noto Sans Hebrew",
                    PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf"),
                ),
                (
                    "Noto Sans Arabic",
                    PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf"),
                ),
                (
                    "Noto Sans Devanagari",
                    PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf"),
                ),
                (
                    "Noto Sans Thai",
                    PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf"),
                ),
                (
                    "Noto Sans CJK JP",
                    PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
                ),
            ];
            for (name, path) in fallback {
                appearance.font_families.push(name.into());
                files.push(path.clone());
                fallback_paths.push(path);
            }
            put(&mut viewport, 0, 0, "ASCII != ffi a b c", 0);
            put(&mut viewport, 1, 0, "─│┌┐└┘├┤┬┴┼╭╮╰╯━┃┏┓┗┛", 0);
            put(&mut viewport, 2, 0, "█▀▄▌▐░▒▓▁▂▃▄▅▆▇▕▏", 0);
            put(&mut viewport, 3, 0, "αβγ Жизнь שלום مرحبا", 0);
            for (row, text) in ["a\u{301}", "कित", "ก้", "界", "क्ष", "a\u{308}\u{301}"]
                .into_iter()
                .enumerate()
            {
                let row = row / 3 + 4;
                let column = (offsets.len() - 1) % 3 * 12;
                put(&mut viewport, row, column, "[", 0);
                Arc::make_mut(&mut viewport.cells)[row * COLUMNS + column + 1] = PackedCell::new(
                    GRAPHEME_TABLE_BIT | (offsets.len() - 1) as u32,
                    0,
                    CellWidth::Wide,
                );
                Arc::make_mut(&mut viewport.cells)[row * COLUMNS + column + 2] =
                    PackedCell::new(0, 0, CellWidth::SpacerTail);
                put(&mut viewport, row, column + 3, "]", 0);
                graphemes.extend_from_slice(text.as_bytes());
                offsets.push(graphemes.len() as u32);
            }
            for (index, value) in "界中語".chars().enumerate() {
                Arc::make_mut(&mut viewport.cells)[6 * COLUMNS + index * 4] =
                    PackedCell::new(u32::from(value), 0, CellWidth::Wide);
                Arc::make_mut(&mut viewport.cells)[6 * COLUMNS + index * 4 + 1] =
                    PackedCell::new(0, 0, CellWidth::SpacerTail);
            }
            Arc::make_mut(&mut viewport.cells)[7 * COLUMNS] =
                PackedCell::new(0, 0, CellWidth::SpacerHead);
            put(&mut viewport, 7, 1, "after spacer head", 0);
        }
        "composition"
        | "composition-tail"
        | "composition-clipped"
        | "composition-no-cursor"
        | "composition-hidden-cursor"
        | "composition-decorated"
        | "composition-boundary"
        | "composition-retained"
        | "composition-retained-fractional" => {
            for row in 0..8 {
                put(&mut viewport, row, 0, "retained cells and composition", 0);
            }
            marked = Some("compose ffi".to_owned());
            viewport.overlays = Arc::from([OverlaySpan::new(4, 0, 20, OverlayKind::Selection)]);
            viewport.cursor = Some(Cursor::new(
                5,
                4,
                true,
                false,
                false,
                CursorStyle::Block,
                Color::rgb(37, 171, 213),
            ));
            match case {
                "composition-tail" => {
                    Arc::make_mut(&mut viewport.cells)[4 * COLUMNS + 5] =
                        PackedCell::new(u32::from('W'), 0, CellWidth::Wide);
                    Arc::make_mut(&mut viewport.cells)[4 * COLUMNS + 6] =
                        PackedCell::new(0, 0, CellWidth::SpacerTail);
                    viewport.cursor = Some(Cursor::new(
                        6,
                        4,
                        true,
                        false,
                        false,
                        CursorStyle::Block,
                        Color::rgb(37, 171, 213),
                    ));
                }
                "composition-clipped" => {
                    viewport.cursor = Some(Cursor::new(
                        47,
                        4,
                        true,
                        false,
                        false,
                        CursorStyle::Block,
                        Color::rgb(37, 171, 213),
                    ));
                }
                "composition-no-cursor" => viewport.cursor = None,
                "composition-hidden-cursor" => {
                    viewport.cursor = Some(Cursor::new(
                        5,
                        12,
                        true,
                        false,
                        false,
                        CursorStyle::Block,
                        Color::rgb(37, 171, 213),
                    ));
                }
                "composition-decorated" => {
                    styles.push(PackedStyle::new(
                        appearance.foreground,
                        appearance.background,
                        Some(Color::rgb(231, 81, 113)),
                        ATTR_STRIKETHROUGH | ATTR_OVERLINE,
                        UnderlineStyle::Curly,
                    ));
                    put(
                        &mut viewport,
                        4,
                        0,
                        "retained ffi != => cells and input",
                        (styles.len() - 1) as u16,
                    );
                }
                "composition-boundary" => {
                    put(
                        &mut viewport,
                        4,
                        0,
                        "=>===->!= <= ffi and retained cells",
                        9,
                    );
                    viewport.cursor = Some(Cursor::new(
                        2,
                        4,
                        true,
                        false,
                        false,
                        CursorStyle::Block,
                        Color::rgb(37, 171, 213),
                    ));
                }
                _ => {}
            }
        }
        _ => return Err("unknown styled fixture case".into()),
    }
    viewport.dictionary = Arc::new(TerminalDictionary::from_shared(
        styles.into(),
        offsets.into(),
        graphemes.into(),
    ));
    let physical_width = (384.0 * scale).round() as i32;
    let physical_height = (160.0 * scale).round() as i32;
    fs::create_dir_all(&directory)?;
    let text_system = Arc::new(CosmicTextSystem::new_without_system_fonts(family));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(penna::Assets), move || {
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
    for path in &files {
        cx.text_system()
            .add_fonts(vec![Cow::Owned(fs::read(path)?)])?;
    }
    cx.update(penna::init);
    let viewport = Arc::new(viewport);
    let input = json!({
        "case": case, "appearance": appearance, "columns": viewport.columns, "rows": viewport.rows,
        "cells": viewport.cells.iter().map(|cell| [cell.glyph(), u32::from(cell.style_id()), u32::from(cell.flags())]).collect::<Vec<_>>(),
        "styles": viewport.dictionary.styles.iter().map(|style| [style.foreground_raw(), style.background_raw(), style.underline_color_raw(), u32::from(style.attributes()), u32::from(style.underline_kind_raw()), 0]).collect::<Vec<_>>(),
        "grapheme_offsets": viewport.dictionary.grapheme_offsets, "grapheme_bytes": viewport.dictionary.grapheme_bytes,
        "overlays": viewport.overlays.iter().map(|span| [span.row, span.start, span.end, span.kind_and_flags()]).collect::<Vec<_>>(),
        "cursor": viewport.cursor.map(|cursor| json!([cursor.color().packed(), cursor.column(), cursor.row(), match cursor.style() { CursorStyle::Bar => 0, CursorStyle::Block => 1, CursorStyle::Underline => 2, CursorStyle::BlockHollow => 3 }, u8::from(cursor.visible()), u8::from(cursor.blinking()), u8::from(cursor.at_wide_tail())])),
        "font_files": files, "style_paths": style_paths, "fallback_paths": fallback_paths, "marked": marked,
        "font_size": font_size, "scale": scale, "width": physical_width, "height": physical_height, "sub_row": sub_row, "retained": retained,
    });
    let geometry = Rc::new(RefCell::new(Value::Null));
    let output = geometry.clone();
    let window = cx.open_window(size(px(384.), px(160.)), |window, cx| {
        window.set_scale_factor(scale);
        cx.new(|_| Fixture {
            viewport,
            appearance,
            cache: Rc::new(RefCell::new(RowRenderCache::default())),
            geometry,
            font_size,
            marked,
            retained,
            sub_row,
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
