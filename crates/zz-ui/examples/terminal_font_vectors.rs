use std::error::Error;

use ala::FontStyle;
use serde_json::{Value, json};
use zz_terminal::{FontFeature, TerminalAppearance};
use zz_ui::terminal::terminal_font_for_style;

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::<Value>::new();
    for weight in [1, 100, 400, 600, 900, 1000] {
        for configured in 0..3 {
            for features in 0..3 {
                let appearance = TerminalAppearance {
                    font_weight: weight,
                    font_families: if configured == 0 {
                        Vec::new()
                    } else {
                        vec!["Primary".to_owned(), "Second 世界".to_owned()]
                    },
                    font_families_bold: if configured == 2 {
                        vec!["Bold".to_owned(), "BoldFallback".to_owned()]
                    } else {
                        Vec::new()
                    },
                    font_families_italic: if configured == 2 {
                        vec!["Italic".to_owned()]
                    } else {
                        Vec::new()
                    },
                    font_families_bold_italic: if configured == 2 {
                        vec!["BoldItalic".to_owned(), "BI1".to_owned(), "BI2".to_owned()]
                    } else {
                        Vec::new()
                    },
                    font_features: match features {
                        1 => vec![
                            FontFeature::new(*b"liga", 0),
                            FontFeature::new(*b"ss01", u32::MAX),
                        ],
                        2 => vec![
                            FontFeature::new(*b" A9z", 7),
                            FontFeature::new(*b"liga", 1),
                            FontFeature::new(*b"liga", 0),
                        ],
                        _ => Vec::new(),
                    },
                    ..TerminalAppearance::default()
                };
                let context = json!({"families":[appearance.font_families,appearance.font_families_bold,appearance.font_families_italic,appearance.font_families_bold_italic],"weight":weight,"features":appearance.font_features});
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    let font = terminal_font_for_style(&appearance, "FallbackMono", bold, italic);
                    let request = json!({
                        "family":font.family.to_string(),
                        "fallbacks":font.fallbacks.map_or_else(Vec::new, |value| value.0.as_ref().clone()),
                        "features":font.features.0.as_ref(),
                        "weight":font.weight.0,
                        "italic":matches!(font.style,FontStyle::Italic),
                    });
                    cases.push(
                        json!({"context":context,"bold":bold,"italic":italic,"expected":request}),
                    );
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
