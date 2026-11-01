use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Node,
    Dummy,
    Label,
    StartPort,
    EndPort,
}

#[derive(Clone, Debug)]
pub struct LNode {
    pub w: f64,
    pub h: f64,
    pub kind: Kind,
    pub pin: f64,
    pub hint: f64,
    pub key: f64,
    pub span: f64,
    pub x: f64,
    pub y: f64,
    pub rank: usize,
}

#[derive(Clone, Debug)]
pub struct LEdge {
    pub tail: usize,
    pub head: usize,
    pub minlen: usize,
    pub label_w: f64,
    pub label_h: f64,
    pub tail_dx: f64,
    pub head_dx: f64,
    pub tail_fixed: bool,
    pub head_fixed: bool,
    pub reversed: bool,
    pub points: Vec<f64>,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Groups {
    pub of: Vec<i32>,
    pub parent: Vec<i32>,
}

#[derive(Clone, Debug)]
pub struct LayeredOptions {
    pub node_sep: f64,
    pub edge_sep: f64,
    pub rank_sep: f64,
    pub port_sep: f64,
    pub tracks: bool,
    pub straight: bool,
    pub apart: bool,
    pub head_room: f64,
    pub groups: Option<Groups>,
    pub order_only: bool,
}

impl Default for LayeredOptions {
    fn default() -> Self {
        Self {
            node_sep: 40.0,
            edge_sep: 20.0,
            rank_sep: 50.0,
            port_sep: 20.0,
            tracks: false,
            straight: false,
            apart: false,
            head_room: 0.0,
            groups: None,
            order_only: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayeredResult {
    pub width: f64,
    pub height: f64,
    pub crossings: Option<usize>,
    pub declared: Option<usize>,
}

pub fn lnode(w: f64, h: f64) -> LNode {
    lnode_kind(w, h, Kind::Node)
}

pub fn lnode_kind(w: f64, h: f64, kind: Kind) -> LNode {
    LNode {
        w,
        h,
        kind,
        pin: 0.0,
        hint: f64::NAN,
        key: f64::NAN,
        span: 0.0,
        x: 0.0,
        y: 0.0,
        rank: 0,
    }
}

pub fn ledge(tail: usize, head: usize) -> LEdge {
    ledge_with(tail, head, 1, 0.0, 0.0)
}

pub fn ledge_with(tail: usize, head: usize, minlen: usize, label_w: f64, label_h: f64) -> LEdge {
    LEdge {
        tail,
        head,
        minlen,
        label_w,
        label_h,
        tail_dx: 0.0,
        head_dx: 0.0,
        tail_fixed: false,
        head_fixed: false,
        reversed: false,
        points: Vec::new(),
        label_x: 0.0,
        label_y: 0.0,
    }
}

pub fn ease(across: f64, along: f64) -> f64 {
    (4.0 * along.abs() / if across == 0.0 { 1.0 } else { across.abs() }).clamp(0.25, 0.5)
}

pub fn orient(nodes: &[LNode], edges: &mut [LEdge]) {
    let mut out = vec![Vec::new(); nodes.len()];
    for (i, e) in edges.iter_mut().enumerate() {
        e.reversed = nodes[e.head].kind == Kind::StartPort || nodes[e.tail].kind == Kind::EndPort;
        if !e.reversed {
            out[e.tail].push(i);
        }
    }
    let mut state = vec![0_u8; nodes.len()];
    let mut cursor = vec![0; nodes.len()];
    let mut stack = Vec::new();
    for root in 0..nodes.len() {
        if state[root] != 0 {
            continue;
        }
        stack.push(root);
        state[root] = 1;
        while let Some(&v) = stack.last() {
            if cursor[v] < out[v].len() {
                let ei = out[v][cursor[v]];
                cursor[v] += 1;
                let w = edges[ei].head;
                if state[w] == 1 {
                    edges[ei].reversed = true;
                } else if state[w] == 0 {
                    state[w] = 1;
                    stack.push(w);
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

pub fn rank(nodes: &mut [LNode], edges: &[LEdge], step: usize) {
    let n = nodes.len();
    if n == 0 {
        return;
    }
    let mut outs = vec![Vec::new(); n];
    let mut ins = vec![Vec::new(); n];
    let mut from = vec![0; edges.len()];
    let mut to = vec![0; edges.len()];
    let mut len = vec![0_i64; edges.len()];
    for (i, e) in edges.iter().enumerate() {
        let a = if e.reversed { e.head } else { e.tail };
        let b = if e.reversed { e.tail } else { e.head };
        from[i] = a;
        to[i] = b;
        len[i] = e.minlen.max(1).saturating_mul(step) as i64;
        if a != b {
            outs[a].push(i);
            ins[b].push(i);
        }
    }
    let mut r = vec![0_i64; n];
    let mut indeg: Vec<_> = ins.iter().map(Vec::len).collect();
    let mut topo: Vec<_> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut q = 0;
    while q < topo.len() {
        let v = topo[q];
        for &ei in &outs[v] {
            let w = to[ei];
            r[w] = r[w].max(r[v].saturating_add(len[ei]));
            indeg[w] -= 1;
            if indeg[w] == 0 {
                topo.push(w);
            }
        }
        q += 1;
    }
    for _ in 0..8 {
        let mut changed = false;
        for &v in topo.iter().rev() {
            let incoming = ins[v].len();
            let outgoing = outs[v].len();
            if incoming == 0 && outgoing == 0 {
                continue;
            }
            let lo = ins[v].iter().map(|&e| r[from[e]] + len[e]).max();
            let hi = outs[v].iter().map(|&e| r[to[e]] - len[e]).min();
            let next = if outgoing > incoming {
                hi
            } else if incoming > outgoing {
                lo
            } else {
                None
            };
            if let Some(next) = next
                && next != r[v]
            {
                r[v] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let non_ports: Vec<_> = (0..n)
        .filter(|&i| !matches!(nodes[i].kind, Kind::StartPort | Kind::EndPort))
        .collect();
    let mut min = non_ports.iter().map(|&i| r[i]).min().unwrap_or(0);
    let max = non_ports.iter().map(|&i| r[i]).max().unwrap_or(0);
    let start_ports = nodes.iter().any(|node| node.kind == Kind::StartPort);
    for i in 0..n {
        if nodes[i].kind == Kind::StartPort {
            r[i] = min - step as i64;
        } else if nodes[i].kind == Kind::EndPort {
            r[i] = max + step as i64;
        }
    }
    if start_ports {
        min -= step as i64;
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        node.rank = r[i].saturating_sub(min).max(0) as usize;
    }
}

#[derive(Clone, Debug)]
struct Segment {
    from: usize,
    to: usize,
    from_dx: f64,
    to_dx: f64,
    from_kind: u8,
    to_kind: u8,
}

struct Graph {
    real: usize,
    nodes: Vec<LNode>,
    segments: Vec<Segment>,
    succ: Vec<Vec<usize>>,
    pred: Vec<Vec<usize>>,
    layers: Vec<Vec<usize>>,
    pos: Vec<usize>,
    fixed: Vec<bool>,
    groups: Option<Groups>,
}

impl Graph {
    fn set_positions(&mut self) {
        for row in &self.layers {
            for (i, &v) in row.iter().enumerate() {
                self.pos[v] = i;
            }
        }
    }

    fn count_crossings(&self) -> usize {
        let mut total = 0;
        let mut places = vec![0; self.segments.len()];
        for r in 0..self.layers.len().saturating_sub(1) {
            let mut next = 0;
            for &v in &self.layers[r + 1] {
                let mut incoming = self.pred[v].clone();
                incoming.sort_by(|&a, &b| cmp(self.segments[a].to_dx, self.segments[b].to_dx));
                let mut prev = None;
                for e in incoming {
                    let dx = self.segments[e].to_dx;
                    if prev.is_some_and(|p| p != dx) {
                        next += 1;
                    }
                    places[e] = next;
                    prev = Some(dx);
                }
                next += 1;
            }
            if next < 2 {
                continue;
            }
            let mut targets = Vec::new();
            for &v in &self.layers[r] {
                let mut leaving = self.succ[v].clone();
                leaving.sort_by(|&a, &b| {
                    cmp(self.segments[a].from_dx, self.segments[b].from_dx)
                        .then(places[a].cmp(&places[b]))
                });
                targets.extend(leaving.iter().map(|&e| places[e]));
            }
            let mut tree = vec![0_usize; next + 1];
            for (seen, target) in targets.into_iter().enumerate() {
                let mut at = target + 1;
                let mut before = 0;
                while at > 0 {
                    before += tree[at];
                    at &= at - 1;
                }
                total += seen - before;
                at = target + 1;
                while at < tree.len() {
                    tree[at] += 1;
                    at += at.isolate_lowest_one();
                }
            }
        }
        total
    }

    fn arrange_groups(&mut self, r: usize, key: &[f64]) {
        let Some(groups) = &self.groups else { return };
        let n = self.nodes.len();
        let count = groups.parent.len();
        let mut depths = vec![0; count];
        let mut sums = vec![0.0; count];
        let mut counts = vec![0; count];
        let mut firsts = vec![0; count];
        for (gr, depth) in depths.iter_mut().enumerate() {
            let mut current = gr as i32;
            while current >= 0 {
                *depth += 1;
                current = groups.parent[current as usize];
            }
        }
        for (i, &v) in self.layers[r].iter().enumerate() {
            let mut gr = groups.of[v];
            while gr >= 0 {
                let g = gr as usize;
                if counts[g] == 0 {
                    firsts[g] = i;
                }
                sums[g] += key[v];
                counts[g] += 1;
                gr = groups.parent[g];
            }
        }
        self.layers[r].sort_by(|&a, &b| {
            let (mut ia, mut ib) = (a, b);
            let (mut ca, mut cb) = (groups.of[a], groups.of[b]);
            let (mut da, mut db) = (
                if ca < 0 { 0 } else { depths[ca as usize] },
                if cb < 0 { 0 } else { depths[cb as usize] },
            );
            while da > db {
                ia = n + ca as usize;
                ca = groups.parent[ca as usize];
                da -= 1;
            }
            while db > da {
                ib = n + cb as usize;
                cb = groups.parent[cb as usize];
                db -= 1;
            }
            while ca != cb {
                ia = n + ca as usize;
                ca = groups.parent[ca as usize];
                ib = n + cb as usize;
                cb = groups.parent[cb as usize];
            }
            let value = |item: usize| {
                if item >= n {
                    sums[item - n] / counts[item - n] as f64
                } else {
                    key[item]
                }
            };
            let first = |item: usize| {
                if item >= n {
                    firsts[item - n]
                } else {
                    self.pos[item]
                }
            };
            cmp(value(ia), value(ib)).then(first(ia).cmp(&first(ib)))
        });
        self.set_positions();
    }

    fn out_of_order(&self) -> usize {
        let mut total = 0;
        for row in &self.layers {
            for (i, &v) in row.iter().enumerate() {
                if v >= self.real {
                    continue;
                }
                for &w in &row[i + 1..] {
                    if w < v {
                        total += 1;
                    }
                }
            }
        }
        total
    }

    fn sweep(&mut self, down: bool, bias_right: bool) {
        let n = self.nodes.len();
        for step in 1..self.layers.len() {
            let r = if down {
                step
            } else {
                self.layers.len() - 1 - step
            };
            if self.fixed[r] {
                continue;
            }
            let mut bary = vec![-1.0; n];
            let mut moving = Vec::new();
            for (i, &v) in self.layers[r].iter().enumerate() {
                let pin = self.nodes[v].pin;
                if pin != 0.0 {
                    bary[v] = pin * 1e9;
                    moving.push(v);
                    continue;
                }
                let adjacent = if down { &self.pred[v] } else { &self.succ[v] };
                if adjacent.is_empty() {
                    if self.groups.is_some() {
                        let across = if down {
                            self.layers[r - 1].len()
                        } else {
                            self.layers[r + 1].len()
                        };
                        bary[v] = i as f64 * across as f64 / self.layers[r].len() as f64;
                    }
                    continue;
                }
                let mut sum = 0.0;
                for &ei in adjacent {
                    let e = &self.segments[ei];
                    let u = if down { e.from } else { e.to };
                    let dx = if down { e.from_dx } else { e.to_dx };
                    sum += self.pos[u] as f64;
                    if self.nodes[u].w > 0.0 {
                        sum += dx / self.nodes[u].w;
                    }
                }
                bary[v] = sum / adjacent.len() as f64;
                moving.push(v);
            }
            if self.groups.is_some() {
                self.arrange_groups(r, &bary);
                continue;
            }
            if moving.len() < 2 {
                continue;
            }
            moving.sort_by(|&a, &b| {
                cmp(bary[a], bary[b]).then(if bias_right {
                    self.pos[b].cmp(&self.pos[a])
                } else {
                    self.pos[a].cmp(&self.pos[b])
                })
            });
            let mut k = 0;
            for v in &mut self.layers[r] {
                if bary[*v] != -1.0 {
                    *v = moving[k];
                    k += 1;
                }
            }
            self.set_positions();
        }
    }

    fn between(&self, a: usize, b: usize) -> usize {
        let mut count = 0;
        for up in [true, false] {
            let left = if up { &self.pred[a] } else { &self.succ[a] };
            let right = if up { &self.pred[b] } else { &self.succ[b] };
            let at = |ei: usize| {
                let e = &self.segments[ei];
                let u = if up { e.from } else { e.to };
                let dx = if up { e.from_dx } else { e.to_dx };
                self.pos[u] as f64
                    + if self.nodes[u].w > 0.0 {
                        dx / self.nodes[u].w
                    } else {
                        0.0
                    }
            };
            for &x in left {
                for &y in right {
                    if at(x) > at(y) {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn transpose(&mut self) {
        for _ in 0..8 {
            let mut improved = false;
            for r in 0..self.layers.len() {
                if self.fixed[r] {
                    continue;
                }
                for i in 0..self.layers[r].len().saturating_sub(1) {
                    let (a, b) = (self.layers[r][i], self.layers[r][i + 1]);
                    if self.nodes[a].pin != self.nodes[b].pin {
                        continue;
                    }
                    if self
                        .groups
                        .as_ref()
                        .is_some_and(|groups| groups.of[a] != groups.of[b])
                    {
                        continue;
                    }
                    if self.between(b, a) >= self.between(a, b) {
                        continue;
                    }
                    self.layers[r].swap(i, i + 1);
                    self.pos[b] = i;
                    self.pos[a] = i + 1;
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }
    }

    fn reduce_crossings(&mut self) -> (usize, usize) {
        let declared = self.count_crossings();
        if self.groups.is_some() {
            let at: Vec<_> = self.pos.iter().map(|&p| p as f64).collect();
            for r in 0..self.layers.len() {
                self.arrange_groups(r, &at);
            }
        }
        let mut best = if self.groups.is_some() {
            self.count_crossings()
        } else {
            declared
        };
        if best == 0 {
            return (0, declared);
        }
        let initial = self.layers.clone();
        let start = best.min(declared);
        let small = self.nodes.len() <= 600;
        let mut chosen = initial.clone();
        let mut chosen_out = usize::MAX;
        for attempt in 0..if small { 2 } else { 1 } {
            self.layers.clone_from(&initial);
            self.set_positions();
            let mut found = start;
            let mut found_order = initial.clone();
            let mut stale = 0;
            let mut i = 0;
            while stale < 4 && i < 24 {
                self.sweep((i + attempt) % 2 == 0, i % 4 >= 2);
                let c = self.count_crossings();
                if c < found {
                    found = c;
                    found_order.clone_from(&self.layers);
                    stale = 0;
                    if c == 0 {
                        break;
                    }
                }
                i += 1;
                stale += 1;
            }
            self.layers.clone_from(&found_order);
            self.set_positions();
            if small && found > 0 {
                self.transpose();
                let c = self.count_crossings();
                if c < found {
                    found = c;
                    found_order.clone_from(&self.layers);
                }
            }
            let out = if small { self.out_of_order() } else { 0 };
            if attempt == 0 || found < best || (found == best && out < chosen_out) {
                best = found;
                chosen = found_order;
                chosen_out = out;
            }
        }
        self.layers = chosen.clone();
        self.set_positions();
        if self.groups.is_some() {
            best = self.count_crossings();
        }
        let max_rank = self.layers.len() - 1;
        if small
            && !self.nodes[..self.real].iter().any(|node| node.pin != 0.0)
            && chosen_out > 0
            && !self.fixed[0]
            && !self.fixed[max_rank]
        {
            for row in &mut self.layers {
                row.reverse();
            }
            self.set_positions();
            if self.count_crossings() > best || self.out_of_order() >= chosen_out {
                self.layers = chosen;
                self.set_positions();
            }
        }
        (best, declared)
    }
}

fn cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn is_dummy(kind: Kind) -> bool {
    matches!(kind, Kind::Dummy | Kind::Label)
}

fn position(g: &Graph, opt: &LayeredOptions) -> Vec<f64> {
    let n = g.nodes.len();
    let max_rank = g.layers.len() - 1;
    let gap: Vec<_> = g
        .nodes
        .iter()
        .map(|node| {
            node.w / 2.0
                + if node.kind == Kind::Node {
                    opt.node_sep
                } else {
                    opt.edge_sep
                } / 2.0
        })
        .collect();
    let mut succ = g.succ.clone();
    let mut pred = g.pred.clone();
    for v in 0..n {
        succ[v].sort_by_key(|&e| (g.pos[g.segments[e].to], e));
        pred[v].sort_by_key(|&e| (g.pos[g.segments[e].from], e));
    }
    let mut conflict = vec![false; g.segments.len()];
    for r in 1..=max_rank {
        let prev_len = g.layers[r - 1].len();
        let row = &g.layers[r];
        let mut k0 = 0;
        let mut scan = 0;
        for (i, &v) in row.iter().enumerate() {
            let inner = if is_dummy(g.nodes[v].kind) {
                pred[v]
                    .first()
                    .map(|&e| g.segments[e].from)
                    .filter(|&u| is_dummy(g.nodes[u].kind))
            } else {
                None
            };
            let k1 = inner.map_or(prev_len, |u| g.pos[u]);
            if inner.is_some() || i == row.len() - 1 {
                for &w in &row[scan..=i] {
                    let dummy = is_dummy(g.nodes[w].kind);
                    for &e in &pred[w] {
                        let up = g.pos[g.segments[e].from];
                        if (up < k0 || k1 < up)
                            && !(dummy && is_dummy(g.nodes[g.segments[e].from].kind))
                        {
                            conflict[e] = true;
                        }
                    }
                }
                scan = i + 1;
                k0 = k1;
            }
        }
    }
    let mut results = Vec::new();
    for variant in 0..4 {
        let up = variant < 2;
        let right = variant & 1 == 1;
        let adjacent_edges = if up { &pred } else { &succ };
        let mut root: Vec<_> = (0..n).collect();
        let mut align = root.clone();
        let mut shift = vec![0.0; n];
        for step in 0..=max_rank {
            let r = if up { step } else { max_rank - step };
            let adjacent = if up {
                r.checked_sub(1)
            } else if r < max_rank {
                Some(r + 1)
            } else {
                None
            };
            let adjacent_last = adjacent.map_or(0, |a| g.layers[a].len().saturating_sub(1));
            let mut prev_idx: i64 = -1;
            let mut row = g.layers[r].clone();
            if right {
                row.reverse();
            }
            for v in row {
                let neighbors = &adjacent_edges[v];
                let d = neighbors.len();
                if d == 0 {
                    continue;
                }
                let lo = (d - 1) / 2;
                let hi = d / 2;
                for pass in 0..2 {
                    if pass == 1 && lo == hi {
                        break;
                    }
                    let ei = neighbors[if (pass == 0) != right { lo } else { hi }];
                    let e = &g.segments[ei];
                    let w = if up { e.from } else { e.to };
                    let own_dx = if up { e.to_dx } else { e.from_dx };
                    let other_dx = if up { e.from_dx } else { e.to_dx };
                    let idx = if right {
                        adjacent_last as i64 - g.pos[w] as i64
                    } else {
                        g.pos[w] as i64
                    };
                    if align[v] == v && prev_idx < idx && !conflict[ei] {
                        align[w] = v;
                        align[v] = root[w];
                        root[v] = root[w];
                        shift[v] = shift[w] + other_dx - own_dx;
                        prev_idx = idx;
                    }
                }
            }
        }
        let mut block_edges = Vec::new();
        let mut out = vec![Vec::new(); n];
        let mut indeg = vec![0; n];
        for row in &g.layers {
            for pair in row.windows(2) {
                let (a, b) = if right {
                    (pair[1], pair[0])
                } else {
                    (pair[0], pair[1])
                };
                let sep = gap[a] + gap[b];
                let weight = if right {
                    sep - shift[a] + shift[b]
                } else {
                    sep + shift[a] - shift[b]
                };
                let ei = block_edges.len();
                block_edges.push((root[a], root[b], weight));
                out[root[a]].push(ei);
                indeg[root[b]] += 1;
            }
        }
        let mut topo: Vec<_> = (0..n).filter(|&i| root[i] == i && indeg[i] == 0).collect();
        let mut x = vec![0.0_f64; n];
        let mut q = 0;
        while q < topo.len() {
            let v = topo[q];
            for &e in &out[v] {
                let (_, w, weight) = block_edges[e];
                x[w] = x[w].max(x[v] + weight);
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    topo.push(w);
                }
            }
            q += 1;
        }
        for &v in topo.iter().rev() {
            let minimum = out[v]
                .iter()
                .map(|&e| {
                    let (_, w, weight) = block_edges[e];
                    x[w] - weight
                })
                .fold(f64::INFINITY, f64::min);
            if minimum.is_finite() && minimum > x[v] {
                x[v] = minimum;
            }
        }
        results.push(
            (0..n)
                .map(|i| {
                    if right {
                        -x[root[i]] + shift[i]
                    } else {
                        x[root[i]] + shift[i]
                    }
                })
                .collect::<Vec<_>>(),
        );
    }
    let mut narrowest = 0;
    let mut best_width = f64::INFINITY;
    let mut mins = Vec::new();
    let mut maxs = Vec::new();
    for (k, xs) in results.iter().enumerate() {
        let lo = (0..n)
            .map(|i| xs[i] - g.nodes[i].w / 2.0)
            .fold(f64::INFINITY, f64::min);
        let hi = (0..n)
            .map(|i| xs[i] + g.nodes[i].w / 2.0)
            .fold(f64::NEG_INFINITY, f64::max);
        mins.push(lo);
        maxs.push(hi);
        if hi - lo < best_width {
            best_width = hi - lo;
            narrowest = k;
        }
    }
    for k in 0..4 {
        let delta = if k & 1 == 0 {
            mins[narrowest] - mins[k]
        } else {
            maxs[narrowest] - maxs[k]
        };
        for x in &mut results[k] {
            *x += delta;
        }
    }
    (0..n)
        .map(|i| {
            let mut xs = [results[0][i], results[1][i], results[2][i], results[3][i]];
            xs.sort_by(|a, b| cmp(*a, *b));
            (xs[1] + xs[2]) / 2.0
        })
        .collect()
}

fn side_ends(g: &Graph, v: usize, side: usize, span: f64) -> (Vec<usize>, Vec<f64>, f64) {
    let list = if side == 0 { &g.pred[v] } else { &g.succ[v] };
    let kind = |e: usize| {
        if side == 0 {
            g.segments[e].to_kind
        } else {
            g.segments[e].from_kind
        }
    };
    let other = |e: usize| {
        if side == 0 {
            g.segments[e].from
        } else {
            g.segments[e].to
        }
    };
    let mut ids: Vec<_> = list.iter().copied().filter(|&e| kind(e) != 0).collect();
    ids.sort_by_key(|&e| (g.pos[other(e)], e));
    let mut room = vec![1.0; ids.len()];
    if ids.len() < 2 {
        return (ids, room, 0.0);
    }
    room[0] = 0.0;
    let mut total = (ids.len() - 1) as f64;
    if span / total < 8.0 {
        for k in 1..ids.len() {
            room[k] = if kind(ids[k]) == kind(ids[k - 1]) && kind(ids[k]) == 1 {
                0.5
            } else {
                1.0
            };
        }
        total = room.iter().sum();
    }
    if span / total < 8.0 {
        for k in 1..ids.len() {
            room[k] = if kind(ids[k]) == kind(ids[k - 1]) {
                0.0
            } else {
                1.0
            };
        }
        total = room.iter().sum();
    }
    if total == 0.0 {
        return (ids, room, 0.0);
    }
    let pitch = (span / total).min(16.0);
    for value in &mut room[1..] {
        *value *= pitch;
    }
    (ids, room, total * pitch)
}

fn spread(g: &mut Graph) {
    for v in 0..g.real {
        let span = g.nodes[v].span.min(g.nodes[v].w);
        if span <= 0.0 || g.nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let (ids, room, total) = side_ends(g, v, side, span);
            if total == 0.0 {
                continue;
            }
            let mut at = -total / 2.0;
            for (k, &e) in ids.iter().enumerate() {
                at += room[k];
                if side == 0 {
                    g.segments[e].to_dx = at;
                } else {
                    g.segments[e].from_dx = at;
                }
            }
        }
    }
}

fn empty_ranks(g: &Graph) -> Vec<bool> {
    g.layers
        .iter()
        .enumerate()
        .map(|(r, row)| {
            r > 0
                && r + 1 < g.layers.len()
                && !row.is_empty()
                && row.iter().all(|&v| g.nodes[v].kind == Kind::Dummy)
        })
        .collect()
}

fn follow(g: &Graph, empty: &[bool], mut e: usize, side: usize) -> usize {
    for _ in 0..g.nodes.len() {
        let u = if side == 0 {
            g.segments[e].from
        } else {
            g.segments[e].to
        };
        if g.nodes[u].kind != Kind::Dummy || !empty[g.nodes[u].rank] {
            break;
        }
        let list = if side == 0 { &g.pred[u] } else { &g.succ[u] };
        let Some(&next) = list.first() else { break };
        e = next;
    }
    e
}

fn align_ports(g: &Graph, xs: &mut [f64], empty: &[bool]) {
    for (r, kind) in [(0, Kind::StartPort), (g.layers.len() - 1, Kind::EndPort)] {
        let mut to = Vec::new();
        for &v in &g.layers[r] {
            let start = kind == Kind::StartPort;
            let list = if start { &g.succ[v] } else { &g.pred[v] };
            if g.nodes[v].kind != kind || list.len() != 1 {
                break;
            }
            let ei = follow(g, empty, list[0], if start { 1 } else { 0 });
            let e = &g.segments[ei];
            let at = if start {
                xs[e.to] + e.to_dx
            } else {
                xs[e.from] + e.from_dx
            };
            if to.last().is_some_and(|&prev| at < prev + 1.0) {
                break;
            }
            to.push(at);
        }
        if !to.is_empty() && to.len() == g.layers[r].len() {
            for (i, &v) in g.layers[r].iter().enumerate() {
                xs[v] = to[i];
            }
        }
    }
}

fn straighten(
    g: &Graph,
    xs: &mut [f64],
    empty: &[bool],
    chains: &[Vec<usize>],
    sep: f64,
    on_line: &mut [bool],
) {
    let mut want = vec![0.0; xs.len()];
    let mut from = vec![0.0; xs.len()];
    let mut to = vec![0.0; xs.len()];
    for chain in chains {
        let mut i = 0;
        while i < chain.len() {
            let d = chain[i];
            if !empty[g.nodes[d].rank] {
                i += 1;
                continue;
            }
            let mut last = i;
            while last + 1 < chain.len() && empty[g.nodes[chain[last + 1]].rank] {
                last += 1;
            }
            let above = &g.segments[g.pred[d][0]];
            let below = &g.segments[g.succ[chain[last]][0]];
            let x0 = xs[above.from] + above.from_dx;
            let x1 = xs[below.to] + below.to_dx;
            let steps = last - i + 2;
            for (k, &bend) in chain.iter().enumerate().take(last + 1).skip(i) {
                want[bend] = x0 + (x1 - x0) * (k - i + 1) as f64 / steps as f64;
                from[bend] = x0;
                to[bend] = x1;
            }
            i = last + 1;
        }
    }
    for (r, row) in g.layers.iter().enumerate() {
        if !empty[r] {
            continue;
        }
        let mut row = row.clone();
        row.sort_by(|&a, &b| {
            cmp(from[a], from[b])
                .then(cmp(to[a], to[b]))
                .then(a.cmp(&b))
        });
        let mut i = 0;
        while i < row.len() {
            let mut j = i;
            while j < row.len()
                && (from[row[j]] - from[row[i]]).abs() < 1.0
                && (to[row[j]] - to[row[i]]).abs() < 1.0
            {
                j += 1;
            }
            let mut twins = row[i..j].to_vec();
            twins.sort_by_key(|&v| g.pos[v]);
            for (k, &v) in twins.iter().enumerate() {
                xs[v] = want[v] + (k as f64 - (twins.len() - 1) as f64 / 2.0) * sep;
                if twins.len() == 1 || sep == 0.0 {
                    on_line[v] = true;
                }
            }
            i = j;
        }
    }
}

fn past_labels(g: &Graph, xs: &mut [f64], chains: &[Vec<usize>], clear: f64, on_line: &mut [bool]) {
    let labelled: Vec<_> = g
        .layers
        .iter()
        .map(|row| {
            row.iter().any(|&v| g.nodes[v].kind == Kind::Label)
                && row.iter().all(|&v| is_dummy(g.nodes[v].kind))
        })
        .collect();
    for chain in chains {
        for (i, &d) in chain.iter().enumerate() {
            if g.nodes[d].kind != Kind::Dummy || !labelled[g.nodes[d].rank] {
                continue;
            }
            let mut first = i;
            let mut last = i;
            while first > 0 && on_line[chain[first - 1]] {
                first -= 1;
            }
            while last + 1 < chain.len() && on_line[chain[last + 1]] {
                last += 1;
            }
            let above = &g.segments[g.pred[chain[first]][0]];
            let below = &g.segments[g.succ[chain[last]][0]];
            let from = xs[above.from] + above.from_dx;
            let to = xs[below.to] + below.to_dx;
            let lo = from.min(to) - clear;
            let hi = from.max(to) + clear;
            let blocked = g.layers[g.nodes[d].rank].iter().any(|&v| {
                g.nodes[v].kind == Kind::Label
                    && xs[v] + g.nodes[v].w / 2.0 > lo
                    && xs[v] - g.nodes[v].w / 2.0 < hi
            });
            if blocked {
                continue;
            }
            on_line[d] = true;
            let steps = last - first + 2;
            for (k, &v) in chain.iter().enumerate().take(last + 1).skip(first) {
                xs[v] = from + (to - from) * (k - first + 1) as f64 / steps as f64;
            }
        }
    }
}

fn common_group(groups: &Groups, mut a: i32, mut b: i32) -> i32 {
    let depth = |mut v: i32| {
        let mut d = 0;
        while v >= 0 {
            d += 1;
            v = groups.parent[v as usize];
        }
        d
    };
    let (mut da, mut db) = (depth(a), depth(b));
    while da > db {
        a = groups.parent[a as usize];
        da -= 1;
    }
    while db > da {
        b = groups.parent[b as usize];
        db -= 1;
    }
    while a != b {
        a = groups.parent[a as usize];
        b = groups.parent[b as usize];
    }
    a
}

fn free_ends(g: &Graph) -> [Vec<usize>; 2] {
    let mut free = [vec![0; g.nodes.len()], vec![0; g.nodes.len()]];
    for e in &g.segments {
        if e.to_kind != 0 {
            free[0][e.to] += 1;
        }
        if e.from_kind != 0 {
            free[1][e.from] += 1;
        }
    }
    free
}

fn label_room(g: &Graph, xs: &[f64], v: usize, opt: &LayeredOptions) -> (f64, f64) {
    let row = &g.layers[g.nodes[v].rank];
    let at = g.pos[v];
    let clear = |u: usize| {
        g.nodes[u].w / 2.0
            + g.nodes[v].w / 2.0
            + if g.nodes[u].kind == Kind::Node {
                opt.node_sep / 2.0 + opt.edge_sep / 2.0
            } else if g.nodes[u].kind == g.nodes[v].kind {
                opt.edge_sep
            } else {
                opt.edge_sep / 2.0
            }
    };
    (
        if at > 0 {
            xs[row[at - 1]] + clear(row[at - 1])
        } else {
            f64::NEG_INFINITY
        },
        if at + 1 < row.len() {
            xs[row[at + 1]] - clear(row[at + 1])
        } else {
            f64::INFINITY
        },
    )
}

fn choose_label(x: f64, up: (f64, usize), down: (f64, usize), lo: f64, hi: f64) -> f64 {
    let up_first = if up.1 != down.1 {
        up.1 < down.1
    } else {
        (up.0 - x).abs() <= (down.0 - x).abs()
    };
    let choices = if up_first {
        [up.0, down.0]
    } else {
        [down.0, up.0]
    };
    choices
        .into_iter()
        .find(|&c| c >= lo && c <= hi)
        .unwrap_or_else(|| choices[0].min(hi).max(lo))
}

fn label_end(g: &Graph, xs: &[f64], free: &[Vec<usize>; 2], e: usize, side: usize) -> (f64, usize) {
    let segment = &g.segments[e];
    let u = if side == 0 { segment.from } else { segment.to };
    let x = xs[u]
        + if side == 0 {
            segment.from_dx
        } else {
            segment.to_dx
        };
    (
        x,
        if u < g.real && g.nodes[u].kind == Kind::Node {
            free[1 - side][u]
        } else {
            0
        },
    )
}

fn align_labels(
    g: &Graph,
    xs: &mut [f64],
    free: &[Vec<usize>; 2],
    opt: &LayeredOptions,
    empty: &[bool],
) {
    let placed = |v: usize| {
        g.nodes[v].kind == Kind::Label
            || (g.nodes[v].kind == Kind::Dummy && !empty[g.nodes[v].rank])
    };
    let mut done = vec![false; g.nodes.len()];
    for r in 1..g.layers.len().saturating_sub(1) {
        for &v in &g.layers[r] {
            if !placed(v) || done[v] {
                continue;
            }
            let mut top = v;
            while let Some(&e) = g.pred[top].first() {
                let up = g.segments[e].from;
                if !is_dummy(g.nodes[up].kind) {
                    break;
                }
                top = up;
            }
            let mut bottom = top;
            let mut lo = f64::NEG_INFINITY;
            let mut hi = f64::INFINITY;
            let mut chain = Vec::new();
            let mut d = top;
            while is_dummy(g.nodes[d].kind) {
                bottom = d;
                if placed(d) {
                    done[d] = true;
                    chain.push(d);
                    let (least, most) = label_room(g, xs, d, opt);
                    lo = lo.max(least);
                    hi = hi.min(most);
                }
                let Some(&e) = g.succ[d].first() else { break };
                d = g.segments[e].to;
            }
            if chain.is_empty() {
                continue;
            }
            if lo <= hi {
                let x = choose_label(
                    xs[chain[0]],
                    label_end(g, xs, free, g.pred[top][0], 0),
                    label_end(g, xs, free, g.succ[bottom][0], 1),
                    lo,
                    hi,
                );
                for d in chain {
                    xs[d] = x;
                }
            } else {
                for d in chain {
                    let (least, most) = label_room(g, xs, d, opt);
                    if least > most {
                        continue;
                    }
                    xs[d] = choose_label(
                        xs[d],
                        label_end(g, xs, free, follow(g, empty, g.pred[d][0], 0), 0),
                        label_end(g, xs, free, follow(g, empty, g.succ[d][0], 1), 1),
                        least,
                        most,
                    );
                }
            }
        }
    }
}

fn wanted_end(
    g: &Graph,
    xs: &[f64],
    free: &[Vec<usize>; 2],
    empty: &[bool],
    choice: (usize, usize, usize, f64, bool),
) -> f64 {
    let (e, side, v, span, again) = choice;
    if free[side][v] < 2 {
        return 0.0;
    }
    let e = &g.segments[follow(g, empty, e, side)];
    let u = if side == 0 { e.from } else { e.to };
    let point = xs[u] + if side == 0 { e.from_dx } else { e.to_dx };
    if again && (point - xs[v]).abs() <= span / 2.0 + 0.5 {
        return (point - xs[v]).clamp(-span / 2.0, span / 2.0);
    }
    let mut middle = point;
    let mut width = 0.0;
    if u < g.real
        && g.nodes[u].kind == Kind::Node
        && (if side == 0 { e.from_kind } else { e.to_kind }) != 0
        && free[1 - side][u] >= 2
    {
        width = g.nodes[u].span.min(g.nodes[u].w).max(0.0);
        if width > 0.0 {
            middle = xs[u];
        }
    }
    let lo = (xs[v] - span / 2.0).max(middle - width / 2.0);
    let hi = (xs[v] + span / 2.0).min(middle + width / 2.0);
    if lo > hi + 0.5 {
        let lean = (span / 2.0 - 40.0).max(0.0);
        return (point - xs[v]).clamp(-lean, lean);
    }
    let place = (xs[v] * width + middle * span) / (width + span);
    place.min(hi).max(lo) - xs[v]
}

fn settle(g: &mut Graph, xs: &[f64], free: &[Vec<usize>; 2], empty: &[bool]) {
    let mut sides = Vec::new();
    for v in 0..g.real {
        let span = g.nodes[v].span.min(g.nodes[v].w);
        if span <= 0.0 || g.nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let (ids, mut room, _) = side_ends(g, v, side, span);
            if ids.is_empty() {
                continue;
            }
            if ids.len() == 1 {
                room[0] = 0.0;
            }
            let mut all = if side == 0 {
                g.pred[v].clone()
            } else {
                g.succ[v].clone()
            };
            all.sort_by_key(|&e| {
                (
                    g.pos[if side == 0 {
                        g.segments[e].from
                    } else {
                        g.segments[e].to
                    }],
                    e,
                )
            });
            sides.push((v, side, span, ids, room, all));
        }
    }
    for pass in 0..6 {
        let mut moved = false;
        for (v, side, span, ids, room, all) in &sides {
            let (v, side, span) = (*v, *side, *span);
            let mut k = 0;
            let mut lo = -span / 2.0;
            for i in 0..=all.len() {
                let held = i < all.len()
                    && (if side == 0 {
                        g.segments[all[i]].to_kind
                    } else {
                        g.segments[all[i]].from_kind
                    }) == 0;
                if i < all.len() && !held {
                    continue;
                }
                let mut size = 0;
                let mut j = i;
                while j > 0
                    && (if side == 0 {
                        g.segments[all[j - 1]].to_kind
                    } else {
                        g.segments[all[j - 1]].from_kind
                    }) != 0
                {
                    size += 1;
                    j -= 1;
                }
                let dx = |e: usize| {
                    if side == 0 {
                        g.segments[e].to_dx
                    } else {
                        g.segments[e].from_dx
                    }
                };
                let hi = if held { dx(all[i]) - 16.0 } else { span / 2.0 };
                if size > 0 {
                    let mut sums = Vec::<f64>::new();
                    let mut counts = Vec::<usize>::new();
                    let mut before = 0.0;
                    for j in 0..size {
                        if j > 0 {
                            before += room[k + j];
                        }
                        sums.push(
                            wanted_end(g, xs, free, empty, (ids[k + j], side, v, span, pass > 0))
                                - before,
                        );
                        counts.push(1);
                        let mut together = j > 0 && room[k + j] == 0.0;
                        while sums.len() > 1
                            && (together
                                || sums[sums.len() - 2] / counts[counts.len() - 2] as f64
                                    > sums[sums.len() - 1] / counts[counts.len() - 1] as f64)
                        {
                            together = false;
                            let top = sums.pop().unwrap();
                            let pooled = counts.pop().unwrap();
                            *sums.last_mut().unwrap() += top;
                            *counts.last_mut().unwrap() += pooled;
                        }
                    }
                    let total = before;
                    let mut j = 0;
                    before = 0.0;
                    for (sum, count) in sums.into_iter().zip(counts) {
                        let level = if hi - total < lo {
                            (lo + hi - total) / 2.0
                        } else {
                            (sum / count as f64).min(hi - total).max(lo)
                        };
                        for _ in 0..count {
                            if j > 0 {
                                before += room[k + j];
                            }
                            let segment = &mut g.segments[ids[k + j]];
                            let offset = if side == 0 {
                                &mut segment.to_dx
                            } else {
                                &mut segment.from_dx
                            };
                            moved |= (*offset - level - before).abs() > 0.01;
                            *offset = level + before;
                            j += 1;
                        }
                    }
                    k += size;
                }
                if held {
                    lo = (if side == 0 {
                        g.segments[all[i]].to_dx
                    } else {
                        g.segments[all[i]].from_dx
                    }) + 16.0;
                }
            }
        }
        if !moved {
            break;
        }
    }
}

fn join_blocks(block: &mut [usize], members: &mut [Vec<usize>], x: usize, y: usize) {
    let (mut keep, mut drop) = (block[x], block[y]);
    if keep == drop {
        return;
    }
    if members[keep].len() < members[drop].len() {
        std::mem::swap(&mut keep, &mut drop);
    }
    let dropped = std::mem::take(&mut members[drop]);
    for v in dropped {
        block[v] = keep;
        members[keep].push(v);
    }
}

fn snap(g: &Graph, xs: &mut [f64], empty: &[bool], opt: &LayeredOptions) {
    let mut ends = Vec::new();
    for (ei, e) in g.segments.iter().enumerate() {
        if g.nodes[e.from].kind == Kind::Dummy && empty[g.nodes[e.from].rank] {
            continue;
        }
        let other = &g.segments[follow(g, empty, ei, 1)];
        ends.push((e.from, other.to, e.from_dx, other.to_dx));
    }
    let offset = |k: usize, at: &[f64]| {
        let (a, b, da, db) = ends[k];
        at[b] + db - at[a] - da
    };
    let mut block: Vec<_> = (0..g.nodes.len()).collect();
    let mut members: Vec<_> = (0..g.nodes.len()).map(|i| vec![i]).collect();
    let mut near = Vec::new();
    for (k, &(a, b, _, _)) in ends.iter().enumerate() {
        let d = offset(k, xs).abs();
        if d < 0.01 {
            join_blocks(&mut block, &mut members, a, b);
        } else if d <= 24.0 {
            near.push(k);
        }
    }
    near.sort_by(|&a, &b| cmp(offset(a, xs).abs(), offset(b, xs).abs()).then(a.cmp(&b)));
    let gap = |v: usize| {
        g.nodes[v].w / 2.0
            + if g.nodes[v].kind == Kind::Node {
                opt.node_sep
            } else {
                opt.edge_sep
            } / 2.0
    };
    for k in near {
        let d = offset(k, xs);
        let (a, b, _, _) = ends[k];
        if d.abs() < 0.01 {
            join_blocks(&mut block, &mut members, a, b);
            continue;
        }
        if d.abs() > 24.0 || block[a] == block[b] {
            continue;
        }
        let (upper, lower) = (block[a], block[b]);
        let first = if members[upper].len() <= members[lower].len() {
            upper
        } else {
            lower
        };
        for id in [first, if first == upper { lower } else { upper }] {
            let shift = if id == upper { d } else { -d };
            if d.abs() > 12.0 && !members[id].iter().all(|&v| g.nodes[v].kind == Kind::Dummy) {
                continue;
            }
            let free = members[id].iter().all(|&v| {
                let row = &g.layers[g.nodes[v].rank];
                let i = g.pos[v];
                if i > 0
                    && block[row[i - 1]] != id
                    && xs[v] + shift - xs[row[i - 1]] < gap(v) + gap(row[i - 1]) - 0.01
                {
                    return false;
                }
                if i + 1 < row.len()
                    && block[row[i + 1]] != id
                    && xs[row[i + 1]] - xs[v] - shift < gap(v) + gap(row[i + 1]) - 0.01
                {
                    return false;
                }
                true
            });
            if !free {
                continue;
            }
            for &v in &members[id] {
                xs[v] += shift;
            }
            join_blocks(&mut block, &mut members, a, b);
            break;
        }
    }
}

#[derive(Default)]
struct Pieces {
    from: Vec<f64>,
    to: Vec<f64>,
    rank: Vec<usize>,
    lower: Vec<Option<usize>>,
    track: Vec<i32>,
    levels: Vec<usize>,
    wide: Vec<bool>,
}

fn piece_pairs(list: &[usize], lo: &[f64], hi: &[f64]) -> Vec<(usize, usize)> {
    let mut order: Vec<_> = (0..list.len()).collect();
    order.sort_by(|&a, &b| cmp(lo[list[a]], lo[list[b]]));
    let mut pairs = Vec::new();
    for (x, &i) in order.iter().enumerate() {
        let reach = hi[list[i]] + 2.0;
        for &j in &order[x + 1..] {
            if lo[list[j]] > reach {
                break;
            }
            pairs.push((i.min(j), i.max(j)));
        }
    }
    pairs
}

fn union_find(group: &mut [usize], mut i: usize) -> usize {
    while group[i] != i {
        group[i] = group[group[i]];
        i = group[i];
    }
    i
}

fn piece_mates(p: &Pieces, a: usize, b: usize) -> bool {
    (p.from[a] - p.from[b]).abs() < 0.5 || (p.to[a] - p.to[b]).abs() < 0.5
}

fn piece_overlap(lo: &[f64], hi: &[f64], a: usize, b: usize) -> bool {
    lo[a] < hi[b] - 0.5 && lo[b] < hi[a] - 0.5
}

fn piece_within(x: f64, lo: &[f64], hi: &[f64], k: usize) -> usize {
    usize::from(x >= lo[k] - 0.5 && x <= hi[k] + 0.5)
}

fn piece_meet(p: &Pieces, lo: &[f64], hi: &[f64], a: usize, b: usize) -> bool {
    piece_overlap(lo, hi, a, b)
        || piece_within(p.to[a], lo, hi, b)
            + piece_within(p.from[b], lo, hi, a)
            + piece_within(p.to[b], lo, hi, a)
            + piece_within(p.from[a], lo, hi, b)
            > 0
}

fn lay_tracks(p: &mut Pieces, list: &mut [usize], lo: &[f64], hi: &[f64], curved: bool) -> usize {
    list.sort_by(|&a, &b| {
        cmp(hi[b] - lo[b], hi[a] - lo[a])
            .then(cmp(lo[a], lo[b]))
            .then(a.cmp(&b))
    });
    let count = list.len();
    let mut group: Vec<_> = (0..count).collect();
    let weighed = count <= 400;
    let pairs = if weighed {
        piece_pairs(list, lo, hi)
    } else {
        Vec::new()
    };
    let mut clashes = vec![Vec::new(); count];
    for &(i, j) in &pairs {
        if piece_overlap(lo, hi, list[i], list[j]) && !piece_mates(p, list[i], list[j]) {
            clashes[i].push(j);
            clashes[j].push(i);
        }
    }
    let mut above = vec![Vec::new(); count];
    let mut sequence: Vec<_> = (0..count).collect();
    if weighed {
        let mut waiting = vec![0_i32; count];
        let mut after = vec![Vec::new(); count];
        for &(i, j) in &pairs {
            let (a, b) = (list[i], list[j]);
            if !piece_meet(p, lo, hi, a, b) {
                continue;
            }
            let first_root = union_find(&mut group, i);
            let second_root = union_find(&mut group, j);
            group[first_root] = second_root;
            let ab = piece_within(p.to[a], lo, hi, b) + piece_within(p.from[b], lo, hi, a);
            let ba = piece_within(p.to[b], lo, hi, a) + piece_within(p.from[a], lo, hi, b);
            if ab == ba {
                continue;
            }
            let (first, second) = if ab < ba { (i, j) } else { (j, i) };
            above[second].push(first);
            after[first].push(second);
            waiting[second] += 1;
        }
        let mut placed = vec![false; count];
        sequence.clear();
        for _ in 0..count {
            let mut pick = None;
            for i in 0..count {
                if placed[i] {
                    continue;
                }
                if pick.is_none_or(|v| waiting[i] < waiting[v]) {
                    pick = Some(i);
                }
                if waiting[i] == 0 {
                    break;
                }
            }
            let pick = pick.unwrap();
            placed[pick] = true;
            sequence.push(pick);
            for &j in &after[pick] {
                waiting[j] -= 1;
            }
        }
    }
    let mut on_track: Vec<Vec<usize>> = Vec::new();
    let mut rows = 0;
    for at in sequence {
        let k = list[at];
        let mut t = 0_i32;
        for &i in &above[at] {
            if p.track[list[i]] >= t {
                t = p.track[list[i]] + 1;
            }
        }
        if weighed {
            let taken: BTreeSet<_> = clashes[at].iter().map(|&i| p.track[list[i]]).collect();
            while taken.contains(&t) {
                t += 1;
            }
        } else {
            while (t as usize) < on_track.len()
                && on_track[t as usize]
                    .iter()
                    .any(|&other| piece_overlap(lo, hi, k, other) && !piece_mates(p, k, other))
            {
                t += 1;
            }
            if t as usize == on_track.len() {
                on_track.push(Vec::new());
            }
            on_track[t as usize].push(k);
        }
        p.track[k] = t;
        rows = rows.max(t as usize + 1);
    }
    let mut first = vec![rows as i32; count];
    let mut last = vec![-1_i32; count];
    for (i, &k) in list.iter().enumerate() {
        let root = union_find(&mut group, i);
        first[root] = first[root].min(p.track[k]);
        last[root] = last[root].max(p.track[k]);
    }
    for (i, &k) in list.iter().enumerate() {
        let root = union_find(&mut group, i);
        p.levels[k] = (last[root] - first[root] + 1) as usize;
        if curved {
            p.track[k] -= first[root];
        }
    }
    rows
}

fn sample_piece(p: &Pieces, k: usize, h: f64) -> [[f64; 2]; 17] {
    let c = ease(p.to[k] - p.from[k], h);
    std::array::from_fn(|i| {
        let t = i as f64 / 16.0;
        let u = 1.0 - t;
        [
            p.from[k] + (p.to[k] - p.from[k]) * t * t * (3.0 - 2.0 * t),
            h * (3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t),
        ]
    })
}

fn piece_reaches(points: &[[f64; 2]; 17], line: &[[f64; 2]; 17], limit: f64) -> bool {
    let bound = limit * limit * (1.0 + 1e-9);
    for &[px, py] in points {
        for span in line.windows(2) {
            let dx = span[1][0] - span[0][0];
            let dy = span[1][1] - span[0][1];
            let d2 = dx * dx + dy * dy;
            let t = (((px - span[0][0]) * dx + (py - span[0][1]) * dy)
                / if d2 == 0.0 { 1.0 } else { d2 })
            .clamp(0.0, 1.0);
            let ex = px - span[0][0] - t * dx;
            let ey = py - span[0][1] - t * dy;
            if ex * ex + ey * ey < bound && ex.hypot(ey) < limit {
                return true;
            }
        }
    }
    false
}

fn pieces_near(p: &Pieces, a: usize, b: usize, height: f64, limit: f64) -> bool {
    let h = (height / 2.0).max(height - 24.0);
    let line_a = sample_piece(p, a, h);
    let line_b = sample_piece(p, b, h);
    piece_reaches(&line_a, &line_b, limit) || piece_reaches(&line_b, &line_a, limit)
}

fn assign_tracks(
    p: &mut Pieces,
    track_count: &mut [usize],
    curved: bool,
    room: &[f64],
    tall: &mut [f64],
) {
    let mut by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    p.track = vec![-1; p.from.len()];
    p.levels = vec![0; p.from.len()];
    for k in 0..p.from.len() {
        if (p.from[k] - p.to[k]).abs() >= 0.5 {
            by_rank.entry(p.rank[k]).or_default().push(k);
        }
    }
    let lo: Vec<_> = p.from.iter().zip(&p.to).map(|(&a, &b)| a.min(b)).collect();
    let hi: Vec<_> = p.from.iter().zip(&p.to).map(|(&a, &b)| a.max(b)).collect();
    for (r, mut all) in by_rank {
        if !curved {
            track_count[r] = lay_tracks(p, &mut all, &lo, &hi, curved);
            continue;
        }
        if all.len() > 400 {
            continue;
        }
        let mut far: Vec<_> = all.iter().copied().filter(|&k| p.wide[k]).collect();
        lay_tracks(p, &mut far, &lo, &hi, curved);
        let mut level: BTreeSet<_> = far.iter().copied().filter(|&k| p.levels[k] > 1).collect();
        let pairs = piece_pairs(&all, &lo, &hi);
        for &(i, j) in &pairs {
            let (a, b) = (all[i], all[j]);
            if (level.contains(&a) && level.contains(&b))
                || !(piece_overlap(&lo, &hi, a, b) && !piece_mates(p, a, b))
                || ((p.from[a] < p.to[a]) != (p.from[b] < p.to[b]))
            {
                continue;
            }
            if pieces_near(
                p,
                a,
                b,
                room[r],
                if p.wide[a] || p.wide[b] { 8.0 } else { 4.0 },
            ) {
                level.insert(a);
                level.insert(b);
            }
        }
        for k in far {
            p.track[k] = -1;
            p.levels[k] = 0;
        }
        if level.is_empty() {
            continue;
        }
        let mut crossing = 0;
        let mut alongside = 0;
        let mut beside = BTreeSet::new();
        for &(i, j) in &pairs {
            let (a, b) = (all[i], all[j]);
            if level.contains(&a) && level.contains(&b) {
                if !piece_overlap(&lo, &hi, a, b) {
                    continue;
                }
                if (p.from[a] < p.to[a]) != (p.from[b] < p.to[b]) {
                    crossing += 1;
                } else if !piece_mates(p, a, b) {
                    alongside += 1;
                }
            } else if (level.contains(&a) || level.contains(&b)) && piece_meet(p, &lo, &hi, a, b) {
                beside.insert(a);
                beside.insert(b);
            }
        }
        if crossing > alongside {
            for k in all {
                tall[r] = tall[r].max((hi[k] - lo[k]) * 0.5);
            }
            continue;
        }
        let mut members: Vec<_> = all
            .into_iter()
            .filter(|k| level.contains(k) || (!p.wide[*k] && beside.contains(k)))
            .collect();
        lay_tracks(p, &mut members, &lo, &hi, curved);
        track_count[r] = members.into_iter().map(|k| p.levels[k]).max().unwrap_or(0);
    }
}

pub fn layered(nodes: &mut [LNode], edges: &mut [LEdge], opt: &LayeredOptions) -> LayeredResult {
    let real = nodes.len();
    if real == 0 {
        return LayeredResult::default();
    }
    if real == 1 && edges.is_empty() && nodes[0].kind == Kind::Node {
        nodes[0].rank = 0;
        nodes[0].x = nodes[0].w / 2.0;
        nodes[0].y = nodes[0].h / 2.0;
        return LayeredResult {
            width: nodes[0].w,
            height: nodes[0].h,
            ..Default::default()
        };
    }
    let mut seen = BTreeSet::new();
    let mut step = 1;
    for e in edges.iter() {
        let pair = (e.tail.min(e.head), e.tail.max(e.head));
        if e.label_w > 0.0 || !seen.insert(pair) {
            step = 2;
            break;
        }
    }
    orient(nodes, edges);
    rank(nodes, edges, step);
    let spans: Vec<_> = edges
        .iter()
        .map(|e| nodes[e.head].rank.abs_diff(nodes[e.tail].rank))
        .collect();
    let mut dummies: usize = spans.iter().map(|&s| s.saturating_sub(1)).sum();
    let mut direct = vec![false; edges.len()];
    if dummies > 20_000 {
        let mut longest: Vec<_> = (0..edges.len()).filter(|&i| spans[i] > 1).collect();
        longest.sort_by(|&a, &b| spans[b].cmp(&spans[a]).then(a.cmp(&b)));
        for ei in longest {
            if dummies <= 20_000 {
                break;
            }
            direct[ei] = true;
            dummies -= spans[ei] - 1;
        }
    }
    let max_rank = nodes.iter().map(|n| n.rank).max().unwrap_or(0);
    let mut work = nodes.to_vec();
    work.reserve(dummies);
    let mut segments = Vec::new();
    let mut chains = vec![Vec::new(); edges.len()];
    let mut first_seg = vec![None; edges.len()];
    let mut last_seg = vec![None; edges.len()];
    let mut label_node = vec![None; edges.len()];
    let mut heads = vec![0_u8; max_rank + 1];
    let mut groups = opt.groups.clone();
    if let Some(groups) = &mut groups {
        groups.of.resize(real, -1);
    }
    for (ei, e) in edges.iter().enumerate() {
        if direct[ei] || e.tail == e.head {
            continue;
        }
        let (a, b) = if e.reversed {
            (e.head, e.tail)
        } else {
            (e.tail, e.head)
        };
        let (a_dx, b_dx) = if e.reversed {
            (e.head_dx, e.tail_dx)
        } else {
            (e.tail_dx, e.head_dx)
        };
        let (ra, rb) = (work[a].rank, work[b].rank);
        if rb <= ra {
            continue;
        }
        let label_rank = if e.label_w > 0.0 {
            let mut r = ra + (rb - ra) / 2;
            if step == 2 && rb - ra >= 4 {
                if r & 1 == 1 {
                    r = if e.reversed { r + 1 } else { r - 1 };
                }
            } else if step == 2 && r & 1 == 0 {
                r = if e.reversed {
                    r + 1
                } else {
                    r.saturating_sub(1)
                };
            }
            Some(r)
        } else {
            None
        };
        let group = groups
            .as_ref()
            .map(|g| common_group(g, g.of[e.tail], g.of[e.head]));
        let mut prev = a;
        let from_kind = if if e.reversed {
            e.head_fixed
        } else {
            e.tail_fixed
        } {
            0
        } else if e.reversed {
            2
        } else {
            1
        };
        let to_kind = if if e.reversed {
            e.tail_fixed
        } else {
            e.head_fixed
        } {
            0
        } else if e.reversed {
            1
        } else {
            2
        };
        first_seg[ei] = Some(segments.len());
        for (r, head_mask) in heads.iter_mut().enumerate().take(rb).skip(ra + 1) {
            let d = work.len();
            let label = label_rank == Some(r);
            let mut node = lnode_kind(
                if label { e.label_w } else { 0.0 },
                if label { e.label_h } else { 0.0 },
                if label { Kind::Label } else { Kind::Dummy },
            );
            node.rank = r;
            node.key =
                work[a].key + (work[b].key - work[a].key) * (r - ra) as f64 / (rb - ra) as f64;
            work.push(node);
            if let Some(groups) = &mut groups {
                groups.of.push(group.unwrap_or(-1));
            }
            if label {
                label_node[ei] = Some(d);
                *head_mask |= if e.reversed { 2 } else { 1 };
            }
            chains[ei].push(d);
            segments.push(Segment {
                from: prev,
                to: d,
                from_dx: if prev == a { a_dx } else { 0.0 },
                to_dx: 0.0,
                from_kind: if prev == a { from_kind } else { 0 },
                to_kind: 0,
            });
            prev = d;
        }
        last_seg[ei] = Some(segments.len());
        segments.push(Segment {
            from: prev,
            to: b,
            from_dx: if prev == a { a_dx } else { 0.0 },
            to_dx: b_dx,
            from_kind: if prev == a { from_kind } else { 0 },
            to_kind,
        });
    }
    let n = work.len();
    let mut succ = vec![Vec::new(); n];
    let mut pred = vec![Vec::new(); n];
    for (ei, e) in segments.iter().enumerate() {
        succ[e.from].push(ei);
        pred[e.to].push(ei);
    }
    let mut by_rank = vec![Vec::new(); max_rank + 1];
    for (i, node) in work.iter().enumerate() {
        by_rank[node.rank].push(i);
    }
    let mut visited = vec![false; n];
    let mut layers = vec![Vec::new(); max_rank + 1];
    let mut stack = Vec::new();
    for row in &by_rank {
        for &root in row {
            if visited[root] {
                continue;
            }
            stack.push(root);
            while let Some(v) = stack.pop() {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                layers[work[v].rank].push(v);
                for &ei in succ[v].iter().rev() {
                    let w = segments[ei].to;
                    if !visited[w] {
                        stack.push(w);
                    }
                }
            }
        }
    }
    let mut fixed = vec![false; max_rank + 1];
    for row in &mut layers {
        if row.iter().all(|&v| !work[v].key.is_nan()) {
            row.sort_by(|&a, &b| cmp(work[a].key, work[b].key));
        }
        if work[..real].iter().any(|node| node.pin != 0.0) {
            row.sort_by(|&a, &b| cmp(work[a].pin, work[b].pin));
        }
    }
    for r in [0, max_rank] {
        let row = &mut layers[r];
        if row.len() > 1
            && row.iter().all(|&v| {
                matches!(work[v].kind, Kind::StartPort | Kind::EndPort) && !work[v].hint.is_nan()
            })
        {
            row.sort_by(|&a, &b| cmp(work[a].hint, work[b].hint));
            fixed[r] = true;
        }
    }
    let mut g = Graph {
        real,
        nodes: work,
        segments,
        succ,
        pred,
        layers,
        pos: vec![0; n],
        fixed,
        groups,
    };
    g.set_positions();
    let (crossings, declared) = g.reduce_crossings();
    if opt.order_only {
        let at = position(&g, opt);
        for i in 0..real {
            nodes[i].x = at[i];
        }
        for (ei, e) in edges.iter_mut().enumerate() {
            let (a, b) = if e.reversed {
                (e.head, e.tail)
            } else {
                (e.tail, e.head)
            };
            let mut points = vec![g.nodes[a].rank as f64, at[a]];
            for &d in &chains[ei] {
                points.extend([g.nodes[d].rank as f64, at[d]]);
            }
            points.extend([g.nodes[b].rank as f64, at[b]]);
            e.points = points;
        }
        return LayeredResult {
            crossings: Some(crossings),
            declared: Some(declared),
            ..Default::default()
        };
    }
    let spans_nodes = g.nodes[..real]
        .iter()
        .any(|node| node.span > 0.0 && node.kind == Kind::Node);
    if spans_nodes {
        spread(&mut g);
    }
    let mut xs = position(&g, opt);
    let empty = empty_ranks(&g);
    if spans_nodes {
        let free = free_ends(&g);
        align_labels(&g, &mut xs, &free, opt, &empty);
        settle(&mut g, &xs, &free, &empty);
        snap(&g, &mut xs, &empty, opt);
    }
    align_ports(&g, &mut xs, &empty);
    let mut on_line = vec![false; n];
    if !opt.tracks {
        straighten(
            &g,
            &mut xs,
            &empty,
            &chains,
            if opt.apart { 0.0 } else { opt.edge_sep },
            &mut on_line,
        );
        past_labels(&g, &mut xs, &chains, opt.edge_sep, &mut on_line);
    }
    for (ei, e) in edges.iter_mut().enumerate() {
        if let (Some(first), Some(last)) = (first_seg[ei], last_seg[ei]) {
            let a_dx = g.segments[first].from_dx;
            let b_dx = g.segments[last].to_dx;
            e.tail_dx = if e.reversed { b_dx } else { a_dx };
            e.head_dx = if e.reversed { a_dx } else { b_dx };
        }
    }
    let mut piece_start = vec![0; edges.len() + 1];
    let mut pieces = if opt.tracks || (spans_nodes && !opt.straight) {
        Some(Pieces::default())
    } else {
        None
    };
    if let Some(p) = &mut pieces {
        for ei in 0..edges.len() {
            piece_start[ei] = p.from.len();
            let (Some(first), Some(last)) = (first_seg[ei], last_seg[ei]) else {
                continue;
            };
            if chains[ei]
                .iter()
                .any(|&d| !opt.tracks && empty[g.nodes[d].rank] != on_line[d])
            {
                continue;
            }
            let start = &g.segments[first];
            let mut x = xs[start.from] + start.from_dx;
            let mut r = g.nodes[start.from].rank;
            for &d in &chains[ei] {
                if empty[g.nodes[d].rank] {
                    continue;
                }
                p.from.push(x);
                p.to.push(xs[d]);
                p.rank.push(r);
                p.lower.push(Some(d));
                x = xs[d];
                r = g.nodes[d].rank;
            }
            let last = &g.segments[last];
            p.from.push(x);
            p.to.push(xs[last.to] + last.to_dx);
            p.rank.push(r);
            p.lower.push(None);
        }
        piece_start[edges.len()] = p.from.len();
    }
    let port_only = |r: usize, kind: Kind| {
        g.layers[r].iter().any(|&i| g.nodes[i].kind == kind)
            && g.layers[r]
                .iter()
                .all(|&i| g.nodes[i].kind == kind || g.nodes[i].kind == Kind::Dummy)
    };
    let start_ranks = if port_only(0, Kind::StartPort) {
        step
    } else {
        0
    };
    let end_ranks = if port_only(max_rank, Kind::EndPort) {
        step
    } else {
        0
    };
    let mut run = vec![0.0_f64; max_rank + 1];
    for e in &g.segments {
        let across = (xs[e.from] + e.from_dx - xs[e.to] - e.to_dx).abs();
        let r = g.nodes[e.from].rank;
        run[r] = run[r].max(across);
    }
    let mut gap_under = vec![0.0; max_rank + 1];
    for r in 0..max_rank {
        let usual = if r < start_ranks || r >= max_rank.saturating_sub(end_ranks) {
            opt.port_sep
        } else {
            opt.rank_sep
        } / step as f64;
        gap_under[r] = if pieces.is_some() {
            usual
        } else {
            usual.max((3.0 * opt.rank_sep / step as f64).min(run[r] * 0.125))
        };
    }
    let mut between = vec![true; max_rank + 1];
    for node in &g.nodes[..real] {
        between[node.rank] = false;
    }
    for r in 1..max_rank {
        if opt.tracks || !between[r] || !matches!(heads[r], 1 | 2) {
            continue;
        }
        let shift =
            (opt.head_room / 2.0).min(gap_under[if heads[r] == 1 { r - 1 } else { r }] / 2.0);
        gap_under[r - 1] += if heads[r] == 1 { -shift } else { shift };
        gap_under[r] += if heads[r] == 1 { shift } else { -shift };
    }
    let margin = if opt.tracks { 18.0 } else { 28.0 };
    let least_pitch = if opt.tracks { 10.0 } else { 12.0 };
    let most_pitch: f64 = if opt.tracks { 14.0 } else { 12.0 };
    let mut below = vec![0; max_rank + 1];
    let mut track_count = vec![0; max_rank + 1];
    if let Some(p) = &mut pieces {
        let mut room = vec![0.0; max_rank + 1];
        for r in 0..max_rank {
            if empty[r] {
                continue;
            }
            let mut next = r + 1;
            room[r] = gap_under[r];
            while next < max_rank && empty[next] {
                room[r] += gap_under[next];
                next += 1;
            }
            below[r] = next;
        }
        p.wide = p
            .from
            .iter()
            .zip(&p.to)
            .enumerate()
            .map(|(k, (&a, &b))| (a - b).abs() > 2.0 * room[p.rank[k]])
            .collect();
        let mut tall = vec![0.0; max_rank + 1];
        assign_tracks(p, &mut track_count, !opt.tracks, &room, &mut tall);
        for r in 0..max_rank {
            if track_count[r] > 0 {
                gap_under[r] +=
                    (2.0 * margin + (track_count[r] - 1) as f64 * least_pitch - room[r]).max(0.0);
            } else if tall[r] > 0.0 {
                gap_under[r] += ((3.0 * opt.rank_sep).min(tall[r]) - room[r]).max(0.0);
            }
        }
    }
    let mut band_top = vec![0.0; max_rank + 1];
    let mut band_bottom = vec![0.0; max_rank + 1];
    let mut y = 0.0;
    for r in 0..=max_rank {
        let h = g.layers[r]
            .iter()
            .map(|&i| g.nodes[i].h)
            .fold(0.0, f64::max);
        band_top[r] = y;
        band_bottom[r] = y + h;
        y += h + gap_under[r];
    }
    let height = y;
    let min_x = (0..n)
        .map(|i| xs[i] - g.nodes[i].w / 2.0)
        .fold(f64::INFINITY, f64::min);
    let max_x = (0..n)
        .map(|i| xs[i] + g.nodes[i].w / 2.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut arrives = vec![false; max_rank + 1];
    let mut leaves = vec![false; max_rank + 1];
    if spans_nodes {
        for i in 0..real {
            if g.nodes[i].kind == Kind::Node {
                arrives[g.nodes[i].rank] |= !g.pred[i].is_empty();
                leaves[g.nodes[i].rank] |= !g.succ[i].is_empty();
            }
        }
    }
    for i in 0..real {
        let node = &mut nodes[i];
        let r = node.rank;
        node.x = xs[i] - min_x;
        node.y = if node.kind == Kind::Node && leaves[r] && !arrives[r] {
            band_bottom[r] - node.h / 2.0
        } else if node.kind == Kind::Node && arrives[r] && !leaves[r] {
            band_top[r] + node.h / 2.0
        } else {
            (band_top[r] + band_bottom[r]) / 2.0
        };
    }
    let mut width = max_x - min_x;
    for (ei, e) in edges.iter_mut().enumerate() {
        if e.tail == e.head {
            let node = &nodes[e.tail];
            let right = node.x + node.w / 2.0;
            let reach = (opt.edge_sep + e.label_w).max(30.0);
            e.points = vec![
                right,
                node.y - node.h / 4.0,
                right + reach,
                node.y - node.h / 4.0,
                right + reach,
                node.y + node.h / 4.0,
                right,
                node.y + node.h / 4.0,
            ];
            e.label_x = right + reach;
            e.label_y = node.y;
            width = width.max(right + reach + e.label_w / 2.0);
            continue;
        }
        let first = &nodes[if e.reversed { e.head } else { e.tail }];
        let last = &nodes[if e.reversed { e.tail } else { e.head }];
        let mut pts = vec![
            first.x + if e.reversed { e.head_dx } else { e.tail_dx },
            first.y + first.h / 2.0,
        ];
        let inset = |d: usize| {
            let r = g.nodes[d].rank;
            if g.nodes[d].kind == Kind::Dummy && between[r] && !opt.tracks {
                (band_bottom[r] - band_top[r]) / 4.0
            } else {
                0.0
            }
        };
        if let Some(p) = &pieces
            && piece_start[ei + 1] > piece_start[ei]
        {
            let track_y = |k: usize, on_track: i32| {
                let r = p.rank[k];
                let top = band_bottom[r];
                let bottom = band_top[below[r]];
                let count = if opt.tracks {
                    track_count[r]
                } else {
                    p.levels[k]
                };
                let pitch = if count > 1 {
                    most_pitch.min((bottom - top - 2.0 * margin) / (count - 1) as f64)
                } else {
                    0.0
                };
                (top + bottom) / 2.0 + (on_track as f64 - (count as f64 - 1.0) / 2.0) * pitch
            };
            for k in piece_start[ei]..piece_start[ei + 1] {
                if p.track[k] >= 0 && (opt.tracks || p.wide[k]) {
                    let across = track_y(k, p.track[k]);
                    pts.extend([p.from[k] - min_x, across, p.to[k] - min_x, across]);
                } else if p.track[k] > 0 {
                    pts.extend([p.from[k] - min_x, track_y(k, p.track[k] - 1)]);
                }
                if let Some(d) = p.lower[k] {
                    let r = g.nodes[d].rank;
                    pts.extend([xs[d] - min_x, band_top[r] + inset(d)]);
                    if band_bottom[r] > band_top[r] {
                        pts.extend([xs[d] - min_x, band_bottom[r] - inset(d)]);
                    }
                }
            }
        } else {
            for &d in &chains[ei] {
                if on_line[d] {
                    continue;
                }
                let r = g.nodes[d].rank;
                pts.extend([xs[d] - min_x, band_top[r] + inset(d)]);
                if band_bottom[r] > band_top[r] {
                    pts.extend([xs[d] - min_x, band_bottom[r] - inset(d)]);
                }
            }
        }
        pts.extend([
            last.x + if e.reversed { e.tail_dx } else { e.head_dx },
            last.y - last.h / 2.0,
        ]);
        if e.reversed {
            let mut reversed = Vec::with_capacity(pts.len());
            for point in pts.as_chunks::<2>().0.iter().rev() {
                reversed.extend(point);
            }
            pts = reversed;
        }
        if let Some(ln) = label_node[ei] {
            let r = g.nodes[ln].rank;
            e.label_x = xs[ln] - min_x;
            e.label_y = (band_top[r] + band_bottom[r]) / 2.0;
        } else if e.label_w > 0.0 {
            e.label_x = (pts[0] + pts[pts.len() - 2]) / 2.0;
            e.label_y = (pts[1] + pts[pts.len() - 1]) / 2.0;
        }
        e.points = pts;
    }
    LayeredResult {
        width,
        height,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn empty_and_single_node_have_exact_bounds() {
        let mut nodes = Vec::new();
        let size = layered(&mut nodes, &mut [], &LayeredOptions::default());
        close(size.width, 0.0);
        close(size.height, 0.0);
        nodes.push(lnode(80.0, 40.0));
        let size = layered(&mut nodes, &mut [], &LayeredOptions::default());
        close(size.width, 80.0);
        close(size.height, 40.0);
        close(nodes[0].x, 40.0);
        close(nodes[0].y, 20.0);
    }

    #[test]
    fn chain_matches_pele_reference_coordinates() {
        let mut nodes = vec![lnode(80.0, 40.0), lnode(100.0, 60.0), lnode(60.0, 40.0)];
        let mut edges = vec![ledge(0, 1), ledge(1, 2)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        close(size.width, 100.0);
        close(size.height, 240.0);
        for (node, (x, y, rank)) in
            nodes
                .iter()
                .zip([(50.0, 20.0, 0), (50.0, 120.0, 1), (50.0, 220.0, 2)])
        {
            close(node.x, x);
            close(node.y, y);
            assert_eq!(node.rank, rank);
        }
        assert_eq!(edges[0].points, [50.0, 40.0, 50.0, 90.0]);
        assert_eq!(edges[1].points, [50.0, 150.0, 50.0, 200.0]);
    }

    #[test]
    fn diamond_matches_pele_reference_coordinates() {
        let mut nodes = vec![lnode(80.0, 40.0); 4];
        let mut edges = vec![ledge(0, 1), ledge(0, 2), ledge(1, 3), ledge(2, 3)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        close(size.width, 200.0);
        close(size.height, 220.0);
        for (node, (x, y)) in
            nodes
                .iter()
                .zip([(100.0, 20.0), (40.0, 110.0), (160.0, 110.0), (100.0, 200.0)])
        {
            close(node.x, x);
            close(node.y, y);
        }
        assert_eq!(edges[0].points, [100.0, 40.0, 40.0, 90.0]);
    }

    #[test]
    fn cycle_matches_reference_and_routes_back_to_original_head() {
        let mut nodes = vec![lnode(80.0, 40.0); 3];
        let mut edges = vec![ledge(0, 1), ledge(1, 2), ledge(2, 0)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        close(size.width, 115.0);
        close(size.height, 220.0);
        assert!(edges[2].reversed);
        assert_eq!(
            edges[2].points,
            [75.0, 180.0, 110.0, 130.0, 110.0, 90.0, 75.0, 40.0]
        );
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn label_reserves_a_middle_rank_like_pele() {
        let mut nodes = vec![lnode(80.0, 40.0); 2];
        let mut edges = vec![ledge_with(0, 1, 1, 60.0, 20.0)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        close(size.width, 80.0);
        close(size.height, 150.0);
        close(edges[0].label_x, 40.0);
        close(edges[0].label_y, 75.0);
        assert_eq!(
            edges[0].points,
            [40.0, 40.0, 40.0, 65.0, 40.0, 85.0, 40.0, 110.0]
        );
        assert_eq!(nodes[1].rank, 2);
    }

    #[test]
    fn crossing_reduction_keeps_declaration_order_when_it_can() {
        let mut nodes = vec![lnode(80.0, 40.0); 4];
        let mut edges = vec![ledge(0, 3), ledge(1, 2)];
        let opt = LayeredOptions {
            order_only: true,
            ..Default::default()
        };
        let size = layered(&mut nodes, &mut edges, &opt);
        assert_eq!(size.crossings, Some(0));
        assert!(nodes[0].x < nodes[1].x);
        assert!(nodes[3].x < nodes[2].x);
        assert_eq!(edges[0].points[0], 0.0);
        assert_eq!(edges[0].points[2], 1.0);
    }

    #[test]
    fn explicit_ports_are_ranked_and_aligned_with_their_nodes() {
        let mut nodes = vec![
            lnode_kind(0.0, 0.0, Kind::StartPort),
            lnode(80.0, 40.0),
            lnode_kind(0.0, 0.0, Kind::EndPort),
        ];
        let mut edges = vec![ledge(0, 1), ledge(1, 2)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        assert!(size.width.is_finite() && size.height.is_finite());
        assert_eq!(nodes.iter().map(|n| n.rank).collect::<Vec<_>>(), [0, 1, 2]);
        close(nodes[0].x, nodes[1].x);
        close(nodes[2].x, nodes[1].x);
    }

    #[test]
    fn pinned_nodes_and_sibling_groups_remain_ordered() {
        let mut nodes = vec![lnode(40.0, 20.0); 6];
        nodes[0].pin = -1.0;
        nodes[1].pin = 1.0;
        let mut edges = vec![ledge(0, 5), ledge(1, 2), ledge(0, 3), ledge(1, 4)];
        let options = LayeredOptions {
            groups: Some(Groups {
                of: vec![-1, -1, 0, 1, 0, 1],
                parent: vec![-1, -1],
            }),
            ..Default::default()
        };
        layered(&mut nodes, &mut edges, &options);
        assert!(nodes[0].x < nodes[1].x);
        let mut bottom = [2, 3, 4, 5];
        bottom.sort_by(|&a, &b| cmp(nodes[a].x, nodes[b].x));
        let labels: Vec<_> = bottom
            .iter()
            .map(|&i| options.groups.as_ref().unwrap().of[i])
            .collect();
        assert!(
            labels == [0, 0, 1, 1] || labels == [1, 1, 0, 0],
            "{labels:?}"
        );
    }

    #[test]
    fn parallel_edges_keep_distinct_routes() {
        let mut nodes = vec![lnode(80.0, 40.0); 2];
        let mut edges = vec![ledge(0, 1), ledge(0, 1)];
        layered(&mut nodes, &mut edges, &LayeredOptions::default());
        assert_ne!(edges[0].points, edges[1].points);
        assert!(edges.iter().all(|e| e.points.iter().all(|p| p.is_finite())));
    }

    #[test]
    fn deterministic_dags_have_finite_nonoverlapping_nodes() {
        let mut state = 0x1234_5678_u64;
        for count in 2..28 {
            let mut nodes = (0..count)
                .map(|i| lnode(20.0 + (i % 7) as f64 * 11.0, 20.0 + (i % 4) as f64 * 9.0))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for a in 0..count {
                for b in a + 1..count {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    if state >> 60 < 3 {
                        edges.push(ledge(a, b));
                    }
                }
            }
            let (mut again_nodes, mut again_edges) = (nodes.clone(), edges.clone());
            let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
            let again = layered(
                &mut again_nodes,
                &mut again_edges,
                &LayeredOptions::default(),
            );
            close(size.width, again.width);
            close(size.height, again.height);
            for (a, node) in nodes.iter().enumerate() {
                assert!(node.x.is_finite() && node.y.is_finite());
                close(node.x, again_nodes[a].x);
                close(node.y, again_nodes[a].y);
                assert!(node.x - node.w / 2.0 >= -1e-9);
                assert!(node.x + node.w / 2.0 <= size.width + 1e-9);
                for other in &nodes[a + 1..] {
                    if node.rank == other.rank {
                        assert!((node.x - other.x).abs() + 1e-9 >= (node.w + other.w) / 2.0);
                    }
                }
            }
            assert!(edges.iter().all(|e| e.points.iter().all(|p| p.is_finite())));
        }
    }

    #[test]
    fn long_edges_obey_dummy_budget_without_mutating_node_count() {
        let mut nodes = vec![lnode(30.0, 20.0); 4];
        let mut edges = vec![
            ledge_with(0, 3, 10_001, 0.0, 0.0),
            ledge_with(1, 3, 10_001, 0.0, 0.0),
            ledge_with(2, 3, 10_001, 0.0, 0.0),
        ];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        assert_eq!(nodes.len(), 4);
        assert!(size.width.is_finite() && size.height.is_finite());
        assert!(edges.iter().any(|e| e.points.len() == 4));
    }
    #[test]
    fn endpoint_settling_and_weighted_tracks_match_pele() {
        for tracks in [false, true] {
            let mut nodes = vec![lnode(100.0, 40.0); 6];
            for node in &mut nodes {
                node.span = 80.0;
            }
            let mut edges = [(0, 2), (0, 3), (0, 4), (1, 3), (1, 4), (1, 5)]
                .into_iter()
                .map(|(a, b)| ledge(a, b))
                .collect::<Vec<_>>();
            let options = LayeredOptions {
                tracks,
                ..Default::default()
            };
            let size = layered(&mut nodes, &mut edges, &options);
            close(size.width, 520.0);
            close(size.height, if tracks { 130.0 } else { 146.79963991769546 });
            close(nodes[0].x, 178.79963991769546);
            close(nodes[1].x, 341.2003600823045);
            close(edges[0].tail_dx, -17.599279835390945);
            close(edges[1].tail_dx, -1.599279835390945);
            close(edges[1].head_dx, -12.799639917695472);
            let wanted = if tracks {
                vec![
                    326.79963991769546,
                    40.0,
                    326.79963991769546,
                    72.0,
                    193.20036008230454,
                    72.0,
                    193.20036008230454,
                    90.0,
                ]
            } else {
                vec![
                    326.79963991769546,
                    40.0,
                    193.20036008230454,
                    106.79963991769546,
                ]
            };
            assert_eq!(edges[3].points.len(), wanted.len());
            for (&a, b) in edges[3].points.iter().zip(wanted) {
                close(a, b);
            }
        }
    }

    #[test]
    fn self_loop_is_bounded_and_keeps_the_declared_endpoints() {
        let mut nodes = vec![lnode(80.0, 40.0)];
        let mut edges = vec![ledge(0, 0)];
        let size = layered(&mut nodes, &mut edges, &LayeredOptions::default());
        assert!(size.width > nodes[0].w);
        assert!(edges[0].points.iter().all(|v| v.is_finite()));
        close(edges[0].points[0], nodes[0].x + nodes[0].w / 2.0);
        close(edges[0].points[6], nodes[0].x + nodes[0].w / 2.0);
        assert!(edges[0].points[1] < edges[0].points[7]);
    }
}
