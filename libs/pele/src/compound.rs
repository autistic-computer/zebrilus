use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::layout::{
    Groups, Kind, LEdge, LNode, LayeredOptions, LayeredResult, ease, layered, ledge_with, lnode,
    lnode_kind, orient, rank,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dir {
    #[default]
    Tb,
    Bt,
    Lr,
    Rl,
}

impl Dir {
    pub fn vertical(self) -> bool {
        matches!(self, Self::Tb | Self::Bt)
    }

    pub fn sideways(self) -> bool {
        matches!(self, Self::Lr | Self::Rl)
    }

    pub fn flipped(self) -> bool {
        matches!(self, Self::Bt | Self::Rl)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tb => "TB",
            Self::Bt => "BT",
            Self::Lr => "LR",
            Self::Rl => "RL",
        }
    }
}

pub fn direction(dir: &str) -> Dir {
    match dir {
        "BT" => Dir::Bt,
        "LR" => Dir::Lr,
        "RL" => Dir::Rl,
        _ => Dir::Tb,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CNode {
    pub w: f64,
    pub h: f64,
    pub parent: Option<usize>,
    pub is_group: bool,
    pub dir: Option<Dir>,
    pub pad_x: f64,
    pub pad_top: f64,
    pub pad_bottom: f64,
    pub min_w: f64,
    pub seq: usize,
    pub span: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CEdge {
    pub src: usize,
    pub dst: usize,
    pub minlen: usize,
    pub label_w: f64,
    pub label_h: f64,
    pub route: Vec<f64>,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompoundResult {
    pub width: f64,
    pub height: f64,
}

pub fn cnode(w: f64, h: f64, parent: Option<usize>) -> CNode {
    CNode {
        w,
        h,
        parent,
        is_group: false,
        dir: None,
        pad_x: 0.0,
        pad_top: 0.0,
        pad_bottom: 0.0,
        min_w: 0.0,
        seq: 0,
        span: 0.0,
        x: 0.0,
        y: 0.0,
    }
}

pub fn cedge(src: usize, dst: usize) -> CEdge {
    CEdge {
        src,
        dst,
        minlen: 1,
        label_w: 0.0,
        label_h: 0.0,
        route: Vec::new(),
        label_x: 0.0,
        label_y: 0.0,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Curved,
    Stepped,
    Linear,
}

pub fn style_of(opt: &LayeredOptions) -> Style {
    if opt.tracks {
        Style::Stepped
    } else if opt.straight {
        Style::Linear
    } else {
        Style::Curved
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Right => Self::Left,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
        }
    }
}

fn start_side(dir: Dir) -> Side {
    match dir {
        Dir::Tb => Side::Top,
        Dir::Bt => Side::Bottom,
        Dir::Lr => Side::Left,
        Dir::Rl => Side::Right,
    }
}

const MAX_PORTS: usize = 40_000;
const MAX_ORDERED_PORTS: usize = 400;
const MAX_ORDERED_EDGES: usize = 500;
const FLAT_SLACK: f64 = 1.5;
const FLAT_SPARE: f64 = 4.0;
const FAN: f64 = 24.0;
const IN_LINE: f64 = 0.5;
const CURVE_PIECES: usize = 3;
const CORNER_PIECES: usize = 2;
const STEP_PIECES: usize = 3;
const HANDLE: f64 = 0.62;
const MAX_INSERT: usize = 128;
const NONE: usize = usize::MAX;

fn port_key(e: usize, out: bool) -> usize {
    e * 2 + usize::from(!out)
}

pub fn route_crossings(nodes: &[CNode], edges: &[CEdge], style: Style) -> usize {
    let mut size = 0;
    for e in edges {
        if e.src != e.dst && e.route.len() > 3 {
            size += (e.route.len() / 3 - 1) * STEP_PIECES;
        }
    }
    let mut seg = Segments {
        ax: vec![0.0; size],
        ay: vec![0.0; size],
        bx: vec![0.0; size],
        by: vec![0.0; size],
        low: vec![0.0; size],
        high: vec![0.0; size],
        owner: vec![0; size],
        ends: vec![0; size],
        count: 0,
        top: f64::INFINITY,
        bottom: f64::NEG_INFINITY,
        from_x: f64::NAN,
        from_y: f64::NAN,
        to_x: f64::NAN,
        to_y: f64::NAN,
    };
    for (ei, e) in edges.iter().enumerate() {
        if e.src == e.dst {
            continue;
        }
        let r = &e.route;
        let aligned =
            |k: usize| (r[k] - r[k - 3]).abs() < 0.01 || (r[k + 1] - r[k - 2]).abs() < 0.01;
        let length = |k: usize| (r[k] - r[k - 3]).abs() + (r[k + 1] - r[k - 2]).abs();
        let turns = |k: usize| {
            style == Style::Curved
                && k > 0
                && k + 3 < r.len()
                && aligned(k)
                && aligned(k + 3)
                && length(k) >= 0.01
                && length(k + 3) >= 0.01
                && ((r[k] - r[k - 3]).abs() < 0.01) != ((r[k + 3] - r[k]).abs() < 0.01)
        };
        let (mut at_x, mut at_y) = (
            r.first().copied().unwrap_or(f64::NAN),
            r.get(1).copied().unwrap_or(f64::NAN),
        );
        let mut k = 3;
        while k < r.len() {
            let part = u8::from(k == 3) | if k + 3 == r.len() { 2 } else { 0 };
            let (x0, y0) = (at_x, at_y);
            let (x1, y1) = (r[k], r[k + 1]);
            at_x = x1;
            at_y = y1;
            if aligned(k) || style == Style::Linear {
                if !turns(k) {
                    seg.add(ei, part, x0, y0, x1, y1);
                    k += 3;
                    continue;
                }
                let before = if turns(k - 3) {
                    length(k) / 2.0
                } else {
                    length(k)
                };
                let after = if turns(k + 3) {
                    length(k + 3) / 2.0
                } else {
                    length(k + 3)
                };
                let ux = (x1 - r[k - 3]) / length(k);
                let uy = (y1 - r[k - 2]) / length(k);
                let vx = (r[k + 3] - x1) / length(k + 3);
                let vy = (r[k + 4] - y1) / length(k + 3);
                let sx = x1 - ux * before;
                let sy = y1 - uy * before;
                seg.add(ei, part, x0, y0, sx, sy);
                let c1x = x1 - ux * before * (1.0 - HANDLE);
                let c1y = y1 - uy * before * (1.0 - HANDLE);
                let c2x = x1 + vx * after * (1.0 - HANDLE);
                let c2y = y1 + vy * after * (1.0 - HANDLE);
                at_x = x1 + vx * after;
                at_y = y1 + vy * after;
                let turn = part | if k + 6 == r.len() { 2 } else { 0 };
                let (mut px, mut py) = (sx, sy);
                for i in 1..=CORNER_PIECES {
                    let t = i as f64 / CORNER_PIECES as f64;
                    let u = 1.0 - t;
                    let x = u * u * u * sx
                        + 3.0 * u * u * t * c1x
                        + 3.0 * u * t * t * c2x
                        + t * t * t * at_x;
                    let y = u * u * u * sy
                        + 3.0 * u * u * t * c1y
                        + 3.0 * u * t * t * c2y
                        + t * t * t * at_y;
                    seg.add(ei, turn, px, py, x, y);
                    px = x;
                    py = y;
                }
                k += 3;
                continue;
            }
            if style == Style::Stepped {
                let down = r[k + 2] == 0.0;
                let mx = if down { x0 } else { (x0 + x1) / 2.0 };
                let my = if down { (y0 + y1) / 2.0 } else { y0 };
                seg.add(ei, part, x0, y0, mx, my);
                seg.add(
                    ei,
                    part,
                    mx,
                    my,
                    if down { x1 } else { mx },
                    if down { my } else { y1 },
                );
                seg.add(
                    ei,
                    part,
                    if down { x1 } else { mx },
                    if down { my } else { y1 },
                    x1,
                    y1,
                );
                k += 3;
                continue;
            }
            let down = r[k + 2] == 0.0;
            let c = if down {
                ease(x1 - x0, y1 - y0)
            } else {
                ease(y1 - y0, x1 - x0)
            };
            let (mut px, mut py) = (x0, y0);
            for i in 1..=CURVE_PIECES {
                let t = i as f64 / CURVE_PIECES as f64;
                let u = 1.0 - t;
                let across = t * t * (3.0 - 2.0 * t);
                let along = 3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t;
                let x = x0 + (x1 - x0) * if down { across } else { along };
                let y = y0 + (y1 - y0) * if down { along } else { across };
                seg.add(ei, part, px, py, x, y);
                px = x;
                py = y;
            }
            k += 3;
        }
    }
    seg.count_crossings(nodes, edges)
}

struct Segments {
    ax: Vec<f64>,
    ay: Vec<f64>,
    bx: Vec<f64>,
    by: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    owner: Vec<usize>,
    ends: Vec<u8>,
    count: usize,
    top: f64,
    bottom: f64,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
}

impl Segments {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, ei: usize, part: u8, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
        let on = x0 == self.to_x
            && y0 == self.to_y
            && ((y1 == y0 && self.from_y == y0) || (x1 == x0 && self.from_x == x0));
        if on
            && self.count > 0
            && self.owner[self.count - 1] == ei
            && self.ends[self.count - 1] == part
            && (x0 - self.from_x) * (x1 - x0) + (y0 - self.from_y) * (y1 - y0) > 0.0
        {
            self.count -= 1;
            x0 = self.from_x;
            y0 = self.from_y;
        }
        self.from_x = x0;
        self.from_y = y0;
        self.to_x = x1;
        self.to_y = y1;
        let c = self.count;
        self.ends[c] = part;
        self.ax[c] = x0.min(x1);
        self.bx[c] = x0.max(x1);
        self.ay[c] = if x0 <= x1 { y0 } else { y1 };
        self.by[c] = if x0 <= x1 { y1 } else { y0 };
        self.low[c] = y0.min(y1);
        self.high[c] = y0.max(y1);
        if self.low[c] < self.top {
            self.top = self.low[c];
        }
        if self.high[c] > self.bottom {
            self.bottom = self.high[c];
        }
        self.owner[c] = ei;
        self.count += 1;
    }

    fn count_crossings(&self, nodes: &[CNode], edges: &[CEdge]) -> usize {
        let count = self.count;
        let (top, bottom) = (self.top, self.bottom);
        let bands = if bottom > top {
            ((count as f64).sqrt().round() as usize).max(1)
        } else {
            1
        };
        let scale = if bottom > top {
            bands as f64 / (bottom - top)
        } else {
            0.0
        };
        let band_of = |y: f64| -> usize {
            let at = ((y - top) * scale).floor();
            if at.is_nan() || at < 0.0 {
                0
            } else {
                (at as usize).min(bands - 1)
            }
        };
        let first: Vec<usize> = self.low[..count].iter().map(|&y| band_of(y)).collect();
        let mut band_start = vec![0_usize; bands + 1];
        for (p, &from) in first.iter().enumerate() {
            for b in from..=band_of(self.high[p]) {
                band_start[b + 1] += 1;
            }
        }
        for b in 0..bands {
            band_start[b + 1] += band_start[b];
        }
        let mut fill = band_start[..bands].to_vec();
        let mut in_band = vec![0; band_start[bands]];
        for (p, &from) in first.iter().enumerate() {
            for b in from..=band_of(self.high[p]) {
                in_band[fill[b]] = p;
                fill[b] += 1;
            }
        }
        let mut runs = vec![0.0; edges.len() * 8];
        let in_line = |r: &[f64], a: usize, b: usize, c: usize| {
            ((r[a] - r[b]).abs() < IN_LINE && (r[a] - r[c]).abs() < IN_LINE)
                || ((r[a + 1] - r[b + 1]).abs() < IN_LINE && (r[a + 1] - r[c + 1]).abs() < IN_LINE)
        };
        for (ei, e) in edges.iter().enumerate() {
            let r = &e.route;
            if r.len() < 6 {
                continue;
            }
            let mut k = 3;
            while k + 3 < r.len() && in_line(r, 0, k, k + 3) {
                k += 3;
            }
            let last = r.len() - 3;
            let mut j = last as isize - 3;
            while j >= 3 && in_line(r, last, j as usize, j as usize - 3) {
                j -= 3;
            }
            let j = j as usize;
            runs[ei * 8..ei * 8 + 8].copy_from_slice(&[
                r[0],
                r[1],
                r[k],
                r[k + 1],
                r[j],
                r[j + 1],
                r[last],
                r[last + 1],
            ]);
        }
        let on_run = |ei: usize, end: u8, x: f64, y: f64| {
            let o = ei * 8 + if end == 1 { 0 } else { 4 };
            let (x0, y0) = (runs[o], runs[o + 1]);
            let dx = runs[o + 2] - x0;
            let dy = runs[o + 3] - y0;
            let length = dx * dx + dy * dy;
            let t = if length > 0.0 {
                (((x - x0) * dx + (y - y0) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (x - x0 - t * dx).hypot(y - y0 - t * dy) < IN_LINE
        };
        let mut total = 0;
        for band in 0..bands {
            let (start, end) = (band_start[band], band_start[band + 1]);
            let list = &mut in_band[start..end];
            if end - start > MAX_INSERT {
                list.sort_by(|&p, &q| {
                    self.ax[p]
                        .partial_cmp(&self.ax[q])
                        .unwrap_or(Ordering::Equal)
                        .then(p.cmp(&q))
                });
            } else {
                for i in 1..list.len() {
                    let p = list[i];
                    let mut j = i;
                    while j > 0 && self.ax[list[j - 1]] > self.ax[p] {
                        list[j] = list[j - 1];
                        j -= 1;
                    }
                    list[j] = p;
                }
            }
            for s in start..end {
                let p = in_band[s];
                let dx = self.bx[p] - self.ax[p];
                let dy = self.by[p] - self.ay[p];
                let here = first[p] == band;
                for &q in &in_band[s + 1..end] {
                    if self.ax[q] > self.bx[p] {
                        break;
                    }
                    if self.high[q] < self.low[p]
                        || self.low[q] > self.high[p]
                        || self.owner[p] == self.owner[q]
                        || (!here && first[q] != band)
                    {
                        continue;
                    }
                    let d1 = dx * (self.ay[q] - self.ay[p]) - dy * (self.ax[q] - self.ax[p]);
                    let d2 = dx * (self.by[q] - self.ay[p]) - dy * (self.bx[q] - self.ax[p]);
                    if (d1 > 0.0) == (d2 > 0.0) {
                        continue;
                    }
                    let ex = self.bx[q] - self.ax[q];
                    let ey = self.by[q] - self.ay[q];
                    let d3 = ex * (self.ay[p] - self.ay[q]) - ey * (self.ax[p] - self.ax[q]);
                    let d4 = ex * (self.by[p] - self.ay[q]) - ey * (self.bx[p] - self.ax[q]);
                    if (d3 > 0.0) == (d4 > 0.0) {
                        continue;
                    }
                    let ep = &edges[self.owner[p]];
                    let eq = &edges[self.owner[q]];
                    let at = d3 / (d3 - d4);
                    let x = self.ax[p] + at * dx;
                    let y = self.ay[p] + at * dy;
                    let fan = |v: usize, pe: u8, qe: u8| {
                        (self.ends[p] & pe != 0 && self.ends[q] & qe != 0)
                            || ((x - nodes[v].x).abs() < nodes[v].w / 2.0 + FAN
                                && (y - nodes[v].y).abs() < nodes[v].h / 2.0 + FAN)
                            || (on_run(self.owner[p], pe, x, y) && on_run(self.owner[q], qe, x, y))
                    };
                    if ep.src == eq.src && fan(ep.src, 1, 1) {
                        continue;
                    }
                    if ep.src == eq.dst && fan(ep.src, 1, 2) {
                        continue;
                    }
                    if ep.dst == eq.src && fan(ep.dst, 2, 1) {
                        continue;
                    }
                    if ep.dst == eq.dst && fan(ep.dst, 2, 2) {
                        continue;
                    }
                    total += 1;
                }
            }
        }
        total
    }
}

pub fn inherit_crossed_directions(
    nodes: &mut [CNode],
    ends: impl IntoIterator<Item = (usize, usize)>,
) {
    let ancestors = |nodes: &[CNode], i: usize| {
        let mut set = HashSet::new();
        let mut p = nodes[i].parent.filter(|&q| q < nodes.len());
        while let Some(q) = p {
            if !set.insert(q) {
                break;
            }
            p = nodes[q].parent.filter(|&q| q < nodes.len());
        }
        set
    };
    for (src, dst) in ends {
        if src >= nodes.len() || dst >= nodes.len() || src == dst {
            continue;
        }
        let a = ancestors(nodes, src);
        let b = ancestors(nodes, dst);
        for &g in a.difference(&b).chain(b.difference(&a)) {
            nodes[g].dir = None;
        }
    }
}

pub fn shift_layout(nodes: &mut [CNode], edges: &mut [CEdge], ox: f64, oy: f64) {
    for c in nodes.iter_mut() {
        c.x += ox;
        c.y += oy;
    }
    for e in edges.iter_mut() {
        for point in e.route.as_chunks_mut::<3>().0 {
            point[0] += ox;
            point[1] += oy;
        }
        e.label_x += ox;
        e.label_y += oy;
    }
}

#[derive(Clone, Debug)]
struct PortEdge {
    e: usize,
    inner: usize,
    out: bool,
    side: Side,
    hint: f64,
    px: f64,
    py: f64,
    route: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
struct LevelEdge {
    e: usize,
    a: usize,
    b: usize,
}

#[derive(Clone, Debug)]
struct Level {
    id: Option<usize>,
    dir: Dir,
    items: Vec<usize>,
    edges: Vec<LevelEdge>,
    ports: Vec<PortEdge>,
    port_of: HashMap<usize, usize>,
    routes: HashMap<usize, Vec<f64>>,
    x: f64,
    y: f64,
}

impl Level {
    fn new(id: Option<usize>, dir: Dir) -> Self {
        Self {
            id,
            dir,
            items: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
            port_of: HashMap::new(),
            routes: HashMap::new(),
            x: 0.0,
            y: 0.0,
        }
    }

    fn port(&self, e: usize, out: bool) -> &PortEdge {
        &self.ports[self.port_of[&port_key(e, out)]]
    }
}

struct Snapshot {
    nodes: Vec<[f64; 4]>,
    routes: Vec<Vec<f64>>,
    labels: Vec<[f64; 2]>,
    levels: Vec<f64>,
    total: CompoundResult,
}

struct Built {
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    dummies: Vec<Option<usize>>,
}

struct Laid {
    given: Vec<f64>,
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    size: LayeredResult,
}

struct Flat {
    ledges: Vec<LEdge>,
    ledge_of: Vec<Option<usize>>,
    last: Vec<i64>,
}

struct Compound<'a> {
    nodes: &'a mut [CNode],
    edges: &'a mut [CEdge],
    opt: &'a LayeredOptions,
    levels: Vec<Level>,
    level_of: Vec<usize>,
    local: Vec<usize>,
    keyed: bool,
    key: Vec<f64>,
    flat: Option<Flat>,
    ordered: bool,
    laid: HashMap<Option<usize>, Vec<Laid>>,
    total: CompoundResult,
}

fn same(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn kind_number(kind: Kind) -> f64 {
    match kind {
        Kind::Node => 0.0,
        Kind::Dummy => 1.0,
        Kind::Label => 2.0,
        Kind::StartPort => 3.0,
        Kind::EndPort => 4.0,
    }
}

impl Compound<'_> {
    fn level_index(&self, group: Option<usize>) -> usize {
        group.map_or(0, |g| self.level_of[g])
    }

    fn endpoint_below(&self, item: usize, e: usize, out: bool) -> bool {
        let edge = &self.edges[e];
        (if out { edge.src } else { edge.dst }) != item
    }

    fn held(&self, item: usize, e: usize, out: bool) -> bool {
        self.nodes[item].is_group && self.endpoint_below(item, e, out)
    }

    fn offset(&self, vertical: bool, sized: bool, item: usize, e: usize, out: bool) -> f64 {
        if !sized || !self.held(item, e, out) {
            return 0.0;
        }
        let port = self.levels[self.level_of[item]].port(e, out);
        if vertical {
            port.px - self.nodes[item].w / 2.0
        } else {
            port.py - self.nodes[item].h / 2.0
        }
    }

    fn far_hint(&self, li: usize, port: &PortEdge) -> f64 {
        let Some(flat) = &self.flat else {
            return f64::NAN;
        };
        let level = &self.levels[li];
        let Some(le) = flat.ledge_of[port.e] else {
            return f64::NAN;
        };
        let Some(id) = level.id else {
            return f64::NAN;
        };
        if !port.out || port.side != start_side(level.dir).opposite() {
            return f64::NAN;
        }
        let points = &flat.ledges[le].points;
        let mut inside = self.key[self.edges[port.e].src];
        let mut outside = 0.0;
        let last = flat.last[id] as f64;
        for point in points.as_chunks::<2>().0 {
            if point[0] == last {
                inside = point[1];
            }
            if point[0] == last + 1.0 {
                outside = point[1];
            }
        }
        inside + outside * 1e-6
    }

    fn build(&mut self, li: usize, sized: bool) -> Built {
        let dir = self.levels[li].dir;
        let vertical = dir.vertical();
        let start = start_side(dir);
        let items = self.levels[li].items.clone();
        let mut lnodes = Vec::with_capacity(items.len() + self.levels[li].ports.len());
        let mut ledges = Vec::new();
        for (i, &item) in items.iter().enumerate() {
            self.local[item] = i;
            let node = &self.nodes[item];
            let mut ln = if sized {
                lnode(
                    if vertical { node.w } else { node.h },
                    if vertical { node.h } else { node.w },
                )
            } else {
                lnode(0.0, 0.0)
            };
            ln.span = if node.is_group {
                js_min(node.span, ln.w - 2.0 * node.pad_x)
            } else {
                node.span
            };
            ln.key = if self.keyed { self.key[item] } else { f64::NAN };
            lnodes.push(ln);
        }
        let level = &self.levels[li];
        for le in &level.edges {
            let e = &self.edges[le.e];
            let mut edge = ledge_with(self.local[le.a], self.local[le.b], e.minlen, 0.0, 0.0);
            if sized {
                edge.label_w = if vertical { e.label_w } else { e.label_h };
                edge.label_h = if vertical { e.label_h } else { e.label_w };
                edge.tail_dx = self.offset(vertical, sized, le.a, le.e, true);
                edge.head_dx = self.offset(vertical, sized, le.b, le.e, false);
                edge.tail_fixed = self.held(le.a, le.e, true);
                edge.head_fixed = self.held(le.b, le.e, false);
            }
            ledges.push(edge);
        }
        let mut dummies = vec![None; level.ports.len()];
        for (i, port) in level.ports.iter().enumerate() {
            if port.side == start || port.side == start.opposite() {
                let d = lnodes.len();
                let mut ln = lnode_kind(
                    0.0,
                    0.0,
                    if port.side == start {
                        Kind::StartPort
                    } else {
                        Kind::EndPort
                    },
                );
                let edge = &self.edges[port.e];
                ln.key = if self.keyed {
                    self.key[if port.out { edge.dst } else { edge.src }]
                } else {
                    f64::NAN
                };
                ln.hint = if port.hint.is_nan() && self.keyed && self.flat.is_some() {
                    self.far_hint(li, port)
                } else {
                    port.hint
                };
                lnodes.push(ln);
                dummies[i] = Some(d);
                let inner = self.local[port.inner];
                let mut edge = if port.out {
                    ledge_with(inner, d, 1, 0.0, 0.0)
                } else {
                    ledge_with(d, inner, 1, 0.0, 0.0)
                };
                if port.out {
                    edge.tail_dx = self.offset(vertical, sized, port.inner, port.e, true);
                    edge.tail_fixed = self.held(port.inner, port.e, true);
                } else {
                    edge.head_dx = self.offset(vertical, sized, port.inner, port.e, false);
                    edge.head_fixed = self.held(port.inner, port.e, false);
                }
                ledges.push(edge);
            } else {
                lnodes[self.local[port.inner]].pin = if matches!(port.side, Side::Top | Side::Left)
                {
                    -1.0
                } else {
                    1.0
                };
            }
        }
        Built {
            lnodes,
            ledges,
            dummies,
        }
    }

    fn lay_out(
        &mut self,
        li: usize,
        mut lnodes: Vec<LNode>,
        mut ledges: Vec<LEdge>,
    ) -> (Vec<LNode>, Vec<LEdge>, LayeredResult) {
        if !self.ordered {
            let size = layered(&mut lnodes, &mut ledges, self.opt);
            return (lnodes, ledges, size);
        }
        let mut given = Vec::with_capacity(lnodes.len() * 7 + ledges.len() * 9);
        for ln in &lnodes {
            given.extend([
                ln.w,
                ln.h,
                kind_number(ln.kind),
                ln.pin,
                ln.hint,
                ln.key,
                ln.span,
            ]);
        }
        for le in &ledges {
            given.extend([
                le.tail as f64,
                le.head as f64,
                le.minlen as f64,
                le.label_w,
                le.label_h,
                le.tail_dx,
                le.head_dx,
                f64::from(u8::from(le.tail_fixed)),
                f64::from(u8::from(le.head_fixed)),
            ]);
        }
        let id = self.levels[li].id;
        let past = self.laid.entry(id).or_default();
        for p in past.iter() {
            if p.given.len() == given.len() && p.given.iter().zip(&given).all(|(a, b)| same(*a, *b))
            {
                return (p.lnodes.clone(), p.ledges.clone(), p.size.clone());
            }
        }
        let size = layered(&mut lnodes, &mut ledges, self.opt);
        past.push(Laid {
            given,
            lnodes: lnodes.clone(),
            ledges: ledges.clone(),
            size: size.clone(),
        });
        (lnodes, ledges, size)
    }

    fn snapshot(&self) -> Snapshot {
        let mut levels = Vec::new();
        for level in &self.levels {
            levels.extend([level.x, level.y]);
            for port in &level.ports {
                levels.extend([port.px, port.py]);
            }
        }
        Snapshot {
            nodes: self.nodes.iter().map(|c| [c.x, c.y, c.w, c.h]).collect(),
            routes: self.edges.iter().map(|e| e.route.clone()).collect(),
            labels: self.edges.iter().map(|e| [e.label_x, e.label_y]).collect(),
            levels,
            total: self.total,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        for (c, at) in self.nodes.iter_mut().zip(&s.nodes) {
            [c.x, c.y, c.w, c.h] = *at;
        }
        for ((e, route), label) in self.edges.iter_mut().zip(s.routes).zip(&s.labels) {
            e.route = route;
            [e.label_x, e.label_y] = *label;
        }
        let mut k = 0;
        for level in &mut self.levels {
            level.x = s.levels[k];
            level.y = s.levels[k + 1];
            k += 2;
            for port in &mut level.ports {
                port.px = s.levels[k];
                port.py = s.levels[k + 1];
                k += 2;
            }
        }
        self.total = s.total;
    }

    fn place(&mut self) {
        for li in 0..self.levels.len() {
            for pi in 0..self.levels[li].ports.len() {
                let port = &self.levels[li].ports[pi];
                let (inner, e, out, side) = (port.inner, port.e, port.out, port.side);
                if self.nodes[inner].is_group && self.endpoint_below(inner, e, out) {
                    let child = self.level_of[inner];
                    let at = self.levels[child].port_of[&port_key(e, out)];
                    self.levels[child].ports[at].side = side;
                }
            }
            if self.levels[li].edges.is_empty() {
                continue;
            }
            let needed = self.levels[li].edges.iter().any(|le| {
                let e = &self.edges[le.e];
                (self.nodes[le.a].is_group && e.src != le.a)
                    || (self.nodes[le.b].is_group && e.dst != le.b)
            });
            if !needed {
                continue;
            }
            let Built {
                mut lnodes,
                mut ledges,
                ..
            } = self.build(li, false);
            orient(&lnodes, &mut ledges);
            rank(&mut lnodes, &ledges, 1);
            let start = start_side(self.levels[li].dir);
            let level_edges = self.levels[li].edges.clone();
            for (i, le) in level_edges.iter().enumerate() {
                let (src, dst) = (self.edges[le.e].src, self.edges[le.e].dst);
                let a_above = !ledges[i].reversed;
                if self.nodes[le.a].is_group && src != le.a {
                    let child = self.level_of[le.a];
                    let at = self.levels[child].port_of[&port_key(le.e, true)];
                    self.levels[child].ports[at].side =
                        if a_above { start.opposite() } else { start };
                }
                if self.nodes[le.b].is_group && dst != le.b {
                    let child = self.level_of[le.b];
                    let at = self.levels[child].port_of[&port_key(le.e, false)];
                    self.levels[child].ports[at].side =
                        if a_above { start } else { start.opposite() };
                }
            }
        }

        for q in (0..self.levels.len()).rev() {
            let dir = self.levels[q].dir;
            let group = self.levels[q].id;
            let vertical = dir.vertical();
            let flip = dir.flipped();
            let built = self.build(q, true);
            let dummies = built.dummies;
            let (lnodes, ledges, size) = self.lay_out(q, built.lnodes, built.ledges);
            let content_w = if vertical { size.width } else { size.height };
            let content_h = if vertical { size.height } else { size.width };
            let (pad_x, pad_top, pad_bottom, min_w) = group.map_or((0.0, 0.0, 0.0, 0.0), |g| {
                let c = &self.nodes[g];
                (c.pad_x, c.pad_top, c.pad_bottom, c.min_w)
            });
            let box_w = (content_w + 2.0 * pad_x).max(min_w);
            let box_h = content_h + pad_top + pad_bottom;
            let off_x = (box_w - content_w) / 2.0;
            let off_y = pad_top;
            let tx = |lx: f64, ly: f64| {
                off_x
                    + if vertical {
                        lx
                    } else if flip {
                        size.height - ly
                    } else {
                        ly
                    }
            };
            let ty = |lx: f64, ly: f64| {
                off_y
                    + if vertical {
                        if flip { size.height - ly } else { ly }
                    } else {
                        lx
                    }
            };
            let axis = if vertical { 0.0 } else { 1.0 };
            let convert = |points: &[f64]| {
                let mut out = Vec::with_capacity(points.len() / 2 * 3);
                for point in points.as_chunks::<2>().0 {
                    out.extend([tx(point[0], point[1]), ty(point[0], point[1]), axis]);
                }
                out
            };
            let items = self.levels[q].items.clone();
            for (i, &item) in items.iter().enumerate() {
                self.nodes[item].x = tx(lnodes[i].x, lnodes[i].y);
                self.nodes[item].y = ty(lnodes[i].x, lnodes[i].y);
            }
            let level_edges = self.levels[q].edges.clone();
            for (i, le) in level_edges.iter().enumerate() {
                let edge = &ledges[i];
                self.levels[q].routes.insert(le.e, convert(&edge.points));
                let e = &mut self.edges[le.e];
                if e.label_w > 0.0 {
                    e.label_x = tx(edge.label_x, edge.label_y);
                    e.label_y = ty(edge.label_x, edge.label_y);
                }
            }
            let mut k = level_edges.len();
            for (i, dummy) in dummies.iter().enumerate() {
                let port = &self.levels[q].ports[i];
                let (inner, e, out, side) = (port.inner, port.e, port.out, port.side);
                if dummy.is_some() {
                    let mut route = convert(&ledges[k].points);
                    k += 1;
                    let at = if out { route.len() - 3 } else { 0 };
                    let (mut bx, mut by) = (route[at], route[at + 1]);
                    match side {
                        Side::Top => by = 0.0,
                        Side::Bottom => by = box_h,
                        Side::Left => bx = 0.0,
                        Side::Right => bx = box_w,
                    }
                    if out {
                        route.extend([bx, by, axis]);
                    } else {
                        route.splice(0..0, [bx, by, axis]);
                    }
                    let port = &mut self.levels[q].ports[i];
                    port.px = bx;
                    port.py = by;
                    port.route = route;
                    continue;
                }
                let item = &self.nodes[inner];
                let (mut ix, mut iy) = (item.x, item.y);
                if item.is_group && self.endpoint_below(inner, e, out) {
                    let child = self.levels[self.level_of[inner]].port(e, out);
                    ix += child.px - item.w / 2.0;
                    iy += child.py - item.h / 2.0;
                } else {
                    match side {
                        Side::Top => iy -= item.h / 2.0,
                        Side::Bottom => iy += item.h / 2.0,
                        Side::Left => ix -= item.w / 2.0,
                        Side::Right => ix += item.w / 2.0,
                    }
                }
                let (mut bx, mut by) = (ix, iy);
                match side {
                    Side::Top => by = 0.0,
                    Side::Bottom => by = box_h,
                    Side::Left => bx = 0.0,
                    Side::Right => bx = box_w,
                }
                let a = if matches!(side, Side::Top | Side::Bottom) {
                    0.0
                } else {
                    1.0
                };
                let port = &mut self.levels[q].ports[i];
                port.px = bx;
                port.py = by;
                port.route = if out {
                    vec![ix, iy, a, bx, by, a]
                } else {
                    vec![bx, by, a, ix, iy, a]
                };
            }
            if let Some(g) = group {
                self.nodes[g].w = box_w;
                self.nodes[g].h = box_h;
            } else {
                self.total = CompoundResult {
                    width: box_w,
                    height: box_h,
                };
            }
        }

        for li in 0..self.levels.len() {
            let (ox, oy) = (self.levels[li].x, self.levels[li].y);
            for ii in 0..self.levels[li].items.len() {
                let item = self.levels[li].items[ii];
                let node = &mut self.nodes[item];
                node.x += ox;
                node.y += oy;
                if node.is_group {
                    let (x, y) = (node.x - node.w / 2.0, node.y - node.h / 2.0);
                    let child = self.level_of[item];
                    self.levels[child].x = x;
                    self.levels[child].y = y;
                }
            }
        }

        fn append(route: &mut Vec<f64>, part: &[f64], ox: f64, oy: f64) {
            for point in part.as_chunks::<3>().0 {
                let x = point[0] + ox;
                let y = point[1] + oy;
                let len = route.len();
                if len >= 3
                    && (route[len - 3] - x).abs() < 0.01
                    && (route[len - 2] - y).abs() < 0.01
                {
                    continue;
                }
                route.extend([x, y, point[2]]);
            }
        }

        for li in 0..self.levels.len() {
            let level = &self.levels[li];
            for le in &level.edges {
                let e = &self.edges[le.e];
                let mut route = Vec::new();
                let mut outward = Vec::new();
                let mut item = e.src;
                while self.nodes[item].parent != level.id {
                    let parent = self.nodes[item].parent;
                    outward.push(self.level_index(parent));
                    item = parent.unwrap_or(item);
                }
                for &lv in &outward {
                    let lv = &self.levels[lv];
                    append(&mut route, &lv.port(le.e, true).route, lv.x, lv.y);
                }
                append(&mut route, &level.routes[&le.e], level.x, level.y);
                let mut inward = Vec::new();
                let mut item = e.dst;
                while self.nodes[item].parent != level.id {
                    let parent = self.nodes[item].parent;
                    inward.push(self.level_index(parent));
                    item = parent.unwrap_or(item);
                }
                for &lv in inward.iter().rev() {
                    let lv = &self.levels[lv];
                    append(&mut route, &lv.port(le.e, false).route, lv.x, lv.y);
                }
                let (lx, ly) = (level.x, level.y);
                let e = &mut self.edges[le.e];
                e.route = route;
                if e.label_w > 0.0 {
                    e.label_x += lx;
                    e.label_y += ly;
                }
            }
        }
    }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn descend(
    from: usize,
    levels: &mut Vec<Level>,
    level_of: &mut [usize],
    depth: &mut [usize],
    placed: &mut [bool],
    children: &[Vec<usize>],
    nodes: &[CNode],
) {
    let mut q = from;
    while q < levels.len() {
        let id = levels[q].id;
        let dir = levels[q].dir;
        for ii in 0..levels[q].items.len() {
            let item = levels[q].items[ii];
            placed[item] = true;
            depth[item] = id.map_or(0, |g| depth[g] + 1);
            if nodes[item].is_group {
                let mut child = Level::new(Some(item), nodes[item].dir.unwrap_or(dir));
                child.items = children[item].clone();
                level_of[item] = levels.len();
                levels.push(child);
            }
        }
        q += 1;
    }
}

pub fn compound_layout(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    root_dir: Dir,
    opt: &LayeredOptions,
) -> CompoundResult {
    let n = nodes.len();
    let mut levels = vec![Level::new(None, root_dir)];
    let mut level_of = vec![NONE; n];
    let mut children = vec![Vec::new(); n];
    for i in 0..n {
        match nodes[i].parent {
            Some(p) if p < n && p != i && nodes[p].is_group => children[p].push(i),
            _ => {
                nodes[i].parent = None;
                levels[0].items.push(i);
            }
        }
    }
    let mut depth = vec![0_usize; n];
    let mut placed = vec![false; n];
    descend(
        0,
        &mut levels,
        &mut level_of,
        &mut depth,
        &mut placed,
        &children,
        nodes,
    );
    for i in 0..n {
        if placed[i] {
            continue;
        }
        if let Some(former) = nodes[i].parent {
            children[former].retain(|&c| c != i);
        }
        nodes[i].parent = None;
        levels[0].items.push(i);
        placed[i] = true;
        depth[i] = 0;
        if nodes[i].is_group {
            let from = levels.len();
            let mut child = Level::new(Some(i), nodes[i].dir.unwrap_or(root_dir));
            child.items = children[i].clone();
            level_of[i] = levels.len();
            levels.push(child);
            descend(
                from,
                &mut levels,
                &mut level_of,
                &mut depth,
                &mut placed,
                &children,
                nodes,
            );
        }
    }

    for q in (1..levels.len()).rev() {
        if let Some(seq) = levels[q].items.iter().map(|&item| nodes[item].seq).min()
            && let Some(id) = levels[q].id
        {
            nodes[id].seq = seq;
        }
    }
    for level in &mut levels {
        level
            .items
            .sort_by(|&a, &b| nodes[a].seq.cmp(&nodes[b].seq).then(a.cmp(&b)));
    }

    let mut ports = 0;
    for e in edges.iter_mut() {
        let (mut a, mut b) = (e.src, e.dst);
        let mut crossed = 0;
        while depth[a] > depth[b] {
            a = nodes[a].parent.unwrap_or(a);
            crossed += 1;
        }
        while depth[b] > depth[a] {
            b = nodes[b].parent.unwrap_or(b);
            crossed += 1;
        }
        while a != b && nodes[a].parent != nodes[b].parent {
            a = nodes[a].parent.unwrap_or(a);
            b = nodes[b].parent.unwrap_or(b);
            crossed += 2;
        }
        if a == b {
            continue;
        }
        if ports + crossed > MAX_PORTS {
            e.src = a;
            e.dst = b;
        } else {
            ports += crossed;
        }
    }

    let mut loops = Vec::new();
    let mut stray = Vec::new();
    for (ei, e) in edges.iter_mut().enumerate() {
        e.route = Vec::new();
        if e.src == e.dst {
            loops.push(ei);
            continue;
        }
        let (mut a, mut b) = (e.src, e.dst);
        let mut a_path = Vec::new();
        let mut b_path = Vec::new();
        while depth[a] > depth[b] {
            a_path.push(a);
            a = nodes[a].parent.unwrap_or(a);
        }
        while depth[b] > depth[a] {
            b_path.push(b);
            b = nodes[b].parent.unwrap_or(b);
        }
        if a == b {
            stray.push(ei);
            continue;
        }
        while nodes[a].parent != nodes[b].parent {
            a_path.push(a);
            b_path.push(b);
            a = nodes[a].parent.unwrap_or(a);
            b = nodes[b].parent.unwrap_or(b);
        }
        let common = nodes[a].parent.map_or(0, |g| level_of[g]);
        levels[common].edges.push(LevelEdge { e: ei, a, b });
        for (path, out) in [(&a_path, true), (&b_path, false)] {
            for &inner in path {
                let li = nodes[inner].parent.map_or(0, |g| level_of[g]);
                let level = &mut levels[li];
                level.port_of.insert(port_key(ei, out), level.ports.len());
                level.ports.push(PortEdge {
                    e: ei,
                    inner,
                    out,
                    side: Side::Top,
                    hint: f64::NAN,
                    px: 0.0,
                    py: 0.0,
                    route: Vec::new(),
                });
            }
        }
    }

    let ordered = ports > 1 && ports <= MAX_ORDERED_PORTS && edges.len() <= MAX_ORDERED_EDGES;
    let mut key = vec![f64::NAN; n];
    let mut flat_crossings = 0;
    let mut flat = levels.len() > 1 && ordered;
    if levels.iter().any(|level| level.dir != root_dir) {
        flat = false;
    }
    let mut flat_data = None;
    if flat {
        let vertical = root_dir.vertical();
        let mut leaf = vec![None; n];
        let mut group = vec![-1_i32; n];
        let mut lnodes = Vec::new();
        let mut of = Vec::new();
        let mut parent = Vec::new();
        for i in 0..n {
            if nodes[i].is_group {
                group[i] = parent.len() as i32;
                parent.push(-1);
            }
        }
        for i in 0..n {
            let p = nodes[i].parent;
            if nodes[i].is_group {
                parent[group[i] as usize] = p.map_or(-1, |p| group[p]);
            } else {
                leaf[i] = Some(lnodes.len());
                lnodes.push(lnode(
                    if vertical { nodes[i].w } else { nodes[i].h },
                    if vertical { nodes[i].h } else { nodes[i].w },
                ));
                of.push(p.map_or(-1, |p| group[p]));
            }
        }
        let mut ledges = Vec::new();
        let mut ledge_of = vec![None; edges.len()];
        for (ei, e) in edges.iter().enumerate() {
            let (Some(src), Some(dst)) = (leaf[e.src], leaf[e.dst]) else {
                continue;
            };
            if e.src == e.dst {
                continue;
            }
            ledge_of[ei] = Some(ledges.len());
            ledges.push(ledge_with(
                src,
                dst,
                e.minlen,
                if vertical { e.label_w } else { e.label_h },
                if vertical { e.label_h } else { e.label_w },
            ));
        }
        let flat_opt = LayeredOptions {
            groups: Some(Groups { of, parent }),
            order_only: true,
            ..opt.clone()
        };
        let flat_layout = layered(&mut lnodes, &mut ledges, &flat_opt);
        flat_crossings = flat_layout
            .crossings
            .unwrap_or(0)
            .min(flat_layout.declared.unwrap_or(0));
        let mut sum = vec![0.0; n];
        let mut count = vec![0_usize; n];
        let mut last = vec![-1_i64; n];
        for i in 0..n {
            let Some(l) = leaf[i] else {
                continue;
            };
            key[i] = lnodes[l].x;
            let mut p = nodes[i].parent;
            while let Some(g) = p {
                sum[g] += key[i];
                count[g] += 1;
                last[g] = last[g].max(lnodes[l].rank as i64);
                p = nodes[g].parent;
            }
        }
        for i in 0..n {
            if count[i] > 0 {
                key[i] = sum[i] / count[i] as f64;
            }
        }
        flat_data = Some(Flat {
            ledges,
            ledge_of,
            last,
        });
    }

    let mut state = Compound {
        nodes,
        edges,
        opt,
        levels,
        level_of,
        local: vec![0; n],
        keyed: flat,
        key,
        flat: flat_data,
        ordered,
        laid: HashMap::new(),
        total: CompoundResult::default(),
    };
    let style = style_of(opt);
    state.place();
    let mut crossings = None;
    if flat {
        let mut found = route_crossings(state.nodes, state.edges, style);
        if found as f64 > flat_crossings as f64 * FLAT_SLACK + FLAT_SPARE {
            let first = state.snapshot();
            state.keyed = false;
            state.place();
            let own = route_crossings(state.nodes, state.edges, style);
            if own >= found {
                state.keyed = true;
                state.restore(first);
            } else {
                found = own;
            }
        }
        crossings = Some(found);
    }

    if state.keyed == flat && ordered {
        let mut again = false;
        for li in 0..state.levels.len() {
            let Some(id) = state.levels[li].id else {
                continue;
            };
            let (bx0, by0, bw, bh) = {
                let b = &state.nodes[id];
                (b.x, b.y, b.w, b.h)
            };
            let level = &state.levels[li];
            let vertical = level.dir.vertical();
            let end = start_side(level.dir).opposite();
            let (lx, ly) = (level.x, level.y);
            let mut seen: Vec<(f64, f64)> = Vec::new();
            let mut hints = Vec::new();
            for (pi, port) in level.ports.iter().enumerate() {
                if port.side != end {
                    continue;
                }
                let route = &state.edges[port.e].route;
                let bx = lx + port.px;
                let by = ly + port.py;
                let mut at = None;
                let mut k = 0;
                while k < route.len() && (at.is_none() || port.out) {
                    if (route[k] - bx).abs() < 0.01 && (route[k + 1] - by).abs() < 0.01 {
                        at = Some(k);
                    }
                    k += 3;
                }
                let Some(at) = at else {
                    continue;
                };
                let a = if vertical { 0 } else { 1 };
                let step = |k: isize| if port.out { k + 3 } else { k - 3 };
                let mut next = step(at as isize);
                while next >= 0
                    && (next as usize) < route.len()
                    && (route[next as usize + a] - route[at + a]).abs() < IN_LINE
                {
                    next = step(next);
                }
                if next < 0 || next as usize >= route.len() {
                    continue;
                }
                let hint = route[next as usize + a]
                    - if vertical {
                        bx0 - bw / 2.0
                    } else {
                        by0 - bh / 2.0
                    };
                hints.push((pi, hint));
                seen.push((if vertical { port.px } else { port.py }, hint));
            }
            for (pi, hint) in hints {
                state.levels[li].ports[pi].hint = hint;
            }
            seen.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(Ordering::Equal));
            if seen.windows(2).any(|pair| pair[0].1 > pair[1].1) {
                again = true;
            }
        }
        if again {
            let before =
                crossings.unwrap_or_else(|| route_crossings(state.nodes, state.edges, style));
            if before > 0 {
                let first = state.snapshot();
                state.place();
                if route_crossings(state.nodes, state.edges, style) >= before {
                    state.restore(first);
                }
            }
        }
    }

    let total = state.total;
    for ei in stray {
        let (src, dst) = (state.edges[ei].src, state.edges[ei].dst);
        let (ax, ay) = (state.nodes[src].x, state.nodes[src].y);
        let (bx, by) = (state.nodes[dst].x, state.nodes[dst].y);
        let e = &mut state.edges[ei];
        e.route = vec![ax, ay, 0.0, bx, by, 0.0];
        e.label_x = (ax + bx) / 2.0;
        e.label_y = (ay + by) / 2.0;
    }
    for ei in loops {
        let node = &state.nodes[state.edges[ei].src];
        let (x, y, w) = (node.x, node.y, node.w);
        let e = &mut state.edges[ei];
        e.label_x = x + w / 2.0 + 28.0 + e.label_w / 2.0;
        e.label_y = y;
    }
    total
}
