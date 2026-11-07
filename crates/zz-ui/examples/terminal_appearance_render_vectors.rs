use std::{
    error::Error,
    io::{self, BufWriter, Write},
};

use ala::{Hsla, Rgba, px, size};
use zz_client::terminal_view::cursor_should_blink;
use zz_terminal::{CellHeightAdjustment, Color, Cursor, CursorBlinkPolicy, CursorStyle};
use zz_ui::terminal::terminal_grid_size;

fn number(output: &mut impl Write, value: u32) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = BufWriter::new(io::stdout().lock());
    let bases = [0.0_f32, 0.25, 1.0, 13.25, 16.625, 32.0, 256.0];
    let values = [
        0.0_f32, -0.0, 1.25, -1.0, -100.0, -256.0, 1024.0, 33.333_332, -99.999_99,
    ];
    let measurements = [
        (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32),
        (0.0, 10.0, 3.0, 1.0),
        (0.999_999_94, 10.25, 2.75, 0.0),
        (8.5, 11.125, 2.875, 0.5),
        (7.499_999_5, 12.3, 3.7, 1.0),
        (7.5, 12.3, 3.7, 1.0),
        (7.500_000_5, 12.3, 3.7, 1.0),
    ];
    let scales = [0.5_f32, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0];
    number(&mut output, 0x4150_5A5A)?;
    number(&mut output, 65_536)?;
    number(&mut output, u32::try_from(bases.len() * values.len() * 3)?)?;
    number(&mut output, 48)?;
    number(
        &mut output,
        u32::try_from(measurements.len() * values.len() * scales.len() * 3)?,
    )?;
    let mut seed = 0x1234_5678_u32;
    for index in 0_u32..65_536 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let packed = if index < 256 {
            (index * 0x0101_0100) | 255
        } else {
            seed & 0xFFFF_FF00 | index & 255
        };
        let rgba = Rgba {
            r: (packed >> 24) as f32 / 255.0,
            g: ((packed >> 16) & 255) as f32 / 255.0,
            b: ((packed >> 8) & 255) as f32 / 255.0,
            a: (packed & 255) as f32 / 255.0,
        };
        let hsla = Hsla::from(rgba);
        number(&mut output, packed)?;
        for value in [
            rgba.r, rgba.g, rgba.b, rgba.a, hsla.h, hsla.s, hsla.l, hsla.a,
        ] {
            number(&mut output, value.to_bits())?;
        }
    }
    for base in bases {
        for value in values {
            for (kind, adjustment) in [
                (0, CellHeightAdjustment::None),
                (1, CellHeightAdjustment::Pixels(value)),
                (2, CellHeightAdjustment::Percent(value)),
            ] {
                number(&mut output, kind)?;
                number(&mut output, base.to_bits())?;
                number(&mut output, value.to_bits())?;
                number(&mut output, adjustment.apply(base).to_bits())?;
            }
        }
    }
    for flags in 0..16 {
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
        for (kind, policy) in [
            (0, CursorBlinkPolicy::Off),
            (1, CursorBlinkPolicy::On),
            (2, CursorBlinkPolicy::Terminal),
        ] {
            number(&mut output, flags)?;
            number(&mut output, kind)?;
            number(
                &mut output,
                u32::from(cursor_should_blink(cursor, policy, flags & 8 != 0)),
            )?;
        }
    }
    for (advance, ascent, descent, line_gap) in measurements {
        for scale in scales {
            for value in values {
                for (kind, adjustment) in [
                    (0, CellHeightAdjustment::None),
                    (1, CellHeightAdjustment::Pixels(value)),
                    (2, CellHeightAdjustment::Percent(value)),
                ] {
                    let pixel = 1.0 / scale;
                    let raw_width = if advance > 1.0 { advance } else { 8.0 };
                    let natural = (ascent + descent + line_gap).max(1.0);
                    let raw_height = adjustment.apply(natural).max(1.0);
                    let physical = terminal_grid_size(
                        size(px(317.25), px(211.875)),
                        px(raw_width.max(pixel)),
                        px(raw_height.max(pixel)),
                        scale,
                    );
                    let width = ((physical.cell_width_px as f32) / scale).max(pixel);
                    let height = ((physical.cell_height_px as f32) / scale).max(pixel);
                    let grid = terminal_grid_size(
                        size(px(317.25), px(211.875)),
                        px(width),
                        px(height),
                        scale,
                    );
                    let baseline = (height - ascent - descent) / 2.0 + ascent;
                    for input in [advance, ascent, descent, line_gap, scale, value] {
                        number(&mut output, input.to_bits())?;
                    }
                    number(&mut output, kind)?;
                    for expected in [width, height, baseline] {
                        number(&mut output, expected.to_bits())?;
                    }
                    for expected in [
                        u32::from(grid.columns),
                        u32::from(grid.rows),
                        grid.cell_width_px,
                        grid.cell_height_px,
                    ] {
                        number(&mut output, expected)?;
                    }
                }
            }
        }
    }
    output.flush()?;
    Ok(())
}
