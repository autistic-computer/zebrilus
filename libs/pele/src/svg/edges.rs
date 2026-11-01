use std::collections::HashMap;
use std::fmt::Write;
use std::sync::LazyLock;

use regex::Regex;

use crate::RenderOptions;
use crate::compound::{CEdge, CNode};
use crate::layout::ease;
use crate::svg::ink::ink;
use crate::svg::theme::parse_float;
use crate::svg::{esc, esc_text, label_with_options, num};
use crate::text::Label;

pub const ARROW: f64 = 8.0;
pub const LEAD: f64 = 6.0;
const CORNER: f64 = 12.0;
const HANDLE: f64 = 0.62;
const CASE: f64 = 4.0;
const TENS: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

static RE_STROKE_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[";]stroke:"#).unwrap());
static RE_FILL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[";]fill:([^;"]*)"#).unwrap());
static RE_WIDTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9.]+(?:px)?$").unwrap());

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgePath {
    pub d: String,
    pub tokens: Option<Vec<f64>>,
    pub sx: f64,
    pub sy: f64,
    pub sdx: f64,
    pub sdy: f64,
    pub ex: f64,
    pub ey: f64,
    pub edx: f64,
    pub edy: f64,
}

#[derive(Clone, Debug, Default)]
pub struct SideReach {
    reach: HashMap<usize, f64>,
}

impl SideReach {
    pub fn add(&mut self, node: usize, side: Option<usize>, kind: &str) {
        let trim = marker_trim(kind);
        if let Some(side) = side {
            let key = node * 4 + side;
            if trim > 0.0 && trim + LEAD > self.reach.get(&key).copied().unwrap_or(0.0) {
                self.reach.insert(key, trim + LEAD);
            }
        }
    }

    pub fn of(&self, node: usize, side: Option<usize>) -> f64 {
        side.and_then(|side| self.reach.get(&(node * 4 + side)).copied())
            .unwrap_or(0.0)
    }
}

pub fn side_reach() -> SideReach {
    SideReach::default()
}

fn sign_or_one(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

fn written(value: f64) -> f64 {
    let scaled = value * 100.0;
    let floor = scaled.floor();
    let n = if scaled - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    };
    if n == 0.0 { 0.0 } else { n / 100.0 }
}

struct Points {
    xs: Vec<f64>,
    ys: Vec<f64>,
    linear: bool,
}

impl Points {
    fn aligned(&self, i: usize) -> bool {
        (self.xs[i] - self.xs[i - 1]).abs() < 0.01 || (self.ys[i] - self.ys[i - 1]).abs() < 0.01
    }

    fn straight(&self, i: usize) -> bool {
        self.linear || self.aligned(i)
    }

    fn length(&self, i: usize) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() + (self.ys[i] - self.ys[i - 1]).abs()
    }

    fn turns(&self, i: usize, last: usize) -> bool {
        !self.linear
            && i > 0
            && i < last
            && self.aligned(i)
            && self.aligned(i + 1)
            && self.length(i) >= 0.01
            && self.length(i + 1) >= 0.01
            && ((self.xs[i] - self.xs[i - 1]).abs() < 0.01)
                != ((self.xs[i + 1] - self.xs[i]).abs() < 0.01)
    }

    fn unit(&self, i: usize, j: usize, axis: f64) -> (f64, f64) {
        if !self.straight(i.max(j)) {
            return if axis == 0.0 {
                (0.0, sign_or_one(self.ys[j] - self.ys[i]))
            } else {
                (sign_or_one(self.xs[j] - self.xs[i]), 0.0)
            };
        }
        let dx = self.xs[j] - self.xs[i];
        let dy = self.ys[j] - self.ys[i];
        let len = dx.hypot(dy);
        let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
        (dx / len, dy / len)
    }

    fn along(&self, i: usize, ux: f64, uy: f64) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() * ux.abs()
            + (self.ys[i] - self.ys[i - 1]).abs() * uy.abs()
    }
}

fn tok(tokens: &mut Vec<f64>, value: f64) -> String {
    tokens.push(written(value));
    num(value)
}

fn line_to(d: &mut String, tokens: &mut Vec<f64>, at: &mut (f64, f64), x: f64, y: f64) {
    if near(x, y, at.0, at.1) {
        return;
    }
    tokens.extend([76.0, 2.0]);
    let _ = write!(d, "L{},{}", tok(tokens, x), tok(tokens, y));
    *at = (x, y);
}

fn near(ax: f64, ay: f64, bx: f64, by: f64) -> bool {
    (ax - bx).abs() < 0.01 && (ay - by).abs() < 0.01
}

pub fn route_path(
    route: &[f64],
    curve: Option<&str>,
    start_trim: f64,
    end_trim: f64,
    start_reach: Option<f64>,
    end_reach: Option<f64>,
) -> EdgePath {
    let start_reach = start_reach.unwrap_or(if start_trim > 0.0 {
        start_trim + LEAD
    } else {
        0.0
    });
    let end_reach = end_reach.unwrap_or(if end_trim > 0.0 { end_trim + LEAD } else { 0.0 });
    let linear = curve == Some("linear");
    let stepped = matches!(curve, Some("step" | "stepBefore" | "stepAfter"));
    let mut p = Points {
        xs: Vec::new(),
        ys: Vec::new(),
        linear,
    };
    let mut axes = Vec::new();
    for point in route.as_chunks::<3>().0 {
        let [x, y, axis] = *point;
        if stepped
            && let (Some(&px), Some(&py)) = (p.xs.last(), p.ys.last())
            && (x - px).abs() >= 0.01
            && (y - py).abs() >= 0.01
        {
            let my = match curve {
                Some("stepBefore") => py,
                Some("stepAfter") => y,
                _ => (py + y) / 2.0,
            };
            let mx = match curve {
                Some("stepBefore") => px,
                Some("stepAfter") => x,
                _ => (px + x) / 2.0,
            };
            let turns = if axis == 0.0 {
                [px, my, x, my]
            } else {
                [mx, py, mx, y]
            };
            for k in [0, 2] {
                if near(turns[k], turns[k + 1], px, py) || near(turns[k], turns[k + 1], x, y) {
                    continue;
                }
                p.xs.push(turns[k]);
                p.ys.push(turns[k + 1]);
                axes.push(axis);
            }
        }
        p.xs.push(x);
        p.ys.push(y);
        axes.push(axis);
    }
    if p.xs.len() < 2 {
        let (x, y) = (
            p.xs.first().copied().unwrap_or(0.0),
            p.ys.first().copied().unwrap_or(0.0),
        );
        return EdgePath {
            d: format!("M{},{}", num(x), num(y)),
            tokens: Some(vec![77.0, 2.0, written(x), written(y)]),
            sx: x,
            sy: y,
            ex: x,
            ey: y,
            ..EdgePath::default()
        };
    }
    let mut last = p.xs.len() - 1;
    let (sdx, sdy) = p.unit(0, 1, axes[1]);
    let (edx, edy) = p.unit(last - 1, last, axes[last]);
    let (sx, sy) = (p.xs[0], p.ys[0]);
    let (ex, ey) = (p.xs[last], p.ys[last]);
    p.xs[0] += sdx * start_trim;
    p.ys[0] += sdy * start_trim;
    p.xs[last] -= edx * end_trim;
    p.ys[last] -= edy * end_trim;
    let lead = |reach: f64, trim: f64, run: f64| {
        let room = (reach - LEAD - trim).max(0.0).min(run / 4.0);
        room + (reach - trim - room).min((run - room) / 4.0)
    };
    let arrives = if end_reach > end_trim && (!p.straight(last) || p.turns(last - 1, last)) {
        lead(end_reach, end_trim, p.along(last, edx, edy))
    } else {
        0.0
    };
    let leaves = if start_reach > start_trim && (!p.straight(1) || p.turns(1, last)) {
        lead(start_reach, start_trim, p.along(1, sdx, sdy))
    } else {
        0.0
    };
    if arrives >= 1.0 {
        let (x, y) = (p.xs[last] - edx * arrives, p.ys[last] - edy * arrives);
        p.xs.insert(last, x);
        p.ys.insert(last, y);
        axes.insert(last, axes[last]);
        last += 1;
    }
    if leaves >= 1.0 {
        let (x, y) = (p.xs[0] + sdx * leaves, p.ys[0] + sdy * leaves);
        p.xs.insert(1, x);
        p.ys.insert(1, y);
        axes.insert(1, axes[1]);
        last += 1;
    }

    let mut tokens = vec![77.0, 2.0];
    let mut d = String::new();
    let _ = write!(
        d,
        "M{},{}",
        tok(&mut tokens, p.xs[0]),
        tok(&mut tokens, p.ys[0])
    );
    let mut at = (p.xs[0], p.ys[0]);
    for i in 1..=last {
        let (x, y) = (p.xs[i], p.ys[i]);
        let (at_x, at_y) = at;
        if !p.straight(i) {
            let down = axes[i] == 0.0;
            let along = if down { y - at_y } else { x - at_x };
            let k = along.abs() * ease(if down { x - at_x } else { y - at_y }, along);
            let ux = if down { 0.0 } else { js_sign(along) * k };
            let uy = if down { js_sign(along) * k } else { 0.0 };
            tokens.extend([67.0, 6.0]);
            let _ = write!(
                d,
                "C{},{} {},{} {},{}",
                tok(&mut tokens, at_x + ux),
                tok(&mut tokens, at_y + uy),
                tok(&mut tokens, x - ux),
                tok(&mut tokens, y - uy),
                tok(&mut tokens, x),
                tok(&mut tokens, y)
            );
            at = (x, y);
            continue;
        }
        if !p.turns(i, last) {
            if i < last
                && p.aligned(i)
                && p.aligned(i + 1)
                && (x - p.xs[i - 1]) * (p.xs[i + 1] - x) + (y - p.ys[i - 1]) * (p.ys[i + 1] - y)
                    > 0.0
            {
                continue;
            }
            line_to(&mut d, &mut tokens, &mut at, x, y);
            continue;
        }
        let mut before = if p.turns(i - 1, last) {
            p.length(i) / 2.0
        } else {
            p.length(i)
        };
        let mut after = if p.turns(i + 1, last) {
            p.length(i + 1) / 2.0
        } else {
            p.length(i + 1)
        };
        if stepped {
            before = CORNER.min(before).min(after);
            after = before;
        }
        let (ux, uy) = p.unit(i - 1, i, axes[i]);
        let (vx, vy) = p.unit(i, i + 1, axes[i + 1]);
        line_to(
            &mut d,
            &mut tokens,
            &mut at,
            x - ux * before,
            y - uy * before,
        );
        at = (x + vx * after, y + vy * after);
        if stepped {
            tokens.extend([81.0, 4.0]);
            let _ = write!(
                d,
                "Q{},{} {},{}",
                tok(&mut tokens, x),
                tok(&mut tokens, y),
                tok(&mut tokens, at.0),
                tok(&mut tokens, at.1)
            );
        } else {
            tokens.extend([67.0, 6.0]);
            let _ = write!(
                d,
                "C{},{} {},{} {},{}",
                tok(&mut tokens, x - ux * before * (1.0 - HANDLE)),
                tok(&mut tokens, y - uy * before * (1.0 - HANDLE)),
                tok(&mut tokens, x + vx * after * (1.0 - HANDLE)),
                tok(&mut tokens, y + vy * after * (1.0 - HANDLE)),
                tok(&mut tokens, at.0),
                tok(&mut tokens, at.1)
            );
        }
    }
    EdgePath {
        d,
        tokens: Some(tokens),
        sx,
        sy,
        sdx: -sdx,
        sdy: -sdy,
        ex,
        ey,
        edx,
        edy,
    }
}

pub fn marker_trim(kind: &str) -> f64 {
    match kind {
        "arrow_point" => ARROW - 1.0,
        "arrow_circle" => ARROW,
        "arrow_cross" => 4.0,
        _ => 0.0,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn marker(kind: &str, x: f64, y: f64, dx: f64, dy: f64, color: &str, stroke: &str) -> String {
    let nx = -dy;
    let ny = dx;
    match kind {
        "arrow_point" => {
            let bx = x - dx * ARROW;
            let by = y - dy * ARROW;
            let half = ARROW * 0.42;
            format!(
                "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}Z\" fill=\"{color}\" stroke=\"{stroke}\"/>",
                num(x),
                num(y),
                num(bx + nx * half),
                num(by + ny * half),
                num(bx - nx * half),
                num(by - ny * half)
            )
        }
        "arrow_circle" => format!(
            "<circle class=\"pele-marker\" cx=\"{}\" cy=\"{}\" r=\"3.5\" fill=\"{color}\" stroke=\"{stroke}\"/>",
            num(x - dx * 4.0),
            num(y - dy * 4.0)
        ),
        "arrow_cross" => {
            let cx = x - dx * 4.0;
            let cy = y - dy * 4.0;
            let a = 3.5;
            let ux = (dx + nx) * a;
            let uy = (dy + ny) * a;
            let vx = (dx - nx) * a;
            let vy = (dy - ny) * a;
            format!(
                "<path class=\"pele-marker\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
                num(cx - ux),
                num(cy - uy),
                num(cx + ux),
                num(cy + uy),
                num(cx - vx),
                num(cy - vy),
                num(cx + vx),
                num(cy + vy)
            )
        }
        _ => String::new(),
    }
}

pub fn tokenize(d: &str) -> Vec<f64> {
    let bytes = d.as_bytes();
    let at = |j: usize| bytes.get(j).copied();
    let mut out = Vec::new();
    let mut command: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_uppercase() && c != b'E' {
            command = Some(out.len() + 1);
            out.extend([f64::from(c), 0.0]);
            i += 1;
        } else if c == b'-' || c == b'.' || c.is_ascii_digit() {
            let mut j = if c == b'-' { i + 1 } else { i };
            let mut m = 0.0_f64;
            let mut places: i32 = -1;
            while let Some(k) = at(j) {
                if k.is_ascii_digit() {
                    m = m * 10.0 + f64::from(k - b'0');
                    if places >= 0 {
                        places += 1;
                    }
                } else if k == b'.' && places < 0 {
                    places = 0;
                } else {
                    break;
                }
                j += 1;
            }
            let value =
                if at(j).is_some_and(|k| k | 32 == b'e') || m > MAX_SAFE_INTEGER || places > 22 {
                    while let Some(k) = at(j) {
                        let signed = (k == b'-' || k == b'+')
                            && j > 0
                            && at(j - 1).is_some_and(|prev| prev | 32 == b'e');
                        if k == b'.' || k == b'e' || k == b'E' || k.is_ascii_digit() || signed {
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    d[i..j].parse::<f64>().unwrap_or(f64::NAN)
                } else {
                    let value = if places > 0 {
                        m / TENS[places as usize]
                    } else {
                        m
                    };
                    if c == b'-' { -value } else { value }
                };
            if let Some(at) = command {
                out.push(value);
                out[at] += 1.0;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

pub fn path_points(d: &str) -> Vec<f64> {
    sample(&tokenize(d))
}

pub fn sample(tokens: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    let mut i = 0;
    while i + 1 < tokens.len() {
        let cmd = tokens[i];
        let count = tokens[i + 1] as usize;
        let s = i + 2;
        let e = (s + count).min(tokens.len());
        i = s + count;
        if cmd == 72.0 || cmd == 86.0 {
            for &value in &tokens[s..e] {
                if cmd == 72.0 {
                    x = value;
                } else {
                    y = value;
                }
                out.extend([x, y]);
            }
        } else if cmd == 67.0 || cmd == 81.0 {
            let cubic = cmd == 67.0;
            let step = if cubic { 6 } else { 4 };
            let mut k = s;
            while k + step <= e {
                let (x1, y1, x2, y2) = (tokens[k], tokens[k + 1], tokens[k + 2], tokens[k + 3]);
                let x3 = if cubic { tokens[k + 4] } else { x2 };
                let y3 = if cubic { tokens[k + 5] } else { y2 };
                let mut t = 0.125;
                while t <= 1.0 {
                    let mut ax = x + (x1 - x) * t;
                    let mut ay = y + (y1 - y) * t;
                    let mut bx = x1 + (x2 - x1) * t;
                    let mut by = y1 + (y2 - y1) * t;
                    if cubic {
                        let cx = x2 + (x3 - x2) * t;
                        let cy = y2 + (y3 - y2) * t;
                        ax += (bx - ax) * t;
                        ay += (by - ay) * t;
                        bx += (cx - bx) * t;
                        by += (cy - by) * t;
                    }
                    out.extend([ax + (bx - ax) * t, ay + (by - ay) * t]);
                    t += 0.125;
                }
                x = x3;
                y = y3;
                k += step;
            }
        } else if cmd == 65.0 {
            let mut k = s;
            while k + 7 <= e {
                arc(
                    &mut out,
                    (x, y),
                    tokens[k],
                    tokens[k + 1],
                    tokens[k + 3],
                    tokens[k + 4],
                    (tokens[k + 5], tokens[k + 6]),
                );
                x = tokens[k + 5];
                y = tokens[k + 6];
                k += 7;
            }
        } else {
            let mut k = s;
            while k + 2 <= e {
                if cmd == 77.0 && k == s && !out.is_empty() {
                    out.extend([f64::NAN, f64::NAN]);
                }
                x = tokens[k];
                y = tokens[k + 1];
                out.extend([x, y]);
                k += 2;
            }
        }
    }
    out
}

fn arc(
    out: &mut Vec<f64>,
    from: (f64, f64),
    mut rx: f64,
    mut ry: f64,
    large: f64,
    sweep: f64,
    to: (f64, f64),
) {
    let ((x1, y1), (x2, y2)) = (from, to);
    let hx = (x1 - x2) / 2.0;
    let hy = (y1 - y2) / 2.0;
    if rx == 0.0 || ry == 0.0 || (hx == 0.0 && hy == 0.0) {
        out.extend([x2, y2]);
        return;
    }
    let scale = ((hx * hx) / (rx * rx) + (hy * hy) / (ry * ry))
        .sqrt()
        .max(1.0);
    rx *= scale;
    ry *= scale;
    let k = if large == sweep { -1.0 } else { 1.0 }
        * ((rx * rx * ry * ry - rx * rx * hy * hy - ry * ry * hx * hx)
            / (rx * rx * hy * hy + ry * ry * hx * hx))
            .max(0.0)
            .sqrt();
    let cx = (k * rx * hy) / ry + (x1 + x2) / 2.0;
    let cy = (-k * ry * hx) / rx + (y1 + y2) / 2.0;
    let start = ((y1 - cy) / ry).atan2((x1 - cx) / rx);
    let mut turn = ((y2 - cy) / ry).atan2((x2 - cx) / rx) - start;
    if sweep != 0.0 && turn < 0.0 {
        turn += 2.0 * std::f64::consts::PI;
    } else if sweep == 0.0 && turn > 0.0 {
        turn -= 2.0 * std::f64::consts::PI;
    }
    for i in 1..=8 {
        let angle = start + (turn * f64::from(i)) / 8.0;
        out.extend([cx + rx * angle.cos(), cy + ry * angle.sin()]);
    }
}

fn clips(x: f64, y: f64, dx: f64, dy: f64, bounds: [f64; 4]) -> bool {
    let [l, t, r, b] = bounds;
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for (p, q) in [(-dx, x - l), (dx, r - x), (-dy, y - t), (dy, b - y)] {
        if p == 0.0 {
            if q < 0.0 {
                hi = -1.0;
            }
        } else if p < 0.0 {
            lo = lo.max(q / p);
        } else {
            hi = hi.min(q / p);
        }
    }
    lo < hi
}

fn enters(points: &[f64], bounds: [f64; 4]) -> bool {
    let [l, t, r, b] = bounds;
    let mut k = 0;
    while k + 2 < points.len() {
        let (x, y, x2, y2) = (points[k], points[k + 1], points[k + 2], points[k + 3]);
        k += 2;
        if (x < l && x2 < l) || (x > r && x2 > r) || (y < t && y2 < t) || (y > b && y2 > b) {
            continue;
        }
        if clips(x, y, x2 - x, y2 - y, bounds) {
            return true;
        }
    }
    false
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub bounds: [f64; 4],
    pub shape: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillUnder<'a> {
    Off,
    Border,
    Inside(&'a Fill),
}

pub fn fill_under<'a>(
    d: &str,
    tips: &[f64],
    fills: &'a [Fill],
    tokens: Option<&[f64]>,
) -> FillUnder<'a> {
    if fills.is_empty() {
        return FillUnder::Off;
    }
    let points = tokens.map_or_else(|| path_points(d), sample);
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points
        .as_chunks::<2>()
        .0
        .iter()
        .chain(tips.as_chunks::<2>().0)
    {
        let [x, y] = *point;
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let mut holder = None;
    let mut least = f64::INFINITY;
    for fill in fills {
        let [l, t, r, b] = fill.bounds;
        if max_x < l + 0.5 || min_x > r - 0.5 || max_y < t + 0.5 || min_y > b - 0.5 {
            continue;
        }
        let inner = [l + 0.5, t + 0.5, r - 0.5, b - 0.5];
        let touched = enters(&points, inner)
            || tips
                .as_chunks::<2>()
                .0
                .iter()
                .any(|&[x, y]| x > l + 0.5 && x < r - 0.5 && y > t + 0.5 && y < b - 0.5);
        if !touched {
            continue;
        }
        if min_x < l || max_x > r || min_y < t || max_y > b {
            return FillUnder::Border;
        }
        let area = (r - l) * (b - t);
        if area < least {
            least = area;
            holder = Some(fill);
        }
    }
    holder.map_or(FillUnder::Off, FillUnder::Inside)
}

pub fn edge_ink(fill: &Fill, line: &str) -> String {
    if RE_STROKE_SET.is_match(line) {
        return String::new();
    }
    ink(&fill.shape, "").unwrap_or("").to_owned()
}

pub fn hollow_on(markers: &str, fill: &Fill, background: &str) -> String {
    match RE_FILL.captures(&fill.shape) {
        Some(color) if !color[1].trim().is_empty() => markers.replace(
            &format!(" fill=\"{}\"", esc(background)),
            &format!(" fill=\"{}\"", color[1].trim()),
        ),
        _ => markers.to_owned(),
    }
}

pub fn over_fills(
    d: &str,
    tips: &[f64],
    fills: &[Fill],
    line: &str,
    tokens: Option<&[f64]>,
) -> Option<String> {
    match fill_under(d, tips, fills, tokens) {
        FillUnder::Off => None,
        FillUnder::Border => Some(String::new()),
        FillUnder::Inside(fill) => Some(edge_ink(fill, line)),
    }
}

fn declaration(line: &str, prop: &str) -> Option<String> {
    if !line.contains(prop) {
        return None;
    }
    let pattern = Regex::new(&format!(r#"[";]{}:([^;"]+);"#, regex::escape(prop))).ok()?;
    pattern
        .captures(line)
        .map(|found| found[1].trim().to_owned())
}

pub fn casing(d: &str, markers: &str, line: &str, thick: bool) -> String {
    let stroke = declaration(line, "stroke");
    if matches!(stroke.as_deref(), Some("none" | "transparent")) {
        return String::new();
    }
    let width = if thick {
        Some("2.5".to_owned())
    } else {
        declaration(line, "stroke-width")
    };
    let mut attrs = String::new();
    let mut style = String::new();
    match width {
        Some(width) if RE_WIDTH.is_match(&width) => {
            let value = parse_float(&width);
            if value != 1.0 {
                attrs = format!(" stroke-width=\"{}\"", num(value + CASE));
            }
        }
        Some(width) => {
            let _ = write!(style, "stroke-width:calc({width} + {}px);", num(CASE));
        }
        None => {}
    }
    for prop in ["opacity", "stroke-opacity"] {
        if let Some(value) = declaration(line, prop) {
            let _ = write!(style, "{prop}:{value};");
        }
    }
    let body = format!(
        "<path d=\"{d}\"/>{}",
        markers.replace(" class=\"pele-marker\"", "")
    );
    if attrs.is_empty() && style.is_empty() {
        body
    } else {
        let style = if style.is_empty() {
            String::new()
        } else {
            format!(" style=\"{style}\"")
        };
        format!("<g{attrs}{style}>{body}</g>")
    }
}

pub fn edge_casing(
    path: &EdgePath,
    start: &str,
    end: &str,
    line: &str,
    thick: bool,
    background: &str,
) -> String {
    let background = esc(background);
    casing(
        &path.d,
        &format!(
            "{}{}",
            marker(
                start,
                path.sx,
                path.sy,
                path.sdx,
                path.sdy,
                &background,
                &background
            ),
            marker(
                end,
                path.ex,
                path.ey,
                path.edx,
                path.edy,
                &background,
                &background
            )
        ),
        line,
        thick,
    )
}

pub fn casings_svg(casings: &str, background: &str) -> String {
    if casings.is_empty() {
        return String::new();
    }
    format!(
        "<g class=\"pele-casings\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" pointer-events=\"none\">{casings}</g>",
        esc(background),
        num(1.0 + CASE)
    )
}

pub fn edge_label_svg(
    id: &str,
    label: &Label,
    x: f64,
    y: f64,
    attrs: &str,
    options: &RenderOptions,
) -> String {
    let w = label.width + 8.0;
    let h = label.height;
    format!(
        "<g class=\"pele-edge-label\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"{}\"/>{}</g>",
        esc_text(id),
        num(x - w / 2.0),
        num(y - h / 2.0),
        num(w),
        num(h),
        esc(&options.theme.background),
        label_with_options(label, x, y, attrs, options)
    )
}

#[allow(clippy::too_many_arguments)]
pub fn loop_path(
    c: &CNode,
    w: f64,
    h: f64,
    k: usize,
    sideways: bool,
    base: f64,
    trim: f64,
    e: &mut CEdge,
) -> EdgePath {
    let k = k as f64;
    let half = if sideways { w } else { h } / 2.0;
    let spread = (half - 4.0).min(8.0 + k * 6.0);
    let reach = base + k * 8.0;
    let len = reach.hypot(spread);
    let len = if len == 0.0 { 1.0 } else { len };
    let at = |out: f64, along: f64| {
        if sideways {
            format!("{},{}", num(c.x + along), num(c.y + h / 2.0 + out))
        } else {
            format!("{},{}", num(c.x + w / 2.0 + out), num(c.y + along))
        }
    };
    let tx = (reach / len) * trim;
    let ty = (spread / len) * trim;
    if sideways {
        e.label_x = c.x;
        e.label_y = c.y + h / 2.0 + reach + 4.0 + e.label_h / 2.0;
    } else {
        e.label_x = c.x + w / 2.0 + reach + 4.0 + e.label_w / 2.0;
        e.label_y = c.y;
    }
    EdgePath {
        d: format!(
            "M{}C{} {} {}",
            at(0.0, -spread),
            at(reach, -spread * 2.0),
            at(reach, spread * 2.0),
            at(tx, spread + ty)
        ),
        tokens: None,
        sx: if sideways {
            c.x - spread
        } else {
            c.x + w / 2.0
        },
        sy: if sideways {
            c.y + h / 2.0
        } else {
            c.y - spread
        },
        sdx: if sideways { 0.0 } else { -1.0 },
        sdy: if sideways { -1.0 } else { 0.0 },
        ex: if sideways {
            c.x + spread
        } else {
            c.x + w / 2.0
        },
        ey: if sideways {
            c.y + h / 2.0
        } else {
            c.y + spread
        },
        edx: if sideways {
            -spread / len
        } else {
            -reach / len
        },
        edy: if sideways {
            -reach / len
        } else {
            -spread / len
        },
    }
}
