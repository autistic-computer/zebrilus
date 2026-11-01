use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::svg::{esc, num};

const SHAPE_PROPS: &[&str] = &[
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-opacity",
    "opacity",
    "rx",
    "ry",
];

const TEXT_PROPS: &[&str] = &[
    "color",
    "font-size",
    "font-family",
    "font-weight",
    "font-style",
    "text-decoration",
    "letter-spacing",
    "word-spacing",
    "opacity",
];

static RE_UNSAFE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)url\s*\(|expression\s*\(|javascript:|[<>{};\\]|@import|image-set|/\*").unwrap()
});
static RE_INVALID_XML: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f\u{fffe}\u{ffff}]").unwrap());
static RE_IMPORTANT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)!important").unwrap());
static RE_PX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9.]+(px)?$").unwrap());
static RE_SETS_FILL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[";]fill:"#).unwrap());
static RE_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Za-z0-9_-]+").unwrap());

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedStyle {
    pub shape: String,
    pub line: String,
    pub text: String,
    pub stroke: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub font_size: Option<f64>,
}

pub fn parse_float(value: &str) -> f64 {
    let mut end = 0;
    let mut dot = false;
    for (at, ch) in value.char_indices() {
        match ch {
            '0'..='9' => end = at + 1,
            '.' if !dot => {
                dot = true;
                end = at + 1;
            }
            _ => break,
        }
    }
    value[..end]
        .trim_end_matches('.')
        .parse()
        .unwrap_or(f64::NAN)
}

pub fn resolve_style<S: AsRef<str>>(declarations: &[S]) -> ResolvedStyle {
    let mut shape = IndexMap::<String, String>::new();
    let mut text = IndexMap::<String, String>::new();
    for decl in declarations {
        let decl = decl.as_ref();
        let Some(at) = decl.find(':').filter(|&at| at > 0) else {
            continue;
        };
        let prop = decl[..at].trim().to_lowercase();
        let value = RE_IMPORTANT.replace_all(&decl[at + 1..], "");
        let value = value.trim();
        if value.is_empty() || RE_UNSAFE.is_match(value) || RE_INVALID_XML.is_match(value) {
            continue;
        }
        if SHAPE_PROPS.contains(&prop.as_str()) {
            shape.insert(prop.clone(), value.to_owned());
        }
        if TEXT_PROPS.contains(&prop.as_str()) {
            let key = if prop == "color" {
                "fill".to_owned()
            } else {
                prop
            };
            text.insert(key, value.to_owned());
        }
    }
    if shape.is_empty() && text.is_empty() {
        return ResolvedStyle::default();
    }
    let mut shape_css = String::new();
    let mut line_css = String::new();
    for (prop, value) in &shape {
        shape_css.push_str(&format!("{prop}:{value};"));
        if prop != "fill" && prop != "fill-opacity" {
            line_css.push_str(&format!("{prop}:{value};"));
        }
    }
    let text_css: String = text
        .iter()
        .map(|(prop, value)| format!("{prop}:{value};"))
        .collect();
    let weight = text.get("font-weight");
    let px = text
        .get("font-size")
        .filter(|size| RE_PX.is_match(size))
        .map(|size| parse_float(size));
    let wrap = |css: &str| {
        if css.is_empty() {
            String::new()
        } else {
            format!(" style=\"{}\"", esc(css))
        }
    };
    ResolvedStyle {
        shape: wrap(&shape_css),
        line: wrap(&line_css),
        text: wrap(&text_css),
        stroke: shape.get("stroke").cloned(),
        bold: weight.is_some_and(|w| {
            w == "bold" || w == "bolder" || w.trim().parse::<f64>().is_ok_and(|n| n >= 600.0)
        }),
        italic: text.get("font-style").is_some_and(|s| s == "italic"),
        font_size: px.filter(|px| *px > 0.0 && *px <= 200.0),
    }
}

pub fn class_names(raw: &str) -> String {
    let cleaned = RE_CLASS.replace_all(raw, " ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        String::new()
    } else {
        format!(" {cleaned}")
    }
}

pub fn sets_fill(shape: &str) -> bool {
    RE_SETS_FILL.is_match(shape)
}

pub fn rounded_attr(radius: f64) -> String {
    format!(" rx=\"{}\"", num(radius))
}

pub fn rounded(attrs: &str, radius: f64) -> String {
    format!("{attrs}{}", rounded_attr(radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_split_into_shape_line_and_text() {
        let style = resolve_style(&[
            "fill:#f00",
            "stroke:#00f !important",
            "color:#fff",
            "font-weight:700",
            "font-size:20px",
            "fill:#0f0",
            "background:url(x)",
            "stroke-width:url(javascript:alert(1))",
        ]);
        assert_eq!(style.shape, " style=\"fill:#0f0;stroke:#00f;\"");
        assert_eq!(style.line, " style=\"stroke:#00f;\"");
        assert_eq!(
            style.text,
            " style=\"fill:#fff;font-weight:700;font-size:20px;\""
        );
        assert_eq!(style.stroke.as_deref(), Some("#00f"));
        assert!(style.bold);
        assert_eq!(style.font_size, Some(20.0));
        assert_eq!(resolve_style::<&str>(&[]), ResolvedStyle::default());
    }

    #[test]
    fn class_names_keep_word_characters() {
        assert_eq!(class_names("default  hot,cold"), " default hot cold");
        assert_eq!(class_names("  "), "");
        assert!(sets_fill(" style=\"stroke:#000;fill:#fff;\""));
        assert!(!sets_fill(" style=\"stroke:#000;fill-opacity:1;\""));
        assert_eq!(rounded(" fill=\"#fff\"", 4.0), " fill=\"#fff\" rx=\"4\"");
    }
}
