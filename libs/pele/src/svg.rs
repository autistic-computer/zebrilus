pub mod cluster;
pub mod edges;
pub mod ink;
pub mod theme;
pub mod title;

use std::fmt::Write;

use crate::RenderOptions;
use crate::text::{self, BOLD, ITALIC, Label, MONO, SUB, SUP, UNDERLINE};

pub fn esc(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\u{0}'..='\u{8}'
            | '\u{b}'..='\u{c}'
            | '\u{e}'..='\u{1f}'
            | '\u{7f}'
            | '\u{fffe}'
            | '\u{ffff}' => {}
            _ => output.push(character),
        }
    }
    output
}

pub fn esc_text(text: &str) -> String {
    esc(&text::decode_entities(text))
}

pub fn num(value: f64) -> String {
    let scaled = value * 100.0;
    let floor = scaled.floor();
    let rounded = if scaled - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    };
    if rounded == 0.0 {
        return "0".to_owned();
    }
    if rounded.is_finite() && rounded.abs() < 1e11 {
        let magnitude = rounded.abs() as u64;
        let integer = magnitude / 100;
        let fraction = magnitude % 100;
        let sign = if rounded < 0.0 { "-" } else { "" };
        if fraction == 0 {
            format!("{sign}{integer}")
        } else if fraction.is_multiple_of(10) {
            format!("{sign}{integer}.{}", fraction / 10)
        } else {
            format!("{sign}{integer}.{fraction:02}")
        }
    } else if rounded.is_nan() {
        "NaN".to_owned()
    } else if rounded == f64::INFINITY {
        "Infinity".to_owned()
    } else if rounded == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else {
        let value = rounded / 100.0;
        if value.abs() >= 1e21 {
            format!("{value:e}").replace('e', "e+")
        } else {
            value.to_string()
        }
    }
}

pub fn document(
    kind: &str,
    width: f64,
    height: f64,
    options: &RenderOptions,
    content: &str,
) -> String {
    let width = num(width);
    let height = num(height);
    let prefix = esc(&options.id_prefix);
    let kind = esc(kind);
    let mut head = String::new();
    let mut accessibility = String::new();
    if let Some(title) = options.acc_title.as_ref().or(options.title.as_ref()) {
        let _ = write!(
            head,
            "<title id=\"{prefix}-title\">{}</title>",
            esc_text(title)
        );
        let _ = write!(accessibility, " aria-labelledby=\"{prefix}-title\"");
    }
    if let Some(description) = &options.acc_descr {
        let _ = write!(
            head,
            "<desc id=\"{prefix}-desc\">{}</desc>",
            esc_text(description)
        );
        let _ = write!(accessibility, " aria-describedby=\"{prefix}-desc\"");
    }
    let responsive = if options.responsive.unwrap_or(true) {
        " style=\"max-width:100%;height:auto\""
    } else {
        ""
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"pele pele-{kind}\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" color=\"{}\" role=\"graphics-document document\" aria-roledescription=\"{kind}\"{accessibility}{responsive}>{head}<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>{content}</svg>",
        esc(&options.font_family),
        num(options.font_size),
        esc(&options.theme.foreground),
        esc(&options.theme.foreground),
        esc(&options.theme.background),
    )
}

pub fn text(raw: &str, x: f64, y: f64, size: f64, anchor: &str, fill: &str) -> String {
    format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{}\" fill=\"{}\">{}</text>",
        num(x),
        num(y),
        num(size),
        esc(anchor),
        esc(fill),
        esc(raw),
    )
}

pub fn span_style(style: u8) -> String {
    span_attributes(style, None, "monospace")
}

fn span_attributes(style: u8, size: Option<f64>, mono_family: &str) -> String {
    let mut attributes = String::new();
    if style & BOLD != 0 {
        attributes.push_str(" font-weight=\"bold\"");
    }
    if style & ITALIC != 0 {
        attributes.push_str(" font-style=\"italic\"");
    }
    if style & MONO != 0 {
        let _ = write!(attributes, " font-family=\"{}\"", esc(mono_family));
    }
    if style & UNDERLINE != 0 {
        attributes.push_str(" text-decoration=\"underline\"");
    }
    if style & (SUB | SUP) != 0 {
        let shift = if style & SUB != 0 { "sub" } else { "super" };
        let _ = write!(
            attributes,
            " baseline-shift=\"{shift}\" font-size=\"{}\"",
            size.map_or_else(|| "83.333333%".to_owned(), |size| num(size / 1.2))
        );
    }
    attributes
}

pub fn shared_style(label: &Label) -> u8 {
    label
        .lines
        .iter()
        .flatten()
        .filter(|span| !span.text.is_empty())
        .map(|span| span.style)
        .reduce(|left, right| left & right)
        .unwrap_or(0)
        & (BOLD | ITALIC | MONO)
}

pub fn label(label: &Label, cx: f64, cy: f64, attributes: &str) -> String {
    label_inner(label, cx, cy, attributes, "monospace")
}

pub fn label_with_options(
    label: &Label,
    cx: f64,
    cy: f64,
    attributes: &str,
    options: &RenderOptions,
) -> String {
    label_inner(label, cx, cy, attributes, &options.font_family_mono)
}

fn label_inner(label: &Label, cx: f64, cy: f64, attributes: &str, mono_family: &str) -> String {
    if label.lines.is_empty() {
        return String::new();
    }
    let mut attributes = attributes.to_owned();
    if !attributes.contains(" font-size=\"") {
        let _ = write!(attributes, " font-size=\"{}\"", num(label.size));
    }
    let shared = shared_style(label);
    for (style, key, value) in [
        (BOLD, "font-weight", "bold"),
        (ITALIC, "font-style", "italic"),
        (MONO, "font-family", mono_family),
    ] {
        if shared & style != 0 && !attributes.contains(&format!(" {key}=\"")) {
            let _ = write!(attributes, " {key}=\"{}\"", esc(value));
        }
    }
    let shift = label.size * 0.35;
    let top = cy - label.height / 2.0;
    if label.lines.len() == 1 && label.lines[0].len() == 1 && label.lines[0][0].style == 0 {
        return format!(
            "<text{attributes} x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            num(cx),
            num(top + label.line_height / 2.0 + shift),
            esc(&label.lines[0][0].text)
        );
    }
    let mut body = String::new();
    for (index, line) in label.lines.iter().enumerate() {
        let y = top + (index as f64 + 0.5) * label.line_height + shift;
        if line.is_empty() {
            let _ = write!(body, "<tspan x=\"{}\" y=\"{}\"> </tspan>", num(cx), num(y));
        }
        for (position, span) in line.iter().enumerate() {
            let coordinate = if position == 0 {
                format!(" x=\"{}\" y=\"{}\"", num(cx), num(y))
            } else {
                String::new()
            };
            let _ = write!(
                body,
                "<tspan{coordinate}{}>{}</tspan>",
                span_attributes(span.style & !shared, Some(label.size), mono_family),
                esc(&span.text)
            );
        }
    }
    format!("<text{attributes} text-anchor=\"middle\" xml:space=\"preserve\">{body}</text>")
}

pub fn label_svg(label: &Label, cx: f64, cy: f64, attributes: &str) -> String {
    self::label(label, cx, cy, attributes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escaping_prevents_attribute_and_element_injection() {
        assert_eq!(
            esc("\"/><script>&\0\u{7f}"),
            "&quot;/&gt;&lt;script&gt;&amp;"
        );
        assert_eq!(esc_text("&lt;script&gt;"), "&lt;script&gt;");
    }

    #[test]
    fn coordinate_rounding_matches_javascript_negative_ties() {
        assert_eq!(num(-0.005), "0");
        assert_eq!(num(-1.125), "-1.12");
        assert_eq!(num(1.125), "1.13");
        assert_eq!(num(0.004999999999999999), "0");
        assert_eq!(num(12.0), "12");
        assert_eq!(num(12.2), "12.2");
        assert_eq!(num(1e21), "999999999999999900000");
        assert_eq!(num(1e22), "1e+22");
    }

    #[test]
    fn svg_output_has_concrete_colors_fonts_and_accessibility() {
        let options = RenderOptions {
            acc_title: Some("A & B".to_owned()),
            acc_descr: Some("<diagram>".to_owned()),
            ..RenderOptions::default()
        };
        let content = label(
            &text::label("<b>Hello</b><br/>world", 13.0, None),
            50.0,
            50.0,
            "",
        );
        let output = document("flowchart", 100.0, 100.0, &options, &content);
        assert!(!output.contains("var("));
        assert!(!output.contains("foreignObject"));
        assert!(output.contains("font-weight=\"bold\""));
        assert!(output.contains("A &amp; B</title>"));
        assert!(output.contains("&lt;diagram&gt;</desc>"));
        assert_eq!(
            output,
            document("flowchart", 100.0, 100.0, &options, &content)
        );
    }

    #[test]
    fn configured_monospace_fonts_are_escaped_and_emitted_concretely() {
        let options = RenderOptions {
            font_family_mono: "Iosevka, \"unsafe\"".to_owned(),
            ..RenderOptions::default()
        };
        let rich = text::label("before <code>value</code>", 13.0, None);
        let output = label_with_options(&rich, 50.0, 20.0, "", &options);
        assert!(output.contains("font-family=\"Iosevka, &quot;unsafe&quot;\""));
        assert!(!output.contains("var("));
    }
}
