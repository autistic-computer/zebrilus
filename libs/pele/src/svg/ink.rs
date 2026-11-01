use std::sync::LazyLock;

use regex::Regex;

use crate::svg::esc;
use crate::svg::theme::ResolvedStyle;

const DARK: &str = "black blue blueviolet brown crimson darkblue darkcyan darkgreen darkmagenta darkolivegreen darkorchid darkred darkslateblue darkslategray darkviolet dimgray firebrick forestgreen green indigo maroon mediumblue mediumslateblue mediumvioletred midnightblue navy olive purple rebeccapurple royalblue saddlebrown seagreen sienna slateblue steelblue teal";
const LIGHT: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque blanchedalmond burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk cyan darkgoldenrod darkgray darkkhaki darkorange darksalmon darkseagreen darkturquoise deeppink deepskyblue dodgerblue floralwhite fuchsia gainsboro ghostwhite gold goldenrod gray greenyellow honeydew hotpink indianred ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightpink lightsalmon lightseagreen lightskyblue lightslategray lightsteelblue lightyellow lime limegreen linen magenta mediumaquamarine mediumorchid mediumpurple mediumseagreen mediumspringgreen mediumturquoise mintcream mistyrose moccasin navajowhite oldlace olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue red rosybrown salmon sandybrown seashell silver skyblue slategray snow springgreen tan thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen";

pub const ON_LIGHT: &str = "#1f1f1f";
pub const ON_DARK: &str = "#fff";

static RE_FILL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[";]fill:([^;"]*)"#).unwrap());
static RE_OPACITIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[";](?:fill-)?opacity:([^;"]*)"#).unwrap());
static RE_HEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$").unwrap());
static RE_FUNCTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(rgba?|hsla?)\(([^()]*)\)$").unwrap());
static RE_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^[+-]?(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:e[+-]?[0-9]+)?(%|deg|grad|rad|turn)?$",
    )
    .unwrap()
});

fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

static EVEN: LazyLock<f64> = LazyLock::new(|| {
    let dark = 31.0 / 255.0;
    (1.05 * (luminance(dark, dark, dark) + 0.05)).sqrt() - 0.05
});

fn number_part(raw: &str, unit: usize) -> f64 {
    raw[..raw.len() - unit].parse().unwrap_or(f64::NAN)
}

fn amount(raw: &str, full: f64) -> f64 {
    let Some(m) = RE_NUMBER.captures(raw) else {
        return f64::NAN;
    };
    let unit = m.get(1).map(|u| u.as_str());
    if unit.is_some_and(|u| u != "%") {
        return f64::NAN;
    }
    let value = number_part(raw, unit.map_or(0, str::len));
    (value / if unit.is_some() { 100.0 } else { full }).clamp(0.0, 1.0)
}

fn hue(raw: &str) -> f64 {
    let Some(m) = RE_NUMBER.captures(raw) else {
        return f64::NAN;
    };
    let unit = m.get(1).map(|u| u.as_str().to_ascii_lowercase());
    if unit.as_deref() == Some("%") {
        return f64::NAN;
    }
    let n = number_part(raw, unit.as_ref().map_or(0, String::len));
    match unit.as_deref() {
        Some("turn") => n * 360.0,
        Some("rad") => n * 180.0 / std::f64::consts::PI,
        Some("grad") => n * 0.9,
        _ => n,
    }
}

fn args(body: &str, hsl: bool) -> Option<Vec<String>> {
    let legacy = body.contains(',');
    let (main, alpha) = if legacy {
        (body, None)
    } else {
        let mut parts = body.split('/');
        let main = parts.next().unwrap_or("");
        let alpha = parts.next();
        if parts.next().is_some() {
            return None;
        }
        (main, alpha)
    };
    let mut list: Vec<String> = if legacy {
        main.trim()
            .split(',')
            .map(|c| c.trim().to_owned())
            .collect()
    } else {
        main.split_whitespace().map(str::to_owned).collect()
    };
    if if legacy {
        list.len() < 3 || list.len() > 4
    } else {
        list.len() != 3
    } {
        return None;
    }
    if let Some(alpha) = alpha {
        list.push(alpha.trim().to_owned());
    }
    if !legacy {
        return Some(
            list.into_iter()
                .map(|c| if c == "none" { "0".to_owned() } else { c })
                .collect(),
        );
    }
    let percent = list[usize::from(hsl)..3]
        .iter()
        .filter(|c| c.ends_with('%'))
        .count();
    (percent == if hsl { 2 } else { 3 } || (!hsl && percent == 0)).then_some(list)
}

fn color(raw: &str) -> Option<(f64, f64)> {
    let value = raw.trim().to_lowercase();
    let name = value.replacen("grey", "gray", 1);
    if DARK.split(' ').any(|known| known == name) {
        return Some((0.0, 1.0));
    }
    if LIGHT.split(' ').any(|known| known == name) {
        return Some((1.0, 1.0));
    }
    if RE_HEX.is_match(&value) {
        let digits: String = if value.len() < 6 {
            value[1..].chars().flat_map(|c| [c, c]).collect()
        } else {
            value[1..].to_owned()
        };
        let n = |i: usize| {
            f64::from(u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).unwrap_or(0)) / 255.0
        };
        return Some((
            luminance(n(0), n(1), n(2)),
            if digits.len() == 8 { n(3) } else { 1.0 },
        ));
    }
    let function = RE_FUNCTION.captures(&value)?;
    let hsl = function[1].starts_with("hsl");
    let list = args(&function[2], hsl)?;
    let alpha = if list.len() == 4 {
        amount(&list[3], 1.0)
    } else {
        1.0
    };
    let rgb: Vec<f64> = if hsl {
        let h = ((hue(&list[0]) % 360.0) + 360.0) % 360.0 / 30.0;
        let s = amount(&list[1], 100.0);
        let l = amount(&list[2], 100.0);
        let a = s * l.min(1.0 - l);
        [0.0, 8.0, 4.0]
            .iter()
            .map(|k| {
                let t = (k + h) % 12.0;
                l - a * (t - 3.0).min(9.0 - t).clamp(-1.0, 1.0)
            })
            .collect()
    } else {
        list[..3].iter().map(|c| amount(c, 255.0)).collect()
    };
    let lum = luminance(rgb[0], rgb[1], rgb[2]);
    (!(lum + alpha).is_nan()).then_some((lum, alpha))
}

fn opacity(shape: &str) -> f64 {
    RE_OPACITIES
        .captures_iter(shape)
        .map(|m| amount(m[1].trim(), 1.0))
        .product()
}

pub fn ink(shape: &str, text: &str) -> Option<&'static str> {
    let fill = RE_FILL.captures(shape)?;
    if RE_FILL.is_match(text) {
        return None;
    }
    let (lum, alpha) = color(&fill[1])?;
    let opaque = alpha * opacity(shape);
    if opaque.is_nan() || opaque < 0.5 {
        return None;
    }
    Some(if lum > *EVEN { ON_LIGHT } else { ON_DARK })
}

pub fn paints(shape: &str) -> bool {
    let Some(fill) = RE_FILL.captures(shape) else {
        return false;
    };
    let fill = fill[1].trim().to_lowercase();
    if fill.is_empty() || fill == "none" || fill == "transparent" {
        return false;
    }
    let scale = opacity(shape);
    color(&fill).map_or(1.0, |(_, alpha)| alpha) * if scale.is_nan() { 1.0 } else { scale } > 0.0
}

pub fn ink_fill(style: &ResolvedStyle, fallback: Option<&str>) -> String {
    ink(&style.shape, &style.text)
        .map(str::to_owned)
        .or_else(|| fallback.map(esc))
        .map_or_else(String::new, |color| format!(" fill=\"{color}\""))
}

pub fn mix(first: &str, second: &str, share: f64) -> String {
    let parse = |value: &str| value.trim().parse::<svgtypes::Color>().ok();
    let (Some(a), Some(b)) = (parse(first), parse(second)) else {
        return esc(first);
    };
    let p = share.clamp(0.0, 1.0);
    let (aa, ba) = (f64::from(a.alpha) / 255.0, f64::from(b.alpha) / 255.0);
    let alpha = aa * p + ba * (1.0 - p);
    if alpha <= 0.0 {
        return "none".to_owned();
    }
    let channel = |x: u8, y: u8| {
        let value = (f64::from(x) * aa * p + f64::from(y) * ba * (1.0 - p)) / alpha;
        value.round().clamp(0.0, 255.0) as u8
    };
    let (r, g, bl) = (
        channel(a.red, b.red),
        channel(a.green, b.green),
        channel(a.blue, b.blue),
    );
    if alpha >= 1.0 {
        format!("#{r:02x}{g:02x}{bl:02x}")
    } else {
        format!("rgba({r},{g},{bl},{})", crate::svg::num(alpha))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::theme::resolve_style;

    fn shape(declarations: &[&str]) -> String {
        resolve_style(declarations).shape
    }

    #[test]
    fn ink_picks_contrast_for_concrete_fills() {
        assert_eq!(ink(&shape(&["fill:#000"]), ""), Some(ON_DARK));
        assert_eq!(ink(&shape(&["fill:#fff"]), ""), Some(ON_LIGHT));
        assert_eq!(ink(&shape(&["fill:navy"]), ""), Some(ON_DARK));
        assert_eq!(ink(&shape(&["fill:lightgrey"]), ""), Some(ON_LIGHT));
        assert_eq!(ink(&shape(&["fill:rgb(10, 10, 10)"]), ""), Some(ON_DARK));
        assert_eq!(ink(&shape(&["fill:hsl(0 0% 90%)"]), ""), Some(ON_LIGHT));
        assert_eq!(ink(&shape(&["fill:#000", "fill-opacity:0.2"]), ""), None);
        assert_eq!(ink(&shape(&["fill:#000"]), " style=\"fill:#f00;\""), None);
        assert_eq!(ink(&shape(&["stroke:#000"]), ""), None);
    }

    #[test]
    fn paints_ignores_transparent_fills() {
        assert!(paints(&shape(&["fill:#f00"])));
        assert!(!paints(&shape(&["fill:none"])));
        assert!(!paints(&shape(&["fill:#f00", "opacity:0"])));
        assert!(paints(&shape(&["fill:currentColor"])));
    }

    #[test]
    fn mixing_is_concrete() {
        assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(mix("#e6e6e6", "#ffffff", 1.0), "#e6e6e6");
    }
}
