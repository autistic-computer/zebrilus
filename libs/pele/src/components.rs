use crate::compound::{CEdge, CNode, CompoundResult};

struct Part {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Part {
    fn include(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.left = self.left.min(x - w / 2.0);
        self.top = self.top.min(y - h / 2.0);
        self.right = self.right.max(x + w / 2.0);
        self.bottom = self.bottom.max(y + h / 2.0);
    }
}

fn join(parent: &mut [usize], a: usize, b: usize) {
    let ra = root(parent, a);
    let rb = root(parent, b);
    parent[ra] = rb;
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

pub fn wrap_components(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    width: f64,
    gap: f64,
) -> Option<CompoundResult> {
    let n = nodes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for (i, c) in nodes.iter().enumerate() {
        if let Some(p) = c.parent.filter(|&p| p < n) {
            join(&mut parent, i, p);
        }
    }
    for e in edges.iter() {
        join(&mut parent, e.src, e.dst);
    }

    let mut slot = vec![None; n];
    let mut parts: Vec<Part> = Vec::new();
    let mut node_part = vec![0; n];
    for (i, c) in nodes.iter().enumerate() {
        let id = root(&mut parent, i);
        let at = *slot[id].get_or_insert_with(|| {
            parts.push(Part {
                left: f64::INFINITY,
                top: f64::INFINITY,
                right: f64::NEG_INFINITY,
                bottom: f64::NEG_INFINITY,
            });
            parts.len() - 1
        });
        node_part[i] = at;
        parts[at].include(c.x, c.y, c.w, c.h);
    }
    if parts.len() < 2 {
        return None;
    }
    let mut edge_part = Vec::with_capacity(edges.len());
    for e in edges.iter() {
        let at = node_part[e.src];
        edge_part.push(at);
        let part = &mut parts[at];
        for point in e.route.as_chunks::<3>().0 {
            part.include(point[0], point[1], 0.0, 0.0);
        }
        if e.label_w > 0.0 {
            part.include(e.label_x, e.label_y, e.label_w, e.label_h);
        }
    }

    let (mut x, mut y, mut row_height, mut right) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut from, mut to) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut shifts = Vec::with_capacity(parts.len());
    for part in &parts {
        let w = part.right - part.left;
        let h = part.bottom - part.top;
        if x > 0.0 && x + w > width {
            y += row_height + gap;
            x = 0.0;
            row_height = 0.0;
        }
        shifts.push((x - part.left, y - part.top));
        from = from.min(part.left);
        to = to.max(part.right);
        right = right.max(x + w);
        row_height = row_height.max(h);
        x += w + gap;
    }
    if right >= to - from {
        return None;
    }
    for (c, &at) in nodes.iter_mut().zip(&node_part) {
        let (ox, oy) = shifts[at];
        c.x += ox;
        c.y += oy;
    }
    for (e, &at) in edges.iter_mut().zip(&edge_part) {
        let (ox, oy) = shifts[at];
        for point in e.route.as_chunks_mut::<3>().0 {
            point[0] += ox;
            point[1] += oy;
        }
        e.label_x += ox;
        e.label_y += oy;
    }
    Some(CompoundResult {
        width: right,
        height: y + row_height,
    })
}
