use std::{borrow::Cow, collections::BTreeSet, error::Error, fs, path::PathBuf, sync::Arc};

use ala::{HeadlessAppContext, Hsla, TextRun, TruncateFrom, font, px};
use ala_platform::wgpu::CosmicTextSystem;
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("truncation output required")?;
    let app = HeadlessAppContext::new(Arc::new(CosmicTextSystem::new_without_system_fonts(
        "Inter",
    )));
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mjui/bench/assets/fonts");
    for name in ["Inter-Regular.ttf", "JetBrainsMono-Regular.ttf"] {
        app.text_system()
            .add_fonts(vec![Cow::Owned(fs::read(directory.join(name))?)])?;
    }
    let mut texts: Vec<String> = [
        "",
        "Project 0 — terminal",
        "0 windows · running",
        "3 windows · running",
        "A custom row format replaces both original fields",
        "Pr…",
        "office affine AV To",
        "word...",
        "word !!! later",
        " leading and trailing ",
        "a\0b",
        "a\u{1c}b",
        "a\u{a0}b",
        "a\u{2003}b",
        "a\u{202f}b",
        "a\u{3000}b",
        "a\u{85}b",
        "a\u{2028}b",
        "a\u{2029}b",
        "a\u{301}bc",
        "éclair Ελληνικά Кириллица",
        "\tword\tmore",
        "text/with-path__end",
        "(parenthesized) [indexed]",
        "quote\" or apostrophe'",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for point in 0..=32 {
        texts.push(format!(
            "abc{} def",
            char::from_u32(point).ok_or("invalid scalar")?
        ));
    }
    let mut cases = Vec::new();
    let scalars: BTreeSet<_> = texts.iter().flat_map(|text| text.chars()).collect();
    let mut characters = Vec::new();
    for (role, family) in ["Inter", "JetBrains Mono"].into_iter().enumerate() {
        for size in [10.0_f32, 11., 13.] {
            let source_font = font(family);
            let id = app.text_system().resolve_font(&source_font);
            for character in &scalars {
                let width = f32::from(app.text_system().layout_width(id, px(size), *character));
                characters.push(json!({"font":role,"size":size.to_bits(),"scalar":u32::from(*character),"expected":width.to_bits().to_string()}));
            }
            let mut wrapper = app
                .text_system()
                .line_wrapper(source_font.clone(), px(size));
            for text in &texts {
                let runs = [TextRun {
                    len: text.len(),
                    font: source_font.clone(),
                    color: Hsla::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                let full = text.chars().fold(0.0_f32, |sum, character| {
                    sum + f32::from(app.text_system().layout_width(id, px(size), character))
                });
                let widths = [
                    0.,
                    0.5,
                    1.,
                    2.,
                    4.,
                    8.,
                    12.,
                    16.,
                    24.,
                    30.8,
                    31.,
                    40.,
                    64.,
                    82.,
                    103.2,
                    128.,
                    256.,
                    512.,
                    full,
                    full + 0.001,
                    (full - 0.001).max(0.),
                ];
                for width in widths {
                    let (result, _) = wrapper.truncate_line(
                        text.clone().into(),
                        px(width),
                        "…",
                        &runs,
                        TruncateFrom::End,
                    );
                    cases.push(json!({"font":role,"size":size.to_bits(),"width":width.to_bits(),"text":text,"expected":result.as_ref()}));
                }
            }
        }
    }
    fs::write(
        PathBuf::from(&output).with_extension("chars.json"),
        serde_json::to_vec(&characters)?,
    )?;
    fs::write(output, serde_json::to_vec(&cases)?)?;
    Ok(())
}
