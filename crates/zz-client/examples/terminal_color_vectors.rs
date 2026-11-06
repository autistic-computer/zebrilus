use std::{
    error::Error,
    io::{self, Write},
};

use zz_client::terminal_color::{
    contrast_ratio, ensure_minimum_contrast, faint_foreground, is_decorative_character,
    resolved_foreground,
};
use zz_terminal::{Color, PackedStyle};

fn append(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn resolution(
    output: &mut Vec<u8>,
    foreground: u32,
    background: u32,
    attributes: u16,
    minimum: f32,
    decorative: bool,
) {
    let style = PackedStyle::from_raw(foreground, background, u32::MAX, attributes, 0);
    for value in [
        foreground,
        background,
        u32::from(attributes),
        minimum.to_bits(),
        u32::from(decorative),
        resolved_foreground(style, minimum, decorative).packed(),
        ensure_minimum_contrast(style.foreground(), style.background(), minimum).packed(),
        faint_foreground(style, style.foreground()).packed(),
    ] {
        append(output, value);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut faint = Vec::new();
    for foreground in 0..=255_u32 {
        for background in 0..=255_u32 {
            let foreground = foreground * 0x0001_0101;
            let background = background * 0x0001_0101;
            let style = PackedStyle::from_raw(foreground, background, u32::MAX, 4, 0);
            for value in [
                foreground,
                background,
                4,
                faint_foreground(style, style.foreground()).packed(),
            ] {
                append(&mut faint, value);
            }
        }
    }
    let mut resolved = Vec::new();
    for foreground in 0..=255_u32 {
        for background in [0, 10, 11, 127, 128, 255_u32] {
            for minimum in [1.0, 4.5, 21.0] {
                for attributes in [0, 4] {
                    for decorative in [false, true] {
                        resolution(
                            &mut resolved,
                            foreground * 0x0001_0101,
                            background * 0x0001_0101,
                            attributes,
                            minimum,
                            decorative,
                        );
                    }
                }
            }
        }
    }
    let colors = [
        0x0000_0001,
        0x0001_0203,
        0x000A_0B0C,
        0x0010_1014,
        0x0028_2C34,
        0x00FF_0000,
        0x0000_FF00,
        0x0000_00FF,
        0x0000_FFFF,
        0x00FF_00FF,
        0x00FF_FF00,
        0x0080_8080,
        0x0077_7777,
        0x0076_7676,
        0x00AB_CDEF,
        0x00E0_6C75,
        0x0098_C379,
        0x0061_AFEF,
    ];
    for foreground in colors {
        for background in colors {
            let ratio = contrast_ratio(
                Color::from_packed(foreground),
                Color::from_packed(background),
            );
            for minimum in [
                1.0001,
                3.0,
                4.5,
                7.0,
                21.0,
                f32::from_bits(ratio.to_bits() - 1),
                ratio,
                f32::from_bits(ratio.to_bits() + 1),
            ] {
                for attributes in [0, 4, 0xFF84] {
                    resolution(
                        &mut resolved,
                        foreground | 0xD500_0000,
                        background,
                        attributes,
                        minimum,
                        false,
                    );
                }
            }
        }
    }
    let mut random = 0x7C4E_9B21_u32;
    for index in 0..4096 {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let foreground = random;
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let background = random & 0x00FF_FFFF;
        let minimum = [1.0, 1.0001, 3.0, 4.5, 7.0, 21.0][index % 6];
        resolution(
            &mut resolved,
            foreground,
            background,
            if index % 2 == 0 { 0 } else { 4 },
            minimum,
            index % 5 == 0,
        );
    }
    let mut decorative = Vec::new();
    for character in char::MIN..=char::MAX {
        append(&mut decorative, u32::from(character));
        append(
            &mut decorative,
            u32::from(is_decorative_character(character)),
        );
    }
    let mut output = io::BufWriter::new(io::stdout().lock());
    output.write_all(b"ZCLR")?;
    output.write_all(&u32::try_from(faint.len() / 16)?.to_le_bytes())?;
    output.write_all(&u32::try_from(resolved.len() / 32)?.to_le_bytes())?;
    output.write_all(&u32::try_from(decorative.len() / 8)?.to_le_bytes())?;
    output.write_all(&faint)?;
    output.write_all(&resolved)?;
    output.write_all(&decorative)?;
    output.flush()?;
    Ok(())
}
