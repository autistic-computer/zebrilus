use pele::layout::{Kind, LEdge, LNode, LayeredOptions, layered, ledge_with, lnode_kind};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    upstream_revision: String,
    seed: u64,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    opt: Options,
    expected: Expected,
}

#[derive(Deserialize)]
struct Node {
    w: f64,
    h: f64,
    kind: u8,
    pin: f64,
    hint: Option<f64>,
    key: Option<f64>,
    span: f64,
    x: f64,
    y: f64,
    rank: usize,
}

impl Node {
    fn native(&self) -> LNode {
        let kind = match self.kind {
            0 => Kind::Node,
            1 => Kind::Dummy,
            2 => Kind::Label,
            3 => Kind::StartPort,
            4 => Kind::EndPort,
            other => panic!("Unknown upstream node kind {other}"),
        };
        let mut node = lnode_kind(self.w, self.h, kind);
        node.pin = self.pin;
        node.hint = self.hint.unwrap_or(f64::NAN);
        node.key = self.key.unwrap_or(f64::NAN);
        node.span = self.span;
        node.x = self.x;
        node.y = self.y;
        node.rank = self.rank;
        node
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Edge {
    tail: usize,
    head: usize,
    minlen: usize,
    label_w: f64,
    label_h: f64,
    tail_dx: f64,
    head_dx: f64,
    tail_fixed: bool,
    head_fixed: bool,
    reversed: bool,
    points: Vec<f64>,
    label_x: f64,
    label_y: f64,
}

impl Edge {
    fn native(&self) -> LEdge {
        let mut edge = ledge_with(
            self.tail,
            self.head,
            self.minlen,
            self.label_w,
            self.label_h,
        );
        edge.tail_dx = self.tail_dx;
        edge.head_dx = self.head_dx;
        edge.tail_fixed = self.tail_fixed;
        edge.head_fixed = self.head_fixed;
        edge.reversed = self.reversed;
        edge.points.clone_from(&self.points);
        edge.label_x = self.label_x;
        edge.label_y = self.label_y;
        edge
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    node_sep: f64,
    edge_sep: f64,
    rank_sep: f64,
    port_sep: f64,
    tracks: bool,
    straight: bool,
    head_room: f64,
}

#[derive(Deserialize)]
struct Bounds {
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct Expected {
    result: Bounds,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

fn close(actual: f64, expected: f64, case: usize, field: &str) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "case {case}, {field}: {actual} != {expected}"
    );
}

#[test]
fn seeded_layouts_match_pinned_pele_geometry() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/layout-reference.json")).unwrap();
    assert_eq!(corpus.upstream_revision, pele::UPSTREAM_REVISION);
    assert_eq!(corpus.seed, 123456789);
    assert_eq!(corpus.cases.len(), 120);
    for (index, case) in corpus.cases.into_iter().enumerate() {
        let mut nodes = case.nodes.iter().map(Node::native).collect::<Vec<_>>();
        let mut edges = case.edges.iter().map(Edge::native).collect::<Vec<_>>();
        let options = LayeredOptions {
            node_sep: case.opt.node_sep,
            edge_sep: case.opt.edge_sep,
            rank_sep: case.opt.rank_sep,
            port_sep: case.opt.port_sep,
            tracks: case.opt.tracks,
            straight: case.opt.straight,
            head_room: case.opt.head_room,
            ..Default::default()
        };
        let result = layered(&mut nodes, &mut edges, &options);
        close(result.width, case.expected.result.width, index, "width");
        close(result.height, case.expected.result.height, index, "height");
        assert_eq!(nodes.len(), case.expected.nodes.len());
        assert_eq!(edges.len(), case.expected.edges.len());
        for (node, expected) in nodes.iter().zip(case.expected.nodes) {
            close(node.x, expected.x, index, "node.x");
            close(node.y, expected.y, index, "node.y");
            assert_eq!(node.rank, expected.rank, "case {index}, node.rank");
        }
        for (edge, expected) in edges.iter().zip(case.expected.edges) {
            close(edge.tail_dx, expected.tail_dx, index, "edge.tail_dx");
            close(edge.head_dx, expected.head_dx, index, "edge.head_dx");
            close(edge.label_x, expected.label_x, index, "edge.label_x");
            close(edge.label_y, expected.label_y, index, "edge.label_y");
            assert_eq!(
                edge.reversed, expected.reversed,
                "case {index}, edge.reversed"
            );
            assert_eq!(
                edge.points.len(),
                expected.points.len(),
                "case {index}, edge.points length"
            );
            for (&actual, expected) in edge.points.iter().zip(expected.points) {
                close(actual, expected, index, "edge.points");
            }
        }
    }
}
