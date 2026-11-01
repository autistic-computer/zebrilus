use std::fmt::Write;

use crate::compound::CNode;
use crate::svg::{esc, num};

pub const SHAPES: &[&str] = &[
    "f-circ",
    "notch-rect",
    "lin-rect",
    "div-rect",
    "win-pane",
    "tri",
    "flip-tri",
    "sl-rect",
    "notch-pent",
    "hourglass",
    "delay",
    "h-cyl",
    "cross-circ",
    "st-rect",
    "doc",
    "flag",
    "datastore",
    "folder",
    "bucket",
    "console",
    "browser",
    "person",
    "bang",
    "cloud",
    "brace",
    "brace-r",
    "braces",
    "bolt",
    "lin-cyl",
    "curv-trap",
    "docs",
    "bow-rect",
    "tag-rect",
    "tag-doc",
    "lin-doc",
    "collapsedGroup",
];

pub fn canonical(name: &str) -> Option<&'static str> {
    Some(match name.trim() {
        "f-circ" | "junction" | "filled-circle" => "f-circ",
        "notch-rect" | "card" | "notched-rectangle" => "notch-rect",
        "lin-rect" | "lined-rectangle" | "lined-process" | "lin-proc" | "shaded-process" => {
            "lin-rect"
        }
        "div-rect" | "div-proc" | "divided-rectangle" | "divided-process" => "div-rect",
        "win-pane" | "internal-storage" | "window-pane" => "win-pane",
        "tri" | "extract" | "triangle" => "tri",
        "flip-tri" | "manual-file" | "flipped-triangle" => "flip-tri",
        "sl-rect" | "manual-input" | "sloped-rectangle" => "sl-rect",
        "notch-pent" | "loop-limit" | "notched-pentagon" => "notch-pent",
        "hourglass" | "collate" => "hourglass",
        "delay" | "half-rounded-rectangle" => "delay",
        "h-cyl" | "das" | "horizontal-cylinder" => "h-cyl",
        "cross-circ" | "summary" | "crossed-circle" => "cross-circ",
        "st-rect" | "procs" | "processes" | "stacked-rectangle" => "st-rect",
        "doc" | "document" => "doc",
        "flag" | "paper-tape" => "flag",
        "datastore" | "data-store" => "datastore",
        "folder" | "directory" => "folder",
        "bucket" => "bucket",
        "console" => "console",
        "browser" => "browser",
        "person" => "person",
        "bang" => "bang",
        "cloud" => "cloud",
        "brace" | "comment" | "brace-l" => "brace",
        "brace-r" => "brace-r",
        "braces" => "braces",
        "bolt" | "com-link" | "lightning-bolt" => "bolt",
        "lin-cyl" | "disk" | "lined-cylinder" => "lin-cyl",
        "curv-trap" | "curved-trapezoid" | "display" => "curv-trap",
        "docs" | "documents" | "st-doc" | "stacked-document" => "docs",
        "bow-rect" | "stored-data" | "bow-tie-rectangle" => "bow-rect",
        "tag-rect" | "tagged-rectangle" | "tag-proc" | "tagged-process" => "tag-rect",
        "tag-doc" | "tagged-document" => "tag-doc",
        "lin-doc" | "lined-document" => "lin-doc",
        "collapsedGroup" => "collapsedGroup",
        _ => return None,
    })
}

pub fn is_supported(name: &str) -> bool {
    canonical(name).is_some()
}

pub fn size(name: &str, text_width: f64, text_height: f64) -> Option<(f64, f64, f64)> {
    let name = canonical(name)?;
    let w = text_width;
    let h = text_height;
    Some(match name {
        "f-circ" => (14.0, 14.0, 0.0),
        "notch-rect" | "datastore" | "tag-rect" => ((w + 36.0).max(48.0), h + 24.0, 0.0),
        "lin-rect" => (w + 44.0, h + 24.0, 0.0),
        "div-rect" => (w + 36.0, h + 32.0, 4.0),
        "win-pane" | "st-rect" => (w + 44.0, h + 32.0, 4.0),
        "tri" | "flip-tri" => {
            let width = (1.6 * w + 40.0).max(80.0);
            let height = (h + 10.0) / (1.0 - (w + 8.0) / width);
            let dy = height / 2.0 - 10.0 - h / 2.0;
            (width, height, if name == "flip-tri" { -dy } else { dy })
        }
        "sl-rect" => (w + 36.0, h + 34.0, 5.0),
        "notch-pent" => (w + 36.0, h + 24.0, 0.0),
        "hourglass" => (32.0, 40.0, 0.0),
        "delay" => (w + 36.0 + (h + 24.0) / 4.0, h + 24.0, 0.0),
        "h-cyl" => (
            w + 36.0 + 2.0 * 12.0_f64.min((h + 24.0) / 4.0),
            h + 24.0,
            0.0,
        ),
        "cross-circ" => (44.0, 44.0, 0.0),
        "doc" | "tag-doc" => (w + 36.0, h + 34.0, -5.0),
        "flag" => (w + 36.0, h + 40.0, 0.0),
        "folder" => ((w + 36.0).max(64.0), h + 32.0, 4.0),
        "bucket" => (w + 56.0, h + 34.0, 3.0),
        "console" | "browser" => ((w + 36.0).max(72.0), h + 36.0, 6.0),
        "person" => ((w + 36.0).max(56.0), h + 50.0, 13.0),
        "bang" => (w + 64.0, h + 48.0, 0.0),
        "cloud" => (w + 68.0, h + 48.0, 0.0),
        "brace" | "brace-r" => (w + 48.0, h + 24.0, 0.0),
        "braces" => (w + 60.0, h + 24.0, 0.0),
        "bolt" => (36.0, 56.0, 0.0),
        "lin-cyl" => {
            let width = (w + 36.0).max(56.0);
            let ry = (width / 14.0).clamp(6.0, 12.0);
            (width, h + 32.0 + 2.0 * ry, ry / 2.0 + 4.0)
        }
        "curv-trap" => (w + 36.0 + (h + 24.0) * 0.6, h + 24.0, 0.0),
        "docs" => (w + 44.0, h + 42.0, -1.0),
        "bow-rect" => (w + 36.0 + (h + 24.0) * 0.4, h + 24.0, 0.0),
        "lin-doc" => (w + 44.0, h + 34.0, -5.0),
        "collapsedGroup" => (w + 44.0, h + 32.0, -2.0),
        _ => unreachable!(),
    })
}

pub fn has_label(name: &str) -> bool {
    !matches!(
        canonical(name),
        Some("f-circ" | "hourglass" | "cross-circ" | "bolt")
    )
}

pub fn filled(name: &str) -> bool {
    !matches!(canonical(name), Some("brace" | "brace-r" | "braces"))
}

fn bulge(t: f64, half: f64, depth: f64) -> f64 {
    let k = (t.abs() / half).min(1.0);
    depth * (1.0 - (1.0 - k * k).sqrt())
}

pub fn inset(name: &str, w: f64, h: f64, side: usize, t: f64) -> f64 {
    let odd = !side.is_multiple_of(2);
    match canonical(name) {
        Some("tri" | "flip-tri" | "bolt") if odd => w / 4.0,
        Some("sl-rect") if side == 0 => 5.0,
        Some("hourglass") if odd => w / 2.0,
        Some("f-circ" | "cross-circ") => {
            if odd {
                bulge(t, h / 2.0, w / 2.0)
            } else {
                bulge(t, w / 2.0, h / 2.0)
            }
        }
        Some("doc" | "tag-doc" | "lin-doc" | "docs") if side == 2 => 10.0,
        Some("flag") if !odd => 8.0,
        Some("bucket") if odd => h / 12.0,
        Some("bang") => 3.0,
        Some("cloud") => {
            if odd {
                2.0
            } else {
                1.0
            }
        }
        Some("lin-cyl") if !odd => bulge(t, w / 2.0, (w / 14.0).clamp(6.0, 12.0)),
        Some("bow-rect") if side == 1 => h * 0.2,
        _ => 0.0,
    }
}

pub fn span(name: &str, w: f64, h: f64, sideways: bool) -> f64 {
    let side = if sideways { h } else { w };
    match canonical(name) {
        Some(
            "notch-rect" | "lin-rect" | "div-rect" | "win-pane" | "notch-pent" | "datastore"
            | "console" | "browser" | "tag-rect",
        ) => (side - 24.0).max(0.0),
        Some("st-rect") => (side - 32.0).max(0.0),
        Some("f-circ" | "cross-circ") => side * 0.6,
        Some("lin-cyl") => {
            if sideways {
                (h - 44.0).max(0.0)
            } else {
                w * 0.6
            }
        }
        _ => 0.0,
    }
}

fn path(d: &str, attrs: &str) -> String {
    format!("<path d=\"{d}\"{attrs}/>")
}

fn poly(points: &[f64], attrs: &str) -> String {
    let points = points
        .as_chunks::<2>()
        .0
        .iter()
        .map(|point| format!("{},{}", num(point[0]), num(point[1])))
        .collect::<Vec<_>>()
        .join(" ");
    format!("<polygon points=\"{points}\"{attrs}/>")
}

fn rect(w: f64, h: f64, attrs: &str, radius: Option<f64>) -> String {
    let radius = radius.map_or_else(String::new, |radius| format!(" rx=\"{}\"", num(radius)));
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs}{radius}/>",
        num(-w / 2.0),
        num(-h / 2.0),
        num(w),
        num(h)
    )
}

fn wave(w: f64, h: f64, attrs: &str) -> String {
    let x = w / 2.0;
    let top = -h / 2.0;
    let base = h / 2.0 - 10.0;
    let q = w / 4.0;
    path(
        &format!(
            "M{},{}H{}V{}q{},-12 {},0t{},0Z",
            num(-x),
            num(top),
            num(x),
            num(base),
            num(-q),
            num(-2.0 * q),
            num(-2.0 * q)
        ),
        attrs,
    )
}

fn brace(x: f64, h: f64, direction: f64, line: &str) -> String {
    let y = h / 2.0;
    let k = 6.0 * direction;
    path(
        &format!(
            "M{},{}q{},0 {},6V-6q0,6 {},6q{},0 {},6V{}q0,6 {},6",
            num(x),
            num(-y),
            num(k),
            num(k),
            num(k),
            num(-k),
            num(-k),
            num(y - 6.0),
            num(-k)
        ),
        &format!(" fill=\"none\"{line}"),
    )
}

fn without_stroke(attrs: &str) -> String {
    if let Some(start) = attrs.find(" stroke=\"") {
        let value_start = start + " stroke=\"".len();
        if let Some(length) = attrs[value_start..].find('"') {
            return format!(
                "{} stroke=\"none\"{}",
                &attrs[..start],
                &attrs[value_start + length + 1..]
            );
        }
    }
    format!("{attrs} stroke=\"none\"")
}

pub fn draw(
    name: &str,
    w: f64,
    h: f64,
    attrs: &str,
    line_attrs: &str,
    radius: f64,
) -> Option<String> {
    draw_with_line_color(name, w, h, attrs, line_attrs, radius, "currentColor")
}

pub fn draw_with_line_color(
    name: &str,
    w: f64,
    h: f64,
    attrs: &str,
    line: &str,
    radius: f64,
    line_color: &str,
) -> Option<String> {
    let name = canonical(name)?;
    let x = w / 2.0;
    let y = h / 2.0;
    Some(match name {
        "f-circ" => format!(
            "<circle r=\"{}\" fill=\"{}\"{line}/>",
            num(x),
            esc(line_color)
        ),
        "notch-rect" => poly(&[-x + 10.0, -y, x, -y, x, y, -x, y, -x, -y + 10.0], attrs),
        "lin-rect" => {
            rect(w, h, attrs, Some(radius))
                + &path(
                    &format!("M{},{}V{}", num(-x + 8.0), num(-y), num(y)),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "div-rect" => {
            rect(w, h, attrs, Some(radius))
                + &path(
                    &format!("M{},{}H{}", num(-x), num(-y + 8.0), num(x)),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "win-pane" => {
            rect(w, h, attrs, Some(radius))
                + &path(
                    &format!(
                        "M{},{}V{}M{},{}H{}",
                        num(-x + 8.0),
                        num(-y),
                        num(y),
                        num(-x),
                        num(-y + 8.0),
                        num(x)
                    ),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "tri" => poly(&[0.0, -y, x, y, -x, y], attrs),
        "flip-tri" => poly(&[-x, -y, x, -y, 0.0, y], attrs),
        "sl-rect" => poly(&[-x, -y + 10.0, x, -y, x, y, -x, y], attrs),
        "notch-pent" => poly(
            &[
                -x + 10.0,
                -y,
                x - 10.0,
                -y,
                x,
                -y + 10.0,
                x,
                y,
                -x,
                y,
                -x,
                -y + 10.0,
            ],
            attrs,
        ),
        "hourglass" => poly(&[-x, -y, x, -y, -x, y, x, y], attrs),
        "delay" => path(
            &format!(
                "M{},{}H{}A{},{} 0 0 1 {},{}H{}Z",
                num(-x),
                num(-y),
                num(x - y),
                num(y),
                num(y),
                num(x - y),
                num(y),
                num(-x)
            ),
            attrs,
        ),
        "h-cyl" => {
            let rx = 12.0_f64.min(h / 4.0);
            let left = -x + rx;
            let right = x - rx;
            path(
                &format!(
                    "M{},{}H{}A{},{} 0 0 1 {},{}H{}A{},{} 0 0 1 {},{}Z",
                    num(left),
                    num(-y),
                    num(right),
                    num(rx),
                    num(y),
                    num(right),
                    num(y),
                    num(left),
                    num(rx),
                    num(y),
                    num(left),
                    num(-y)
                ),
                attrs,
            ) + &path(
                &format!(
                    "M{},{}A{},{} 0 0 0 {},{}",
                    num(right),
                    num(-y),
                    num(rx),
                    num(y),
                    num(right),
                    num(y)
                ),
                &format!(" fill=\"none\"{line}"),
            )
        }
        "cross-circ" => {
            let k = x * std::f64::consts::FRAC_1_SQRT_2;
            format!("<circle r=\"{}\"{attrs}/>", num(x))
                + &path(
                    &format!(
                        "M{},{}L{},{}M{},{}L{},{}",
                        num(-k),
                        num(-k),
                        num(k),
                        num(k),
                        num(k),
                        num(-k),
                        num(-k),
                        num(k)
                    ),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "st-rect" => {
            let mut out = String::new();
            for index in 0..3 {
                let _ = write!(
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs} rx=\"{}\"/>",
                    num(-x + f64::from(2 - index) * 4.0),
                    num(-y + f64::from(index) * 4.0),
                    num(w - 8.0),
                    num(h - 8.0),
                    num(radius)
                );
            }
            out
        }
        "doc" => wave(w, h, attrs),
        "flag" => {
            let y = y - 8.0;
            let q = w / 4.0;
            path(
                &format!(
                    "M{},{}q{},-10 {},0t{},0V{}q{},10 {},0t{},0Z",
                    num(-x),
                    num(-y),
                    num(q),
                    num(2.0 * q),
                    num(2.0 * q),
                    num(y),
                    num(-q),
                    num(-2.0 * q),
                    num(-2.0 * q)
                ),
                attrs,
            )
        }
        "datastore" => {
            rect(w, h, &without_stroke(attrs), None)
                + &path(
                    &format!(
                        "M{},{}H{}M{},{}H{}",
                        num(-x),
                        num(-y),
                        num(x),
                        num(-x),
                        num(y),
                        num(x)
                    ),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "folder" => {
            let tab = (w * 0.4).min(48.0);
            path(
                &format!(
                    "M{},{}H{}L{},{}H{}V{}H{}Z",
                    num(-x),
                    num(-y),
                    num(-x + tab),
                    num(-x + tab + 8.0),
                    num(-y + 8.0),
                    num(x),
                    num(y),
                    num(-x)
                ),
                attrs,
            )
        }
        "bucket" => {
            let taper = h / 6.0;
            path(
                &format!(
                    "M{},{}A{},5 0 0 1 {},{}L{},{}A{},5 0 0 1 {},{}Z",
                    num(-x),
                    num(-y + 5.0),
                    num(x),
                    num(x),
                    num(-y + 5.0),
                    num(x - taper),
                    num(y - 5.0),
                    num(x - taper),
                    num(-x + taper),
                    num(y - 5.0)
                ),
                attrs,
            ) + &path(
                &format!(
                    "M{},{}A{},5 0 0 0 {},{}",
                    num(-x),
                    num(-y + 5.0),
                    num(x),
                    num(x),
                    num(-y + 5.0)
                ),
                &format!(" fill=\"none\"{line}"),
            )
        }
        "console" => {
            rect(w, h, attrs, Some(radius))
                + &path(
                    &format!(
                        "M{},{}H{}M{},{}l3,2.5l-3,2.5M{},{}h4",
                        num(-x),
                        num(-y + 12.0),
                        num(x),
                        num(-x + 7.0),
                        num(-y + 4.0),
                        num(-x + 13.0),
                        num(-y + 9.0)
                    ),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "browser" => {
            let mut out = rect(w, h, attrs, Some(radius))
                + &path(
                    &format!("M{},{}H{}", num(-x), num(-y + 12.0), num(x)),
                    &format!(" fill=\"none\"{line}"),
                );
            for index in 0..3 {
                let _ = write!(
                    out,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"1.5\" fill=\"none\"{line}/>",
                    num(-x + 8.0 + f64::from(index) * 7.0),
                    num(-y + 6.0)
                );
            }
            out
        }
        "person" => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"12\"{attrs}/><circle cy=\"{}\" r=\"13\"{attrs}/>",
            num(-x),
            num(-y + 26.0),
            num(w),
            num(h - 26.0),
            num(-y + 14.0)
        ),
        "bang" => {
            let mut points = Vec::new();
            for index in 0_u32..28 {
                let angle =
                    std::f64::consts::PI * f64::from(index) / 14.0 - std::f64::consts::FRAC_PI_2;
                let k = if index.is_multiple_of(2) { 1.0 } else { 0.82 };
                points.extend_from_slice(&[angle.cos() * x * k, angle.sin() * y * k]);
            }
            poly(&points, attrs)
        }
        "cloud" => {
            let r = y * 0.55;
            path(
                &format!(
                    "M{},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}Z",
                    num(-x + r),
                    num(y),
                    num(r),
                    num(r),
                    num(-x + r * 0.6),
                    num(-y * 0.1),
                    num(x * 0.42),
                    num(y * 0.7),
                    num(-x * 0.1),
                    num(-y * 0.72),
                    num(x * 0.4),
                    num(y * 0.62),
                    num(x * 0.62),
                    num(-y * 0.42),
                    num(r * 1.05),
                    num(r * 1.05),
                    num(x - r),
                    num(y)
                ),
                attrs,
            )
        }
        "brace" => {
            rect(w, h, " fill=\"none\" stroke=\"none\"", None) + &brace(-x + 12.0, h, -1.0, line)
        }
        "brace-r" => {
            rect(w, h, " fill=\"none\" stroke=\"none\"", None) + &brace(x - 12.0, h, 1.0, line)
        }
        "braces" => {
            rect(w, h, " fill=\"none\" stroke=\"none\"", None)
                + &brace(-x + 12.0, h, -1.0, line)
                + &brace(x - 12.0, h, 1.0, line)
        }
        "bolt" => poly(
            &[
                w * 0.15,
                -y,
                -x,
                h * 0.08,
                -w * 0.05,
                h * 0.08,
                -w * 0.15,
                y,
                x,
                -h * 0.08,
                w * 0.05,
                -h * 0.08,
            ],
            attrs,
        ),
        "lin-cyl" => {
            let ry = (w / 14.0).clamp(6.0, 12.0);
            let top = -y + ry;
            let bottom = y - ry;
            let arc = |value| {
                format!(
                    "M{},{}A{},{} 0 0 0 {},{}",
                    num(-x),
                    num(value),
                    num(x),
                    num(ry),
                    num(x),
                    num(value)
                )
            };
            path(
                &format!(
                    "M{},{}A{},{} 0 0 1 {},{}V{}A{},{} 0 0 1 {},{}Z",
                    num(-x),
                    num(top),
                    num(x),
                    num(ry),
                    num(x),
                    num(top),
                    num(bottom),
                    num(x),
                    num(ry),
                    num(-x),
                    num(bottom)
                ),
                attrs,
            ) + &path(
                &(arc(top) + &arc(top + 8.0)),
                &format!(" fill=\"none\"{line}"),
            )
        }
        "curv-trap" => {
            let k = h * 0.3;
            path(
                &format!(
                    "M{},{}H{}A{},{} 0 0 1 {},{}H{}L{},0Z",
                    num(-x + k),
                    num(-y),
                    num(x - k),
                    num(k),
                    num(y),
                    num(x - k),
                    num(y),
                    num(-x + k),
                    num(-x)
                ),
                attrs,
            )
        }
        "docs" => {
            let mut out = String::new();
            for index in 0..3 {
                let _ = write!(
                    out,
                    "<g transform=\"translate({},{})\">{}</g>",
                    (1 - index) * 4,
                    (index - 1) * 4,
                    wave(w - 8.0, h - 8.0, attrs)
                );
            }
            out
        }
        "bow-rect" => {
            let k = h * 0.2;
            path(
                &format!(
                    "M{},{}H{}A{},{} 0 0 0 {},{}H{}A{},{} 0 0 1 {},{}Z",
                    num(-x + k),
                    num(-y),
                    num(x),
                    num(k),
                    num(y),
                    num(x),
                    num(y),
                    num(-x + k),
                    num(k),
                    num(y),
                    num(-x + k),
                    num(-y)
                ),
                attrs,
            )
        }
        "tag-rect" => {
            rect(w, h, attrs, Some(radius))
                + &path(
                    &format!("M{},{}L{},{}", num(x - 12.0), num(y), num(x), num(y - 12.0)),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "tag-doc" => {
            wave(w, h, attrs)
                + &path(
                    &format!(
                        "M{},{}L{},{}",
                        num(x - 12.0),
                        num(y - 14.0),
                        num(x),
                        num(y - 26.0)
                    ),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "lin-doc" => {
            wave(w, h, attrs)
                + &path(
                    &format!("M{},{}V{}", num(-x + 8.0), num(-y), num(y - 6.0)),
                    &format!(" fill=\"none\"{line}"),
                )
        }
        "collapsedGroup" => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs} rx=\"{}\"/><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs} rx=\"{}\"/>",
            num(-x + 6.0),
            num(-y + 8.0),
            num(w - 12.0),
            num(h - 8.0),
            num(radius),
            num(-x),
            num(-y),
            num(w),
            num(h - 4.0),
            num(radius)
        ),
        _ => unreachable!(),
    })
}

pub fn end_side(route: &[f64], at: usize, c: &CNode) -> Option<usize> {
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if (dy.abs() - c.h / 2.0).abs() < 0.5 {
        Some(if dy < 0.0 { 0 } else { 2 })
    } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
        Some(if dx < 0.0 { 3 } else { 1 })
    } else {
        None
    }
}

pub fn inset_route(
    route: &mut [f64],
    at: usize,
    w: f64,
    h: f64,
    c: &CNode,
    inset: impl Fn(usize, f64) -> f64,
) {
    if c.is_group {
        return;
    }
    match end_side(route, at, c) {
        Some(side @ (0 | 2)) => {
            let amount = inset(side, route[at] - c.x) + (c.h - h) / 2.0;
            route[at + 1] += if side == 0 { amount } else { -amount };
        }
        Some(side) => {
            let amount = inset(side, route[at + 1] - c.y) + (c.w - w) / 2.0;
            route[at] += if side == 3 { amount } else { -amount };
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Element {
        name: String,
        attributes: Vec<(String, String)>,
        children: Vec<Self>,
    }

    fn xml(svg: &str) -> Element {
        fn element(node: roxmltree::Node<'_, '_>) -> Element {
            let mut attributes: Vec<_> = node
                .attributes()
                .map(|attribute| (attribute.name().to_owned(), attribute.value().to_owned()))
                .collect();
            attributes.sort();
            Element {
                name: node.tag_name().name().to_owned(),
                attributes,
                children: node
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .map(element)
                    .collect(),
            }
        }
        let wrapped = format!("<svg>{svg}</svg>");
        let document = roxmltree::Document::parse(&wrapped).unwrap();
        element(document.root_element())
    }

    #[test]
    fn extended_shape_geometry_matches_pinned_upstream() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/shape_geometry.json")).unwrap();
        assert_eq!(fixture["upstreamRevision"], crate::UPSTREAM_REVISION);
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let tw = case["tw"].as_f64().unwrap();
            let th = case["th"].as_f64().unwrap();
            let (w, h, dy) = size(name, tw, th).unwrap();
            for (key, actual) in [("w", w), ("h", h), ("dy", dy)] {
                assert!(
                    (actual - case[key].as_f64().unwrap()).abs() < 1e-8,
                    "{name} {key}"
                );
            }
            for side in 0..4 {
                assert!(
                    (inset(name, w, h, side, 7.0) - case["insets"][side].as_f64().unwrap()).abs()
                        < 1e-8,
                    "{name} inset {side}"
                );
            }
            for (index, sideways) in [false, true].into_iter().enumerate() {
                assert!(
                    (span(name, w, h, sideways) - case["span"][index].as_f64().unwrap()).abs()
                        < 1e-8,
                    "{name} span {index}"
                );
            }
            assert_eq!(has_label(name), case["label"].as_bool().unwrap(), "{name}");
            assert_eq!(filled(name), case["filled"].as_bool().unwrap(), "{name}");
            let actual = draw_with_line_color(
                name,
                w,
                h,
                " fill=\"#f3f3f3\" stroke=\"#8a8a8a\"",
                " stroke=\"#8a8a8a\"",
                4.0,
                "#1f1f1f",
            )
            .unwrap();
            assert_eq!(
                xml(&actual),
                xml(case["svg"].as_str().unwrap()),
                "{name} at {tw} by {th}"
            );
        }
    }

    #[test]
    fn document_aliases_have_distinct_geometry_and_label_baselines() {
        assert_eq!(canonical("document"), Some("doc"));
        assert_eq!(canonical("stacked-document"), Some("docs"));
        assert_eq!(size("doc", 100.0, 24.0), Some((136.0, 58.0, -5.0)));
        let document = draw("doc", 136.0, 58.0, "", "", 4.0).unwrap();
        assert!(document.contains("q-34,-12 -68,0t-68,0Z"));
        assert_eq!(
            draw("docs", 144.0, 66.0, "", "", 4.0)
                .unwrap()
                .matches("translate(")
                .count(),
            3
        );
    }

    #[test]
    fn triangles_fit_the_label_and_flip_its_vertical_shift() {
        let up = size("tri", 80.0, 24.0).unwrap();
        let down = size("flip-tri", 80.0, 24.0).unwrap();
        assert_eq!(up.0, down.0);
        assert_eq!(up.1, down.1);
        assert_eq!(up.2, -down.2);
        assert!(up.2 > 0.0);
        assert_ne!(
            draw("tri", up.0, up.1, "", "", 4.0),
            draw("flip-tri", down.0, down.1, "", "", 4.0)
        );
    }

    #[test]
    fn every_named_shape_has_finite_native_static_geometry() {
        for name in SHAPES {
            let (w, h, dy) = size(name, 80.0, 24.0).unwrap();
            assert!(w > 0.0 && h > 0.0 && dy.is_finite(), "{name}");
            let svg = draw(
                name,
                w,
                h,
                " fill=\"#fff\" stroke=\"#111\"",
                " stroke=\"#111\"",
                4.0,
            )
            .unwrap();
            assert!(svg.starts_with('<') && svg.ends_with('>'), "{name}");
            assert!(!svg.contains("var(") && !svg.contains("NaN"), "{name}");
            assert!(span(name, w, h, false) >= 0.0, "{name}");
        }
        assert!(!is_supported("constructor"));
        assert_eq!(draw("not-a-shape", 80.0, 24.0, "", "", 4.0), None);
    }

    #[test]
    fn bare_braces_and_symbols_do_not_draw_labels_or_style_fills() {
        assert!(!filled("comment"));
        assert!(!has_label("junction"));
        assert!(!has_label("lightning-bolt"));
        let brace = draw(
            "brace",
            100.0,
            48.0,
            " fill=\"red\"",
            " stroke=\"blue\"",
            4.0,
        )
        .unwrap();
        assert!(!brace.contains("red"));
        let junction =
            draw_with_line_color("junction", 14.0, 14.0, "", "", 4.0, "#123456").unwrap();
        assert!(junction.contains("fill=\"#123456\""));
    }
}
