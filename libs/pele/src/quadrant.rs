use crate::{PeleError, RenderOptions, RenderResult, svg, text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuadrantText {
    pub text: String,
    pub markdown: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PointStyle {
    pub radius: Option<f64>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuadrantPoint {
    pub x: f64,
    pub y: f64,
    pub text: QuadrantText,
    pub class_name: String,
    pub style: PointStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuadrantModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub quadrants: [Option<QuadrantText>; 4],
    pub x_axis_left: Option<QuadrantText>,
    pub x_axis_right: Option<QuadrantText>,
    pub y_axis_bottom: Option<QuadrantText>,
    pub y_axis_top: Option<QuadrantText>,
    pub points: Vec<QuadrantPoint>,
    pub classes: BTreeMap<String, PointStyle>,
}

fn parse_styles(source: &str) -> Result<PointStyle, PeleError> {
    let mut style = PointStyle::default();
    if source.trim().is_empty() {
        return Ok(style);
    }
    for declaration in source.split(',') {
        let (key, value) = declaration
            .split_once(':')
            .unwrap_or((declaration, "undefined"));
        let key = key.trim();
        let value = value.split(':').next().unwrap_or(value).trim();
        let digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
        let hex = |value: &str| {
            let hex = value.strip_prefix('#').unwrap_or(value);
            matches!(hex.len(), 3 | 6) && hex.bytes().all(|b| b.is_ascii_hexdigit())
        };
        let invalid = |kind: &str| {
            PeleError::semantic(format!(
                "value for {key} {value} is invalid, please use a valid {kind}"
            ))
        };
        match key {
            "radius" => {
                if !digits(value) {
                    return Err(invalid("number"));
                }
                style.radius = Some(value.parse::<f64>().unwrap_or(f64::INFINITY));
            }
            "color" => {
                if !hex(value) {
                    return Err(invalid("hex code"));
                }
                style.color = Some(value.to_owned());
            }
            "stroke-color" => {
                if !hex(value) {
                    return Err(invalid("hex code"));
                }
                style.stroke_color = Some(value.to_owned());
            }
            "stroke-width" => {
                if !value.strip_suffix("px").is_some_and(digits) {
                    return Err(invalid("number of pixels (eg. 10px)"));
                }
                style.stroke_width = Some(value.to_owned());
            }
            _ => {
                return Err(PeleError::semantic(format!(
                    "style named {key} is not supported."
                )));
            }
        }
    }
    Ok(style)
}

fn find_unquoted(source: &str, needle: &str) -> Option<usize> {
    let mut quoted = false;
    for (at, ch) in source.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted && source[at..].starts_with(needle) {
            return Some(at);
        }
    }
    None
}

fn axis_delimiter(source: &str) -> Option<(usize, usize)> {
    let mut quoted = false;
    let mut at = 0;
    while at < source.len() {
        let ch = source[at..].chars().next()?;
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted && ch == '-' {
            let start = at;
            while source.as_bytes().get(at) == Some(&b'-') {
                at += 1;
            }
            if at - start >= 2 && source.as_bytes().get(at) == Some(&b'>') {
                return Some((start, at + 1));
            }
            continue;
        }
        at += ch.len_utf8();
    }
    None
}

fn text_value(source: &str, line: usize) -> Result<QuadrantText, PeleError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(PeleError::syntax("Expected quadrant text", line, 1));
    }
    let markdown = source.starts_with("\"`");
    let value = if markdown {
        source
            .strip_prefix("\"`")
            .and_then(|value| value.strip_suffix("`\""))
            .ok_or_else(|| PeleError::syntax("Unterminated quadrant markdown label", line, 1))?
    } else if source.starts_with('"') {
        source
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| PeleError::syntax("Unterminated quadrant label", line, 1))?
    } else {
        if source.contains(['"', ':', '[', ']', '<', '>', '(', ')', '{', '}', '|']) {
            return Err(PeleError::syntax(
                "Unexpected character in quadrant text",
                line,
                1,
            ));
        }
        source
    };
    if value.contains('"') || (markdown && value.contains('`')) {
        return Err(PeleError::syntax(
            "Unexpected quote inside quadrant text",
            line,
            1,
        ));
    }
    Ok(QuadrantText {
        text: value.trim().to_owned(),
        markdown,
    })
}

fn statements(source: &str) -> Result<Vec<(usize, String)>, PeleError> {
    let mut output = Vec::new();
    let mut value = String::new();
    let mut quote = false;
    let mut comment = false;
    let mut description = false;
    let mut line = 1;
    let mut start_line = 1;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if comment {
            if ch != '\n' {
                continue;
            }
            comment = false;
        }
        if !quote && !description && ch == '%' && chars.peek() == Some(&'%') {
            comment = true;
            chars.next();
            continue;
        }
        if ch == '"' && !description {
            quote = !quote;
        }
        if !quote && ch == '{' && value.trim().eq_ignore_ascii_case("accDescr") {
            description = true;
        }
        if description && ch == '}' {
            description = false;
        }
        if !quote && !description && matches!(ch, '\n' | '\r' | ';') {
            if !value.trim().is_empty() {
                output.push((start_line, std::mem::take(&mut value)));
            }
            if ch == '\n' {
                line += 1;
            }
            start_line = line;
        } else {
            value.push(ch);
            if ch == '\n' {
                line += 1;
            }
        }
    }
    if quote || description {
        return Err(PeleError::syntax(
            "Unterminated quadrant label or description",
            start_line,
            1,
        ));
    }
    if !value.trim().is_empty() {
        output.push((start_line, value));
    }
    Ok(output)
}

fn coordinate(source: &str, at: &mut usize, line: usize) -> Result<f64, PeleError> {
    let bytes = source.as_bytes();
    let start = *at;
    match bytes.get(*at) {
        Some(b'1') => *at += 1,
        Some(b'0') => {
            *at += 1;
            if bytes.get(*at).is_some_and(|b| !matches!(b, b'\r' | b'\n'))
                && bytes.get(*at + 1).is_some_and(u8::is_ascii_digit)
            {
                *at += 2;
                while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
                    *at += 1;
                }
            }
        }
        _ => {
            return Err(PeleError::syntax(
                "Quadrant coordinates must start with 0 or 1",
                line,
                start + 1,
            ));
        }
    }
    let value = &source[start..*at];
    let parsed = if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_or(f64::INFINITY, |number| number as f64)
    } else if let Some(binary) = value.strip_prefix("0b") {
        u64::from_str_radix(binary, 2).map_or(f64::NAN, |number| number as f64)
    } else {
        value.parse::<f64>().unwrap_or(f64::NAN)
    };
    Ok(parsed)
}

pub fn parse(source: &str) -> Result<QuadrantModel, PeleError> {
    let statements = statements(source)?;
    let mut statements = statements.into_iter();
    let Some((line, header)) = statements.next() else {
        return Err(PeleError::syntax("Expected quadrantChart header", 1, 1));
    };
    let header = header.trim();
    if !header
        .get(..13)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("quadrantChart"))
    {
        return Err(PeleError::syntax("Expected quadrantChart header", line, 1));
    }
    let mut model = QuadrantModel::default();
    let inline = header[13..].trim();
    let inline = (!inline.is_empty()).then(|| (line, inline.to_owned()));
    for (line, statement) in inline.into_iter().chain(statements) {
        let statement = statement.trim();
        let lower = statement.to_ascii_lowercase();
        if let Some(title) = lower
            .strip_prefix("title")
            .filter(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
        {
            model.title = Some(statement[statement.len() - title.len()..].trim().to_owned());
        } else if let Some(rest) = lower.strip_prefix("acctitle") {
            let offset = statement.len() - rest.len();
            let Some(value) = statement[offset..].trim_start().strip_prefix(':') else {
                return Err(PeleError::syntax("Expected ':' after accTitle", line, 1));
            };
            model.acc_title = Some(value.trim_start().to_owned());
        } else if let Some(rest) = lower.strip_prefix("accdescr") {
            let offset = statement.len() - rest.len();
            let value = statement[offset..].trim_start();
            if let Some(value) = value.strip_prefix(':') {
                model.acc_descr = Some(value.trim_start().to_owned());
            } else if let Some(value) = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
                model.acc_descr = Some(
                    value
                        .trim()
                        .lines()
                        .map(str::trim_start)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            } else {
                return Err(PeleError::syntax(
                    "Expected quadrant accessibility description",
                    line,
                    1,
                ));
            }
        } else if lower.starts_with("x-axis") || lower.starts_with("y-axis") {
            let raw = statement[6..].trim();
            let delimiter = axis_delimiter(raw);
            let (from, to) = if let Some((at, end)) = delimiter {
                let from = text_value(&raw[..at], line)?;
                let rest = raw[end..].trim();
                if rest.is_empty() {
                    (
                        QuadrantText {
                            text: from.text + " ⟶ ",
                            markdown: from.markdown,
                        },
                        None,
                    )
                } else {
                    (from, Some(text_value(rest, line)?))
                }
            } else {
                (text_value(raw, line)?, None)
            };
            if lower.starts_with('x') {
                model.x_axis_left = Some(from);
                model.x_axis_right = to;
            } else {
                model.y_axis_bottom = Some(from);
                model.y_axis_top = to;
            }
        } else if lower.starts_with("quadrant-") {
            let Some(index) = lower
                .as_bytes()
                .get(9)
                .and_then(|number| number.checked_sub(b'1'))
                .filter(|index| *index < 4)
            else {
                return Err(PeleError::syntax(
                    "Expected quadrant number from 1 to 4",
                    line,
                    1,
                ));
            };
            model.quadrants[usize::from(index)] = Some(text_value(&statement[10..], line)?);
        } else if lower.starts_with("classdef") {
            let rest = statement[8..].trim_start();
            let Some(end) = rest.find(char::is_whitespace) else {
                return Err(PeleError::syntax(
                    "Expected a class name and point styles",
                    line,
                    1,
                ));
            };
            let class_name = rest[..end].to_owned();
            if class_name.is_empty() {
                return Err(PeleError::syntax("Expected quadrant class name", line, 1));
            }
            model
                .classes
                .insert(class_name, parse_styles(&rest[end..])?);
        } else {
            let mut colon = None;
            let mut at = 0;
            while let Some(relative) = find_unquoted(&statement[at..], ":") {
                let position = at + relative;
                if statement[position + 1..].trim_start().starts_with('[') {
                    colon = Some(position);
                    break;
                }
                at = position + 1;
            }
            let Some(colon) = colon else {
                return Err(PeleError::syntax(
                    "Expected quadrant point coordinates ': [x, y]'",
                    line,
                    1,
                ));
            };
            let name = statement[..colon].trim();
            let (name, class_name) = if let Some(class_at) = find_unquoted(name, ":::") {
                let class_name = &name[class_at + 3..];
                if class_name.is_empty()
                    || !class_name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(PeleError::syntax(
                        "Invalid quadrant point class name",
                        line,
                        1,
                    ));
                }
                (&name[..class_at], class_name.to_owned())
            } else {
                (name, String::new())
            };
            let raw = statement[colon + 1..].trim_start();
            let mut cursor = 1;
            while raw
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
            let x = coordinate(raw, &mut cursor, line)?;
            while raw
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
            if raw.as_bytes().get(cursor) != Some(&b',') {
                return Err(PeleError::syntax(
                    "Expected comma between quadrant coordinates",
                    line,
                    cursor + 1,
                ));
            }
            cursor += 1;
            while raw
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
            let y = coordinate(raw, &mut cursor, line)?;
            while raw
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
            if raw.as_bytes().get(cursor) != Some(&b']') {
                return Err(PeleError::syntax(
                    "Expected ']' after quadrant coordinates",
                    line,
                    cursor + 1,
                ));
            }
            cursor += 1;
            model.points.push(QuadrantPoint {
                x,
                y,
                text: text_value(name, line)?,
                class_name,
                style: parse_styles(&raw[cursor..])?,
            });
        }
    }
    Ok(model)
}

fn setting(options: &RenderOptions, name: &str, fallback: f64, maximum: f64) -> f64 {
    options.config["quadrantChart"][name]
        .as_f64()
        .filter(|value| *value >= 0.0)
        .map_or(fallback, |value| value.min(maximum))
}

fn label(
    value: Option<&QuadrantText>,
    size: f64,
    max: f64,
    measurer: &dyn text::TextMeasurer,
) -> text::Label {
    text::layout_label(
        value.map_or("", |value| value.text.as_str()),
        value.is_some_and(|value| value.markdown),
        measurer,
        size,
        max.max(40.0),
        0,
    )
}

fn is_clear(taken: &[f64], from: usize, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    taken[from..].as_chunks::<4>().0.iter().all(|box_| {
        box_[0] >= box_[2]
            || box_[1] >= box_[3]
            || !(x0 < box_[2] && x1 > box_[0] && y0 < box_[3] && y1 > box_[1])
    })
}

fn hex(color: &str) -> String {
    if color.starts_with('#') {
        color.to_owned()
    } else {
        format!("#{color}")
    }
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let model = parse(source)?;
    let fallback = text::MetricsMeasurer;
    let measurer = options.measurer.as_deref().unwrap_or(&fallback);
    let label_svg = |label: &text::Label, x, y, attrs: &str| {
        svg::label_with_options(label, x, y, attrs, options)
    };
    let label =
        |value: Option<&QuadrantText>, size: f64, max: f64| label(value, size, max, measurer);
    let size = options.font_size;
    let axis_size = (size * 0.875).round();
    let point_size = (size * 0.75).round();
    let natural = setting(options, "chartWidth", 500.0, 4000.0)
        .max(100.0)
        .round();
    let width = options
        .max_width
        .filter(|width| *width > 0.0)
        .map_or(natural, |width| {
            natural.min(width).max(280.0_f64.min(natural))
        })
        .round();
    let height =
        (setting(options, "chartHeight", 500.0, 4000.0).max(100.0) * width / natural).round();
    let outer = setting(options, "quadrantPadding", options.padding, 200.0);
    let x_pad = setting(options, "xAxisLabelPadding", 5.0, 200.0);
    let y_pad = setting(options, "yAxisLabelPadding", 5.0, 200.0);
    let has_points = !model.points.is_empty();
    let title = text::layout_label(
        model
            .title
            .as_ref()
            .or(options.title.as_ref())
            .map_or("", String::as_str),
        false,
        measurer,
        size,
        (width - 2.0 * outer).max(40.0),
        1,
    );
    let title_pad = setting(options, "titlePadding", 10.0, 200.0);
    let title_space = if title.height > 0.0 {
        title.height + title_pad
    } else {
        0.0
    };
    let has_x = model
        .x_axis_left
        .as_ref()
        .is_some_and(|text| !text.text.is_empty())
        || model
            .x_axis_right
            .as_ref()
            .is_some_and(|text| !text.text.is_empty());
    let has_y = model
        .y_axis_bottom
        .as_ref()
        .is_some_and(|text| !text.text.is_empty())
        || model
            .y_axis_top
            .as_ref()
            .is_some_and(|text| !text.text.is_empty());
    let x_below = has_points || options.config["quadrantChart"]["xAxisPosition"] == "bottom";
    let y_right = options.config["quadrantChart"]["yAxisPosition"] == "right";
    let side = width - 2.0 * outer;
    let x_left = label(
        model.x_axis_left.as_ref(),
        axis_size,
        if model.x_axis_right.is_some() {
            side / 2.0 - 16.0
        } else {
            side
        },
    );
    let x_right = label(model.x_axis_right.as_ref(), axis_size, side / 2.0 - 16.0);
    let x_height = x_left.height.max(x_right.height);
    let x_space = if has_x { x_height + x_pad } else { 0.0 };
    let tall = height - 2.0 * outer - title_space - x_space;
    let y_bottom = label(
        model.y_axis_bottom.as_ref(),
        axis_size,
        if model.y_axis_top.is_some() {
            tall / 2.0 - 16.0
        } else {
            tall
        },
    );
    let y_top = label(model.y_axis_top.as_ref(), axis_size, tall / 2.0 - 16.0);
    let y_height = y_bottom.height.max(y_top.height);
    let y_space = if has_y { y_height + y_pad } else { 0.0 };
    let left = outer + if y_right { 0.0 } else { y_space };
    let top = outer + title_space + if x_below { 0.0 } else { x_space };
    let w = (side - y_space).max(0.0);
    let h = tall.max(0.0);
    let tile_w = ((w - 4.0) / 2.0).max(0.0);
    let tile_h = ((h - 4.0) / 2.0).max(0.0);
    let mut taken = vec![
        left + tile_w,
        top,
        left + w - tile_w,
        top + h,
        left,
        top + tile_h,
        left + w,
        top + h - tile_h,
    ];
    let text_top = setting(options, "quadrantTextTopPadding", 5.0, 200.0);
    let mut quadrants = String::new();
    for k in 0..4 {
        let x = if k == 0 || k == 3 {
            left + w - tile_w
        } else {
            left
        };
        let y = if k < 2 { top } else { top + h - tile_h };
        let text = label(model.quadrants[k].as_ref(), size, tile_w - 16.0);
        let cx = x + tile_w / 2.0;
        let cy = if has_points {
            y + text_top + text.height / 2.0
        } else {
            y + tile_h / 2.0
        };
        taken.extend([
            cx - text.width / 2.0,
            cy - text.height / 2.0,
            cx + text.width / 2.0,
            cy + text.height / 2.0,
        ]);
        quadrants.push_str(&format!("<g class=\"pele-quadrant\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>{}</g>", k + 1, svg::num(x), svg::num(y), svg::num(tile_w), svg::num(tile_h), svg::num(options.theme.radius), label_svg(&text, cx, cy, &format!(" class=\"pele-label\" fill=\"{}\" stroke=\"none\"", svg::esc(&options.theme.foreground)))));
    }
    let mut axes = String::new();
    if has_x {
        let y = if x_below {
            top + h + x_pad + x_height / 2.0
        } else {
            top - x_pad - x_height / 2.0
        };
        axes.push_str(&label_svg(
            &x_left,
            if x_right.height > 0.0 {
                left + w / 4.0
            } else {
                left + x_left.width / 2.0
            },
            y,
            "",
        ));
        axes.push_str(&label_svg(&x_right, left + w * 3.0 / 4.0, y, ""));
    }
    if has_y {
        let x = if y_right {
            left + w + y_pad + y_height / 2.0
        } else {
            left - y_pad - y_height / 2.0
        };
        for (label, y) in [
            (
                &y_bottom,
                if y_top.height > 0.0 {
                    top + h * 3.0 / 4.0
                } else {
                    top + h - y_bottom.width / 2.0
                },
            ),
            (&y_top, top + h / 4.0),
        ] {
            if label.height > 0.0 {
                axes.push_str(&format!(
                    "<g transform=\"translate({},{}) rotate(-90)\">{}</g>",
                    svg::num(x),
                    svg::num(y),
                    label_svg(label, 0.0, 0.0, "")
                ));
            }
        }
    }
    let radius = setting(options, "pointRadius", 5.0, 200.0);
    let text_pad = setting(options, "pointTextPadding", 5.0, 200.0);
    let fill = options
        .theme
        .series
        .first()
        .map_or(options.theme.accent.as_str(), String::as_str);
    let mut circles = Vec::new();
    let mut dots = Vec::new();
    for point in &model.points {
        if !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
            continue;
        }
        let shared = model.classes.get(&point.class_name);
        let r = point
            .style
            .radius
            .or_else(|| shared.and_then(|s| s.radius))
            .unwrap_or(radius)
            .min(2000.0);
        let color = point
            .style
            .color
            .as_ref()
            .or_else(|| shared.and_then(|s| s.color.as_ref()));
        let stroke_color = point
            .style
            .stroke_color
            .as_ref()
            .or_else(|| shared.and_then(|s| s.stroke_color.as_ref()));
        let stroke_width = point
            .style
            .stroke_width
            .as_ref()
            .or_else(|| shared.and_then(|s| s.stroke_width.as_ref()));
        let mut css = String::new();
        if let Some(color) = color {
            css.push_str(&format!("fill:{};", hex(color)));
        }
        if let Some(stroke_width) = stroke_width {
            if let Some(color) = stroke_color {
                css.push_str(&format!("stroke:{};", hex(color)));
            }
            css.push_str(&format!("stroke-width:{stroke_width};"));
        }
        let cx = left + point.x * w;
        let cy = top + (1.0 - point.y) * h;
        let stroke = stroke_width
            .and_then(|value| value.strip_suffix("px"))
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0)
            .min(400.0);
        dots.push((
            cx,
            cy,
            r + stroke / 2.0,
            label(Some(&point.text), point_size, 200.0),
        ));
        let class_name = point
            .class_name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                    ch
                } else {
                    ' '
                }
            })
            .collect::<String>();
        circles.push(format!(
            "<circle class=\"pele-point{}\" data-id=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\"{}{}/>",
            if class_name.is_empty() {
                String::new()
            } else {
                format!(" {class_name}")
            },
            svg::esc_text(&point.text.text),
            svg::num(cx),
            svg::num(cy),
            svg::num(r),
            if stroke_width.is_some() && stroke_color.is_none() {
                format!(" stroke=\"{}\"", svg::esc(fill))
            } else {
                String::new()
            },
            if css.is_empty() {
                String::new()
            } else {
                format!(" style=\"{}\"", svg::esc(&css))
            }
        ));
    }
    let careful = dots.len() <= 300;
    if careful {
        for (cx, cy, reach, _) in &dots {
            taken.extend([cx - reach, cy - reach, cx + reach, cy + reach]);
        }
    }
    let mut labels = String::new();
    for (cx, cy, reach, text) in &dots {
        let half_w = text.width / 2.0;
        let half_h = text.size * 0.6 + (text.height - text.line_height) / 2.0;
        let away = reach + text_pad;
        let inset = away.min(((tile_w - text.width) / 2.0).max(0.0));
        let quadrant = if *cx < left + w / 2.0 {
            if *cy < top + h / 2.0 { 1 } else { 2 }
        } else if *cy < top + h / 2.0 {
            0
        } else {
            3
        };
        let heading = 8 + 4 * quadrant;
        let below = (cy + away).max(taken[heading + 3] + text_pad) + half_h;
        let spots = [
            *cx,
            cy + away + half_h,
            *cx,
            cy - away - half_h,
            cx + away + half_w,
            *cy,
            cx - away - half_w,
            *cy,
            *cx,
            below,
        ];
        let (mut tx, mut ty) = (*cx, spots[1]);
        let mut found = false;
        for k in (0..20).step_by(2) {
            let spot = k % 10;
            let margin = if spot == 8 { inset } else { 0.0 };
            let x = if (spot < 4 || spot == 8) && text.width < w {
                spots[spot]
                    .max(left + half_w + margin)
                    .min(left + w - half_w - margin)
            } else {
                spots[spot]
            };
            let y = spots[spot + 1];
            if x - half_w < left
                || x + half_w > left + w
                || y - half_h < top
                || y + half_h > top + h
            {
                continue;
            }
            let free = !careful
                || is_clear(
                    &taken,
                    if k < 10 { 0 } else { 8 },
                    x - half_w,
                    y - half_h,
                    x + half_w,
                    y + half_h,
                );
            if !found || free {
                tx = x;
                ty = y;
                found = true;
            }
            if free {
                break;
            }
        }
        if careful {
            taken.extend([tx - half_w, ty - half_h, tx + half_w, ty + half_h]);
        }
        labels.push_str(&label_svg(text, tx, ty, " class=\"pele-label\""));
    }
    let mut content = label_svg(
        &title,
        width / 2.0,
        title_pad / 2.0 + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"bold\"",
    );
    content.push_str(&format!(
        "<g class=\"pele-quadrants\" fill=\"{}\" stroke=\"{}\">{quadrants}</g>",
        svg::esc(&options.theme.surface),
        svg::esc(&options.theme.border)
    ));
    if !axes.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-axis-labels\" font-size=\"{}\" fill=\"{}\">{axes}</g>",
            svg::num(axis_size),
            svg::esc(&options.theme.muted)
        ));
    }
    if !dots.is_empty() {
        circles.reverse();
        content.push_str(&format!("<g class=\"pele-points\" fill=\"{}\">{}</g><g class=\"pele-point-labels\" font-size=\"{}\">{labels}</g>", svg::esc(fill), circles.join(""), svg::num(point_size)));
    }
    let mut document_options = options.clone();
    document_options.title = model.title.clone().or(document_options.title);
    document_options.acc_title = model.acc_title.clone().or(document_options.acc_title);
    document_options.acc_descr = model.acc_descr.clone().or(document_options.acc_descr);
    Ok(RenderResult::new(
        "quadrantChart",
        svg::document("quadrantChart", width, height, &document_options, &content),
        width,
        height,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_axes_quadrants_and_point_classes() {
        let model = parse("quadrantChart\n title Priority\n x-axis Low --> High\n y-axis \"Low impact\" --> \"`**High** impact`\"\n quadrant-1 Plan\n A:::work: [0.2, 0.75] radius: 12\n classDef work color: #f00, stroke-width: 2px").unwrap();
        assert_eq!(model.title.as_deref(), Some("Priority"));
        assert_eq!(model.x_axis_right.unwrap().text, "High");
        assert!(model.y_axis_top.unwrap().markdown);
        assert_eq!(model.points[0].class_name, "work");
        assert_eq!(model.points[0].style.radius, Some(12.0));
        assert_eq!(model.classes["work"].color.as_deref(), Some("#f00"));
    }

    #[test]
    fn rejects_invalid_coordinates_styles_and_unclosed_labels() {
        for source in [
            "quadrantChart\n A: [1.2, 0.4]",
            "quadrantChart\n A: [0.2, 0.4",
            "quadrantChart\n A: [-0.2, 0.4]",
            "quadrantChart\n A: [0.2, 0.4] radius: big",
            "quadrantChart\n A: [0.2, 0.4] color: red",
            "quadrantChart\n classDef c fill:#fff",
            "quadrantChart\n \"A: [0.2, 0.4]",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn honors_coordinate_direction_and_point_paint_order() {
        let rendered = render(
            "quadrantChart\n First: [0, 0]\n Second: [1, 1]",
            &RenderOptions::default(),
        )
        .unwrap();
        assert_eq!((rendered.width, rendered.height), (500.0, 500.0));
        assert!(
            rendered
                .svg
                .contains("data-id=\"First\" cx=\"8\" cy=\"492\"")
        );
        assert!(
            rendered
                .svg
                .contains("data-id=\"Second\" cx=\"492\" cy=\"8\"")
        );
        let first = rendered.svg.find("data-id=\"First\"").unwrap();
        let second = rendered.svg.find("data-id=\"Second\"").unwrap();
        assert!(second < first);
        assert!(!rendered.svg.contains("var("));
    }

    #[test]
    fn applies_direct_style_over_classes_and_dimensions() {
        let options = RenderOptions {
            config: serde_json::json!({"quadrantChart":{"chartWidth":400,"chartHeight":320,"pointRadius":9}}),
            ..RenderOptions::default()
        };
        let rendered = render("quadrantChart\n A:::c: [0.5, 0.5] color:#f00,radius:12\n classDef c color:#0f0,stroke-color:#00f,stroke-width:3px", &options).unwrap();
        assert_eq!((rendered.width, rendered.height), (400.0, 320.0));
        assert!(rendered.svg.contains("r=\"12\""));
        assert!(
            rendered
                .svg
                .contains("fill:#f00;stroke:#00f;stroke-width:3px;")
        );
    }

    #[test]
    fn skips_accepted_non_numeric_or_outside_coordinate_quirks() {
        let rendered = render(
            "quadrantChart\n A: [0x5, 0.5]\n B: [0,5, 0.5]\n C: [0.5, 0.5]",
            &RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(rendered.svg.matches("<circle").count(), 1);
        assert!(rendered.svg.contains("data-id=\"C\""));
    }

    #[test]
    fn semicolons_and_quoted_arrows_preserve_text() {
        let parsed =
            parse("quadrantChart; x-axis \"left --> left\" --> right; \"A; B\": [0.5,0.5]")
                .unwrap();
        assert_eq!(parsed.x_axis_left.unwrap().text, "left --> left");
        assert_eq!(parsed.points[0].text.text, "A; B");
        let inline = parse("quadrantChart title Inline\nx-axis Left ----> Right").unwrap();
        assert_eq!(inline.title.as_deref(), Some("Inline"));
        assert_eq!(inline.x_axis_left.unwrap().text, "Left");
        assert_eq!(inline.x_axis_right.unwrap().text, "Right");
    }
}
