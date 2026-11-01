use crate::{PeleError, RenderOptions, RenderResult, svg, text};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PieModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub show_data: bool,
    pub sections: Vec<(String, f64)>,
}

struct Reader<'a> {
    source: &'a str,
    at: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.source[self.at..]
    }

    fn error(&self, message: &str) -> PeleError {
        let before = &self.source[..self.at];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count()
            + 1;
        PeleError::syntax(message, line, column)
    }

    fn horizontal(&mut self) {
        while matches!(self.source.as_bytes().get(self.at), Some(b' ' | b'\t')) {
            self.at += 1;
        }
    }

    fn comment(&mut self) {
        if self.rest().starts_with("%%") {
            self.at += self.rest().find('\n').unwrap_or(self.rest().len());
        }
    }

    fn keyword(&mut self, keyword: &str) -> bool {
        if self.rest().starts_with(keyword) {
            let next = &self.rest()[keyword.len()..];
            if next.is_empty()
                || next.starts_with("%%")
                || next.chars().next().is_some_and(char::is_whitespace)
            {
                self.at += keyword.len();
                return true;
            }
        }
        false
    }

    fn line_value(&mut self) -> String {
        let end = self.rest().find(['\n', '\r']).unwrap_or(self.rest().len());
        let raw = &self.rest()[..end];
        let raw = raw.split_once("%%").map_or(raw, |(value, _)| value);
        let value = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        self.at += end;
        value
    }

    fn end_line(&mut self) -> Result<(), PeleError> {
        self.horizontal();
        self.comment();
        if !self.rest().is_empty() && !self.rest().starts_with(['\r', '\n']) {
            return Err(self.error("Expected a line break after pie section"));
        }
        while self.rest().starts_with(['\r', '\n']) {
            self.at += 1;
        }
        Ok(())
    }

    fn string(&mut self) -> Result<String, PeleError> {
        let quote = self.rest().chars().next().unwrap_or('\0');
        if quote != '\'' && quote != '"' {
            return Err(self.error("Expected a quoted pie section label"));
        }
        self.at += 1;
        let mut value = String::new();
        while let Some(ch) = self.rest().chars().next() {
            self.at += ch.len_utf8();
            if ch == quote {
                return Ok(value);
            }
            if ch == '\\' {
                let Some(escaped) = self.rest().chars().next() else {
                    break;
                };
                self.at += escaped.len_utf8();
                value.push(match escaped {
                    'b' => '\u{0008}',
                    'f' => '\u{000c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'v' => '\u{000b}',
                    '0' => '\0',
                    other => other,
                });
            } else {
                value.push(ch);
            }
        }
        Err(self.error("Unterminated pie section label"))
    }

    fn number(&mut self) -> Result<f64, PeleError> {
        let start = self.at;
        if self.rest().starts_with('-') {
            self.at += 1;
        }
        let digits = self.at;
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_digit)
        {
            self.at += 1;
        }
        if self.at == digits {
            return Err(self.error("Expected a decimal pie section value"));
        }
        if self.rest().starts_with('.') {
            self.at += 1;
            let fraction = self.at;
            while self
                .source
                .as_bytes()
                .get(self.at)
                .is_some_and(u8::is_ascii_digit)
            {
                self.at += 1;
            }
            if self.at == fraction {
                return Err(self.error("Expected digits after pie decimal point"));
            }
        } else if self.at - digits > 1 && self.source.as_bytes()[digits] == b'0' {
            return Err(self.error("Leading zeros require a decimal fraction"));
        }
        let value = self.source[start..self.at]
            .parse::<f64>()
            .map_err(|_| self.error("Invalid pie section value"))?;
        if !value.is_finite() {
            return Err(PeleError::semantic("Pie section values must be finite"));
        }
        Ok(value)
    }
}

pub fn parse(source: &str) -> Result<PieModel, PeleError> {
    let mut reader = Reader { source, at: 0 };
    while reader
        .rest()
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
    {
        reader.at += reader.rest().chars().next().unwrap().len_utf8();
    }
    if !reader.keyword("pie") {
        return Err(reader.error("Expected pie diagram header"));
    }
    reader.horizontal();
    let mut model = PieModel {
        show_data: reader.keyword("showData"),
        ..PieModel::default()
    };
    while !reader.rest().is_empty() {
        reader.horizontal();
        reader.comment();
        if reader.rest().is_empty() {
            break;
        }
        if reader.rest().starts_with(['\n', '\r']) {
            reader.at += 1;
            continue;
        }
        if reader.keyword("title") {
            model.title = Some(reader.line_value());
        } else if reader.rest().starts_with("accTitle") {
            reader.at += "accTitle".len();
            reader.horizontal();
            if !reader.rest().starts_with(':') {
                return Err(reader.error("Expected ':' after accTitle"));
            }
            reader.at += 1;
            model.acc_title = Some(reader.line_value());
        } else if reader.rest().starts_with("accDescr") {
            reader.at += "accDescr".len();
            reader.horizontal();
            if reader.rest().starts_with(':') {
                reader.at += 1;
                model.acc_descr = Some(reader.line_value());
            } else if reader.rest().starts_with('{') {
                reader.at += 1;
                let Some(end) = reader.rest().find('}') else {
                    return Err(reader.error("Unterminated accDescr block"));
                };
                let description = reader.rest()[..end]
                    .lines()
                    .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                model.acc_descr = Some(description);
                reader.at += end + 1;
            } else {
                return Err(reader.error("Expected ':' or '{' after accDescr"));
            }
        } else {
            let label = reader.string()?;
            reader.horizontal();
            if !reader.rest().starts_with(':') {
                return Err(reader.error("Expected ':' after pie section label"));
            }
            reader.at += 1;
            reader.horizontal();
            let value = reader.number()?;
            if value < 0.0 {
                return Err(PeleError::semantic(format!(
                    "\"{label}\" has invalid value: {value}. Negative values are not allowed in pie charts. All slice values must be >= 0."
                )));
            }
            if !model.sections.iter().any(|(name, _)| name == &label) {
                model.sections.push((label, value));
            }
        }
        reader.end_line()?;
    }
    model.title = model.title.filter(|title| !title.is_empty());
    Ok(model)
}

fn number_text(value: f64) -> String {
    if value != 0.0 && (value.abs() < 1e-6 || value.abs() >= 1e21) {
        let scientific = format!("{value:e}");
        if let Some((mantissa, exponent)) = scientific.split_once('e') {
            return if exponent.starts_with('-') {
                scientific
            } else {
                format!("{mantissa}e+{exponent}")
            };
        }
    }
    if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    }
}

fn point(angle: f64, radius: f64) -> String {
    format!(
        "{},{}",
        svg::num(angle.sin() * radius),
        svg::num(-angle.cos() * radius)
    )
}

fn slice(start: f64, end: f64, outer: f64, inner: f64) -> String {
    if end - start >= TAU - 1e-9 {
        let ring = |radius: f64, sweep: usize| {
            let r = svg::num(radius);
            let minus_r = svg::num(-radius);
            format!("M0,{minus_r}A{r},{r} 0 1 {sweep} 0,{r}A{r},{r} 0 1 {sweep} 0,{minus_r}Z")
        };
        return ring(outer, 1)
            + &if inner > 0.0 {
                ring(inner, 0)
            } else {
                String::new()
            };
    }
    let large = usize::from(end - start > PI);
    let r = svg::num(outer);
    if inner <= 0.0 {
        return format!(
            "M0,0L{}A{r},{r} 0 {large} 1 {}Z",
            point(start, outer),
            point(end, outer)
        );
    }
    let inner_r = svg::num(inner);
    format!(
        "M{}A{r},{r} 0 {large} 1 {}L{}A{inner_r},{inner_r} 0 {large} 0 {}Z",
        point(start, outer),
        point(end, outer),
        point(end, inner),
        point(start, inner)
    )
}

fn series(options: &RenderOptions, index: usize) -> &str {
    options
        .theme
        .series
        .get(index % options.theme.series.len().max(1))
        .map_or(options.theme.accent.as_str(), String::as_str)
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let model = parse(source)?;
    let mut result = draw(&model, options, 120.0, false);
    if let Some(available) = options.max_width.filter(|width| *width > 0.0)
        && result.width > available
    {
        let mut diameter = 240.0;
        result = draw(&model, options, diameter / 2.0, true);
        for _ in 0..3 {
            if result.width <= available || diameter <= 140.0 {
                break;
            }
            diameter = (diameter - (result.width - available)).max(140.0);
            result = draw(&model, options, diameter / 2.0, true);
        }
    }
    Ok(result)
}

fn draw(model: &PieModel, options: &RenderOptions, radius: f64, tight: bool) -> RenderResult {
    let fallback = text::MetricsMeasurer;
    let measurer = options.measurer.as_deref().unwrap_or(&fallback);
    let label_svg = |label: &text::Label, x, y, attrs: &str| {
        svg::label_with_options(label, x, y, attrs, options)
    };
    let size = options.font_size;
    let small = (size * 0.875).round();
    let pad = options.padding;
    let config = &options.config["pie"];
    let text_position = config["textPosition"]
        .as_f64()
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(0.75);
    let inner = config["donutHole"]
        .as_f64()
        .filter(|v| *v > 0.0 && *v <= 0.9)
        .unwrap_or(0.0)
        * radius;
    let placed = config["legendPosition"].as_str().unwrap_or("right");
    let legend_position = if tight && matches!(placed, "left" | "right") {
        "bottom"
    } else {
        placed
    };
    let legend: Vec<_> = model
        .sections
        .iter()
        .map(|(label, value)| {
            let raw = if model.show_data {
                format!("{label} [{}]", number_text(*value))
            } else {
                label.clone()
            };
            text::layout_label(&raw, false, measurer, small, 4000.0, 0)
        })
        .collect();
    let legend_width = legend.iter().map(|label| label.width).fold(0.0, f64::max)
        + if legend.is_empty() { 0.0 } else { 20.0 };
    let legend_height = legend.len() as f64 * 26.0;
    let title_room = options
        .max_width
        .filter(|width| *width > 0.0)
        .map_or(4000.0, |width| (width - 2.0 * pad).max(80.0));
    let title = text::layout_label(
        model
            .title
            .as_ref()
            .or(options.title.as_ref())
            .map_or("", String::as_str),
        false,
        measurer,
        size,
        title_room,
        1,
    );
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let sum: f64 = model.sections.iter().map(|(_, value)| value).sum();
    let shares: Vec<_> = model
        .sections
        .iter()
        .map(|(_, value)| {
            let share = if sum > 0.0 && sum.is_finite() {
                value / sum
            } else {
                0.0
            };
            if share * 100.0 >= 1.0 { share } else { 0.0 }
        })
        .collect();
    let narrow: Vec<_> = shares
        .iter()
        .map(|share| {
            if *share == 0.0 || *share >= 0.5 {
                return false;
            }
            let chord = 2.0 * radius * text_position * (share * PI).sin();
            chord < measurer.width(&format!("{}%", (share * 100.0).round() as i64), small, 0) + 6.0
        })
        .collect();
    let margin = if narrow.contains(&true) { 28.0 } else { 0.0 };
    let diameter = (radius + margin) * 2.0;
    let stacked = matches!(legend_position, "top" | "bottom");
    let mut width = diameter;
    let mut height = diameter;
    if matches!(legend_position, "left" | "right") {
        width += if legend_width > 0.0 {
            32.0 + legend_width
        } else {
            0.0
        };
        height = height.max(legend_height);
    } else if stacked {
        width = width.max(legend_width);
        height += if legend_height > 0.0 {
            20.0 + legend_height
        } else {
            0.0
        };
    }
    width = width.max(title.width);
    let cx = pad
        + match legend_position {
            "left" => legend_width + 32.0 + diameter / 2.0,
            "right" => diameter / 2.0,
            _ => width / 2.0,
        };
    let body_top = pad + title_height;
    let cy = body_top
        + if legend_position == "top" {
            legend_height + 20.0 + diameter / 2.0
        } else if stacked {
            diameter / 2.0
        } else {
            height / 2.0
        };
    let mut slices = String::new();
    let mut labels = String::new();
    let mut outside = String::new();
    let mut angle = 0.0;
    let rim = radius + 14.0;
    let mut placed_outside: Vec<(f64, f64, f64)> = Vec::new();
    for (index, (name, _)) in model.sections.iter().enumerate() {
        let share = shares[index];
        if share == 0.0 {
            continue;
        }
        let end = angle + share * TAU;
        slices.push_str(&format!(
            "<path class=\"pele-slice\" data-id=\"{}\" d=\"{}\" fill=\"{}\"/>",
            svg::esc_text(name),
            slice(angle, end, radius, inner),
            svg::esc(series(options, index))
        ));
        let mid = (angle + end) / 2.0;
        let percentage = format!("{}%", (share * 100.0).round() as i64);
        if narrow[index] {
            let text_width = measurer.width(&percentage, small, 0);
            let mut at = mid;
            for _ in 0..360 {
                let (x, y) = (at.sin() * rim, -at.cos() * rim);
                if placed_outside.iter().all(|(ox, oy, ow)| {
                    (x - ox).abs() >= (text_width + ow) / 2.0 + 2.0 || (y - oy).abs() >= small + 2.0
                }) {
                    break;
                }
                at += 2.0 / rim;
            }
            let (x, y) = (at.sin() * rim, -at.cos() * rim);
            placed_outside.push((x, y, text_width));
            outside.push_str(&format!(
                "<text x=\"{}\" y=\"{}\">{percentage}</text>",
                svg::num(x),
                svg::num(y + small * 0.35)
            ));
        } else {
            let r = radius * text_position;
            labels.push_str(&format!(
                "<text x=\"{}\" y=\"{}\">{percentage}</text>",
                svg::num(mid.sin() * r),
                svg::num(-mid.cos() * r + small * 0.35)
            ));
        }
        angle = end;
    }
    if slices.is_empty() {
        slices = format!(
            "<circle class=\"pele-slice\" r=\"{}\" fill=\"none\" stroke=\"{}\"/>",
            svg::num(radius),
            svg::esc(&options.theme.border)
        );
    }
    let (lx, ly) = match legend_position {
        "right" => (
            cx + diameter / 2.0 + 32.0,
            body_top + (height - legend_height) / 2.0,
        ),
        "left" => (pad, body_top + (height - legend_height) / 2.0),
        "center" => (cx - legend_width / 2.0, cy - legend_height / 2.0),
        _ => (
            pad + (width - legend_width) / 2.0,
            if legend_position == "top" {
                body_top
            } else {
                body_top + diameter + 20.0
            },
        ),
    };
    let mut legend_out = String::new();
    for (index, label) in legend.iter().enumerate() {
        let y = ly + index as f64 * 26.0 + 13.0;
        legend_out.push_str(&format!("<g class=\"pele-legend-item\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"12\" height=\"12\" rx=\"2\" fill=\"{}\"/>{}</g>", svg::esc_text(&model.sections[index].0), svg::num(lx), svg::num(y - 6.0), svg::esc(series(options, index)), label_svg(label, lx + 20.0 + label.width / 2.0, y, "")));
    }
    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    let mut content = label_svg(
        &title,
        total_width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"bold\"",
    );
    content.push_str(&format!("<g class=\"pele-slices\" transform=\"translate({},{})\" stroke=\"{}\" stroke-width=\"1\" stroke-linejoin=\"round\">{slices}</g>", svg::num(cx), svg::num(cy), svg::esc(&options.theme.background)));
    if !labels.is_empty() {
        content.push_str(&format!("<g class=\"pele-slice-labels\" transform=\"translate({},{})\" font-size=\"{}\" text-anchor=\"middle\" fill=\"{}\">{labels}</g>", svg::num(cx), svg::num(cy), svg::num(small), svg::esc(&options.theme.background)));
    }
    if !outside.is_empty() {
        content.push_str(&format!("<g class=\"pele-slice-labels\" transform=\"translate({},{})\" font-size=\"{}\" text-anchor=\"middle\">{outside}</g>", svg::num(cx), svg::num(cy), svg::num(small)));
    }
    if !legend_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-legend\" font-size=\"{}\">{legend_out}</g>",
            svg::num(small)
        ));
    }
    let mut document_options = options.clone();
    document_options.title = model.title.clone().or(document_options.title);
    document_options.acc_title = model.acc_title.clone().or(document_options.acc_title);
    document_options.acc_descr = model.acc_descr.clone().or(document_options.acc_descr);
    RenderResult::new(
        "pie",
        svg::document(
            "pie",
            total_width,
            total_height,
            &document_options,
            &content,
        ),
        total_width,
        total_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_values_accessibility_and_duplicate_names() {
        let model = parse("pie showData title Pets\naccTitle: Adoption\naccDescr { first\n second }\n\"Dogs\": 386\n\"Cats\": 85.5\n\"Dogs\": 1\n").unwrap();
        assert!(model.show_data);
        assert_eq!(model.title.as_deref(), Some("Pets"));
        assert_eq!(model.acc_descr.as_deref(), Some("first\nsecond"));
        assert_eq!(
            model.sections,
            vec![("Dogs".into(), 386.0), ("Cats".into(), 85.5)]
        );
    }

    #[test]
    fn rejects_bad_numbers_and_unterminated_labels() {
        for source in [
            "pie\n\"a\": -1",
            "pie\n\"a\": 1e2",
            "pie\n\"a\": 00",
            "pie\n\"a\": 1.",
            "pie\n\"a: 1",
            "pie\n\"a\": 1; \"b\": 2",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn leaves_tiny_values_in_legend_and_uses_two_arcs_for_full_circle() {
        let options = RenderOptions::default();
        let result = render("pie\n\"Big\": 1000\n\"Tiny\": 1", &options).unwrap();
        assert_eq!(result.svg.matches("class=\"pele-slice\"").count(), 1);
        assert_eq!(result.svg.matches("class=\"pele-legend-item\"").count(), 2);
        let full = render("pie\n\"Only\": 1", &options).unwrap();
        assert!(full.svg.contains("100%"));
        assert!(full.svg.contains("A120,120 0 1 1"));
        let zero = render("pie\n\"Zero\":0", &options).unwrap();
        assert!(!zero.svg.contains("NaN"));
        assert!(!zero.svg.contains("var("));
    }

    #[test]
    fn narrows_and_supports_donut_configuration() {
        let mut options = RenderOptions::default();
        let source = "pie showData\n\"A long label\":1\n\"Another label\":1";
        let plain = render(source, &options).unwrap();
        options.config = serde_json::json!({"pie":{"donutHole":0.5}});
        let donut = render(source, &options).unwrap();
        assert_ne!(plain.svg, donut.svg);
        assert!(donut.svg.contains("A60,60"));
        options.max_width = Some(240.0);
        let narrow = render(source, &options).unwrap();
        assert!(narrow.width < plain.width);
        assert!(narrow.height > plain.height);
    }

    #[test]
    fn escapes_labels_in_attributes_and_text() {
        let rendered = render("pie\n'<script>&x': 1", &RenderOptions::default()).unwrap();
        assert!(rendered.svg.contains("&lt;script&gt;&amp;x"));
        assert!(!rendered.svg.contains("<script>"));
        let entities = render("pie\n'A &amp; B': 1", &RenderOptions::default()).unwrap();
        assert!(entities.svg.contains("data-id=\"A &amp; B\""));
        assert!(!entities.svg.contains("data-id=\"A &amp;amp; B\""));
    }

    #[test]
    fn formats_show_data_numbers_like_javascript() {
        assert_eq!(number_text(1e-7), "1e-7");
        assert_eq!(number_text(1e21), "1e+21");
        assert_eq!(number_text(1e20), "100000000000000000000");
        assert_eq!(number_text(42.96), "42.96");
        assert_eq!(number_text(-0.0), "0");
        let fallback = RenderOptions {
            title: Some("Front matter".into()),
            ..RenderOptions::default()
        };
        let rendered = render("pie title\n'A':1", &fallback).unwrap();
        assert!(rendered.svg.contains("Front matter"));
    }
}
