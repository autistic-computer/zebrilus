use std::collections::HashMap;

use crate::RenderOptions;
use crate::compound::CNode;
use crate::svg::ink::mix;
use crate::svg::{label_with_options, num};
use crate::text::Label;

pub type Crossings = HashMap<i64, Vec<f64>>;

pub fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    }
}

fn row(y: f64) -> i64 {
    js_round(y) as i64
}

pub fn mark_crossings(crossings: &mut Crossings, route: &[f64]) {
    for point in route.as_chunks::<3>().0 {
        crossings.entry(row(point[1])).or_default().push(point[0]);
    }
}

pub fn cluster_title_x(c: &CNode, width: f64, crossings: &Crossings, pad: f64) -> f64 {
    if width == 0.0 {
        return c.x;
    }
    let top = c.y - c.h / 2.0;
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let candidates = [left + pad - 6.0 + half, c.x, left + c.w - pad + 6.0 - half];
    let Some(xs) = crossings.get(&row(top)) else {
        return candidates[0];
    };
    let mut best = candidates[0];
    let mut best_gap = -1.0;
    for x in candidates {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = gap.min((cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            best_gap = gap;
            best = x;
        }
    }
    best
}

pub fn title_crossed(c: &CNode, width: f64, x: f64, crossings: &Crossings) -> bool {
    if width <= 0.0 {
        return false;
    }
    let Some(xs) = crossings.get(&row(c.y - c.h / 2.0)) else {
        return false;
    };
    let left = c.x - c.w / 2.0;
    xs.iter()
        .any(|&cx| cx > left && cx < left + c.w && (cx - x).abs() < width / 2.0 + 4.0)
}

#[allow(clippy::too_many_arguments)]
pub fn struck_title(
    label: &Label,
    x: f64,
    y: f64,
    attrs: &str,
    id: &str,
    nodes: &[CNode],
    index: usize,
    options: &RenderOptions,
) -> String {
    let mut depth = 1;
    let mut p = nodes[index].parent;
    while let Some(q) = p.filter(|&q| q < nodes.len()) {
        if depth >= 8 {
            break;
        }
        depth += 1;
        p = nodes[q].parent;
    }
    let share = num(100.0 * (1.0 - 0.5_f64.powi(depth)))
        .parse::<f64>()
        .unwrap_or(100.0)
        / 100.0;
    let fill = mix(&options.theme.surface_alt, &options.theme.background, share);
    format!(
        "<g class=\"pele-cluster-title\" data-id=\"{id}\">{}{}</g>",
        label_with_options(
            label,
            x,
            y,
            &format!(
                " fill=\"none\" stroke=\"{fill}\" stroke-width=\"5\" stroke-linejoin=\"round\""
            ),
            options,
        ),
        label_with_options(label, x, y, attrs, options),
    )
}
