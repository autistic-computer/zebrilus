use std::{
    error::Error,
    io::{self, BufWriter, Write},
    sync::Arc,
};

use ala::{Bounds, point, px, size};
use zz_terminal::{
    AppearanceColor, Color, Cursor, CursorStyle, OverlayKind, OverlaySpan, SessionStatus,
    TerminalAppearance, TerminalViewport,
};
use zz_ui::terminal::{
    copy_cursor_line_color, cursor_quad, cursor_visible_for_paint, overlay_color, selection_corners,
};

fn number(output: &mut impl Write, value: u32) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    let ranges = [(0, 0), (0, 2), (2, 4), (0, 8), (4, 8), (8, 2)];
    for row in [0_u16, 1, 65_534, 65_535] {
        for (start, end) in [(0, 0), (0, 4), (2, 5), (2, 8), (65_534, 65_535)] {
            for (above_start, above_end) in ranges {
                for (below_start, below_end) in ranges {
                    for rectangle in [0, 1] {
                        let mut spans =
                            vec![OverlaySpan::new(row, start, end, OverlayKind::Selection)];
                        if let Some(above) = row.checked_sub(1) {
                            spans.push(OverlaySpan::with_flags(
                                above,
                                above_start,
                                above_end,
                                OverlayKind::Selection,
                                rectangle,
                            ));
                            spans.push(OverlaySpan::new(
                                above,
                                0,
                                u16::MAX,
                                OverlayKind::SearchMatch,
                            ));
                        }
                        if let Some(below) = row.checked_add(1) {
                            spans.push(OverlaySpan::with_flags(
                                below,
                                below_start,
                                below_end,
                                OverlayKind::Selection,
                                rectangle,
                            ));
                        }
                        cases.push(spans);
                    }
                }
            }
        }
    }
    let mut seed = 0x3456_789A_u32;
    for _ in 0..512 {
        let mut spans = Vec::new();
        for _ in 0..16 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let row = u16::try_from(seed % 5)?;
            let start = u16::try_from((seed >> 4) % 20)?;
            let end = u16::try_from((seed >> 12) % 20)?;
            spans.push(OverlaySpan::with_flags(
                row,
                start,
                end,
                OverlayKind::Selection,
                u8::try_from((seed >> 20) & 1)?,
            ));
        }
        cases.push(spans);
    }
    let mut output = BufWriter::new(io::stdout().lock());
    number(&mut output, 0x4F56_5A5A)?;
    number(&mut output, u32::try_from(cases.len())?)?;
    number(&mut output, 256 * 6)?;
    number(&mut output, 64)?;
    number(&mut output, 4 * 2 * 5 * 4)?;
    for spans in cases {
        let mut viewport = TerminalViewport::blank(0, 0, SessionStatus::Running);
        viewport.overlays = Arc::from(spans);
        number(&mut output, u32::try_from(viewport.overlays.len())?)?;
        for span in viewport.overlays.iter() {
            for value in [span.row, span.start, span.end, span.kind_and_flags()] {
                number(&mut output, u32::from(value))?;
            }
        }
        for index in 0..viewport.overlays.len() {
            let corners = selection_corners(&viewport, index, px(3.125));
            for value in [
                corners.top_left,
                corners.top_right,
                corners.bottom_right,
                corners.bottom_left,
            ] {
                number(&mut output, f32::from(value).to_bits())?;
            }
        }
    }
    for alpha in 0_u8..=255 {
        let color = AppearanceColor::rgba(37, alpha, 213, alpha);
        let appearance = TerminalAppearance {
            selection_background: color,
            search_match_color: color,
            search_current_color: color,
            copy_cursor_color: color,
            link_color: color.rgb(),
            ..TerminalAppearance::default()
        };
        let packed = u32::from(color.r) << 24
            | u32::from(color.g) << 16
            | u32::from(color.b) << 8
            | u32::from(color.a);
        for (kind, hsla) in [
            (0, overlay_color(OverlayKind::Selection, &appearance)),
            (1, overlay_color(OverlayKind::SearchMatch, &appearance)),
            (2, overlay_color(OverlayKind::SearchCurrent, &appearance)),
            (3, overlay_color(OverlayKind::LinkHover, &appearance)),
            (4, overlay_color(OverlayKind::CopyCursor, &appearance)),
            (5, copy_cursor_line_color(&appearance)),
        ] {
            number(&mut output, packed)?;
            number(&mut output, kind)?;
            for value in [hsla.h, hsla.s, hsla.l, hsla.a] {
                number(&mut output, value.to_bits())?;
            }
        }
    }
    for flags in 0..64 {
        let cursor = (flags & 1 != 0).then(|| {
            Cursor::new(
                0,
                0,
                flags & 2 != 0,
                flags & 4 != 0,
                false,
                CursorStyle::Block,
                Color::rgb(0, 0, 0),
            )
        });
        number(&mut output, flags)?;
        number(
            &mut output,
            u32::from(cursor_visible_for_paint(
                cursor,
                flags & 16 != 0,
                flags & 8 != 0,
                flags & 32 != 0,
            )),
        )?;
    }
    for (style_index, style) in [
        CursorStyle::Bar,
        CursorStyle::Block,
        CursorStyle::Underline,
        CursorStyle::BlockHollow,
    ]
    .into_iter()
    .enumerate()
    {
        for focused in [false, true] {
            for (x, y, width, height) in [
                (0.0, 0.0, 8.0, 18.0),
                (1.25, 2.75, 16.0, 21.0),
                (-4.5, -3.25, 9.0, 12.0),
                (17.5, 18.5, 6.0, 9.0),
                (0.0, 0.0, 8.0, 1.0),
            ] {
                for packed in [0x0000_0000, 0x00FF_FFFF, 0x0025_ABD5, 0x00E5_3F99] {
                    let color = Color::rgb(
                        u8::try_from((packed >> 16) & 255)?,
                        u8::try_from((packed >> 8) & 255)?,
                        u8::try_from(packed & 255)?,
                    );
                    for value in [x, y, width, height] {
                        number(&mut output, f32::to_bits(value))?;
                    }
                    number(&mut output, u32::try_from(style_index)?)?;
                    number(&mut output, packed)?;
                    number(&mut output, u32::from(focused))?;
                    let q = cursor_quad(
                        Bounds::new(point(px(x), px(y)), size(px(width), px(height))),
                        style,
                        color,
                        focused,
                        1.0,
                    );
                    for value in [
                        q.bounds.origin.x,
                        q.bounds.origin.y,
                        q.bounds.size.width,
                        q.bounds.size.height,
                        q.border_widths.top,
                    ] {
                        number(&mut output, f32::from(value).to_bits())?;
                    }
                    for hsla in [
                        q.background
                            .as_solid()
                            .ok_or("non-solid cursor background")?,
                        q.border_color,
                    ] {
                        for value in [hsla.h, hsla.s, hsla.l, hsla.a] {
                            number(&mut output, value.to_bits())?;
                        }
                    }
                }
            }
        }
    }
    output.flush()?;
    Ok(())
}
