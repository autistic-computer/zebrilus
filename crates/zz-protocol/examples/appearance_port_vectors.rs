use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    Event, EventPayload, ProtocolMessage, decode_protocol_frame, encode_protocol_message,
};
use zz_terminal::{
    AppearanceColor, AppearanceConfigKey, AppearanceProvenance, AppearanceSource,
    CellHeightAdjustment, Color, CursorBlinkPolicy, CursorStyle, FontFeature, FontSyntheticStyle,
    TerminalAppearance, TerminalColorScheme, TerminalPalette,
};

fn raw_fixture(name: &str, mut bytes: Vec<u8>) -> Result<Value, Box<dyn Error>> {
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    let canonical = decode_protocol_frame(&bytes)
        .ok()
        .map(|decoded| encode_protocol_message(&decoded))
        .transpose()?;
    Ok(json!({"name":name,"bytes":bytes,"canonical":canonical,"accepted":canonical.is_some()}))
}

fn fixture(name: &str, appearance: TerminalAppearance) -> Result<Value, Box<dyn Error>> {
    let mut provenance = AppearanceProvenance::default();
    for (index, key) in AppearanceConfigKey::ALL.into_iter().enumerate() {
        let source = match index % 4 {
            1 => AppearanceSource::ThemeFile,
            2 => AppearanceSource::Ghostty,
            3 => AppearanceSource::Override,
            _ => AppearanceSource::Default,
        };
        if name != "default" {
            provenance.set_source(key, source);
        }
    }
    let message = ProtocolMessage::Event(Event {
        sequence: u64::MAX,
        payload: EventPayload::AppearanceChanged {
            appearance: Box::new(appearance),
            provenance,
        },
    });
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(&message)?);
    raw_fixture(name, bytes)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = vec![fixture("default", TerminalAppearance::default())?];
    for scheme in [TerminalColorScheme::Light, TerminalColorScheme::Dark] {
        for adjustment in [
            CellHeightAdjustment::None,
            CellHeightAdjustment::Pixels(-1024.0),
            CellHeightAdjustment::Percent(1024.0),
        ] {
            for blink in [
                CursorBlinkPolicy::Off,
                CursorBlinkPolicy::On,
                CursorBlinkPolicy::Terminal,
            ] {
                let appearance = TerminalAppearance {
                    color_scheme: scheme,
                    cell_height_adjustment: adjustment,
                    cursor_blink_policy: blink,
                    ..TerminalAppearance::default()
                };
                cases.push(fixture("enums", appearance)?);
            }
        }
    }
    let full = TerminalAppearance {
        font_families: vec!["界".repeat(85) + "x"; 32],
        font_families_bold: vec!["Bold\u{2028}".to_owned(); 32],
        font_families_italic: vec!["Italic\u{202e}".to_owned(); 32],
        font_families_bold_italic: vec!["Bold Italic".to_owned(); 32],
        font_size_points: 256.0,
        font_weight: 1000,
        font_features: vec![FontFeature::new(*b" A9z", u32::MAX); 64],
        font_synthetic_style: FontSyntheticStyle {
            bold: false,
            italic: true,
            bold_italic: false,
        },
        font_thicken: true,
        font_thicken_strength: 0,
        padding_left: 1024.0,
        padding_right: -0.0,
        padding_top: 1.125,
        padding_bottom: 1024.0,
        foreground: Color::rgb(1, 2, 3),
        background: Color::rgb(254, 253, 252),
        cursor_color: Color::rgb(17, 33, 65),
        cursor_style: CursorStyle::BlockHollow,
        selection_foreground: Color::rgb(20, 40, 60),
        selection_background: AppearanceColor::rgba(5, 6, 7, 8),
        search_match_color: AppearanceColor::rgba(9, 10, 11, 12),
        search_current_color: AppearanceColor::rgba(13, 14, 15, 16),
        link_color: Color::rgb(21, 22, 23),
        copy_cursor_color: AppearanceColor::rgba(24, 25, 26, 27),
        palette: TerminalPalette::new(std::array::from_fn(|index| {
            let byte = index.to_le_bytes()[0];
            Color::rgb(byte, 255 - byte, byte ^ 0x55)
        })),
        minimum_contrast: 21.0,
        cursor_blink_interval_ms: 60_000,
        rounded_selection: false,
        background_opacity: 0.0,
        ..TerminalAppearance::default()
    };
    cases.push(fixture("complete-boundary", full)?);
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.0,
        0.5,
        1.0,
        256.0,
        256.00003,
    ] {
        let appearance = TerminalAppearance {
            font_size_points: value,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-size", appearance)?);
    }
    for value in [
        f32::NAN,
        f32::INFINITY,
        -1024.001,
        -1024.0,
        -0.0,
        1024.0,
        1024.001,
    ] {
        let appearance = TerminalAppearance {
            cell_height_adjustment: CellHeightAdjustment::Pixels(value),
            ..TerminalAppearance::default()
        };
        cases.push(fixture("cell-height", appearance)?);
    }
    for value in [f32::NAN, f32::INFINITY, -0.01, -0.0, 0.0, 1024.0, 1024.001] {
        let appearance = TerminalAppearance {
            padding_left: value,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("padding", appearance)?);
    }
    for value in [f32::NAN, f32::INFINITY, 0.9999, 1.0, 21.0, 21.001] {
        let appearance = TerminalAppearance {
            minimum_contrast: value,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("contrast", appearance)?);
    }
    for value in [f32::NAN, f32::INFINITY, -0.0001, -0.0, 0.0, 1.0, 1.0001] {
        let appearance = TerminalAppearance {
            background_opacity: value,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("opacity", appearance)?);
    }
    for interval in [0, 49, 50, 60_000, 60_001, u32::MAX] {
        let appearance = TerminalAppearance {
            cursor_blink_interval_ms: interval,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("blink-interval", appearance)?);
    }
    for weight in [0, 1, 1000, 1001, u16::MAX] {
        let appearance = TerminalAppearance {
            font_weight: weight,
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-weight", appearance)?);
    }
    for family in [
        String::new(),
        "x".repeat(257),
        "nul\0family".to_owned(),
        "c1\u{80}".to_owned(),
        "c1\u{9f}".to_owned(),
        "valid\u{a0}".to_owned(),
    ] {
        let appearance = TerminalAppearance {
            font_families: vec![family],
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-family", appearance)?);
    }
    for count in [0, 32, 33] {
        let appearance = TerminalAppearance {
            font_families: vec!["Mono".to_owned(); count],
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-family-count", appearance)?);
    }
    for tag in [*b"a1 Z", [0, 1, 2, 3], [128, 255, 32, 32], *b"a-b_"] {
        let appearance = TerminalAppearance {
            font_features: vec![FontFeature::new(tag, 1)],
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-feature-tag", appearance)?);
    }
    for count in [64, 65] {
        let appearance = TerminalAppearance {
            font_features: vec![FontFeature::new(*b"liga", 1); count],
            ..TerminalAppearance::default()
        };
        cases.push(fixture("font-feature-count", appearance)?);
    }
    let base: Vec<u8> = serde_json::from_value(cases[0]["bytes"].clone())?;
    let provenance = base.len() - 61;
    if base[provenance] != 30 {
        return Err("appearance provenance wire layout changed".into());
    }
    let mut duplicate = base.clone();
    duplicate[provenance] = 31;
    duplicate.extend_from_slice(&[0, 3]);
    cases.push(raw_fixture("duplicate-provenance-last-wins", duplicate)?);
    let mut missing = base.clone();
    missing[provenance] = 29;
    missing.truncate(missing.len() - 2);
    cases.push(raw_fixture("missing-provenance-key", missing)?);
    let mut unknown = base.clone();
    unknown[provenance] = 31;
    unknown.extend_from_slice(&[30, 0]);
    cases.push(raw_fixture("unknown-provenance-key", unknown)?);
    let mut trailing = base.clone();
    trailing.extend_from_slice(&[0, 255, 128]);
    cases.push(raw_fixture("trailing-control-bytes", trailing)?);
    for cut in [
        8,
        9,
        10,
        19,
        20,
        21,
        22,
        24,
        31,
        63,
        127,
        255,
        512,
        base.len() - 63,
        base.len() - 1,
    ] {
        cases.push(raw_fixture("truncated-appearance", base[..cut].to_vec())?);
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
