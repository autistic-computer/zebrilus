use pele::components::wrap_components;
use pele::compound::{
    CEdge, CNode, Dir, cnode, compound_layout, direction, inherit_crossed_directions,
};
use pele::layout::LayeredOptions;
use serde_json::Value;

const TOLERANCE: f64 = 1e-6;

fn number(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap(),
        Value::String(text) => match text.as_str() {
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => f64::NAN,
        },
        other => panic!("not a number: {other}"),
    }
}

fn index(value: &Value) -> Option<usize> {
    value.as_i64().and_then(|at| usize::try_from(at).ok())
}

fn node(value: &Value) -> CNode {
    let mut c = cnode(
        number(&value["w"]),
        number(&value["h"]),
        index(&value["parent"]),
    );
    c.is_group = value["isGroup"].as_bool().unwrap();
    c.dir = value["dir"].as_str().map(direction);
    c.pad_x = number(&value["padX"]);
    c.pad_top = number(&value["padTop"]);
    c.pad_bottom = number(&value["padBottom"]);
    c.min_w = number(&value["minW"]);
    c.seq = value["seq"].as_u64().unwrap() as usize;
    c.span = number(&value["span"]);
    c.x = number(&value["x"]);
    c.y = number(&value["y"]);
    c
}

fn edge(value: &Value) -> CEdge {
    CEdge {
        src: value["src"].as_u64().unwrap() as usize,
        dst: value["dst"].as_u64().unwrap() as usize,
        minlen: value["minlen"].as_u64().unwrap() as usize,
        label_w: number(&value["labelW"]),
        label_h: number(&value["labelH"]),
        route: value["route"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect(),
        label_x: number(&value["labelX"]),
        label_y: number(&value["labelY"]),
    }
}

fn close(actual: f64, expected: &Value, context: &str) {
    let expected = number(expected);
    assert!(
        (actual - expected).abs() < TOLERANCE || (actual.is_nan() && expected.is_nan()),
        "{context}: {actual} != {expected}"
    );
}

fn check(nodes: &[CNode], edges: &[CEdge], expected: &Value, context: &str) {
    let expected_nodes = expected["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), expected_nodes.len(), "{context}: node count");
    for (i, (c, want)) in nodes.iter().zip(expected_nodes).enumerate() {
        let at = format!("{context}, node {i}");
        close(c.x, &want["x"], &format!("{at} x"));
        close(c.y, &want["y"], &format!("{at} y"));
        close(c.w, &want["w"], &format!("{at} w"));
        close(c.h, &want["h"], &format!("{at} h"));
        assert_eq!(c.parent, index(&want["parent"]), "{at} parent");
        assert_eq!(c.seq as u64, want["seq"].as_u64().unwrap(), "{at} seq");
        assert_eq!(c.dir, want["dir"].as_str().map(direction), "{at} dir");
    }
    let expected_edges = expected["edges"].as_array().unwrap();
    assert_eq!(edges.len(), expected_edges.len(), "{context}: edge count");
    for (i, (e, want)) in edges.iter().zip(expected_edges).enumerate() {
        let at = format!("{context}, edge {i}");
        assert_eq!(e.src as u64, want["src"].as_u64().unwrap(), "{at} src");
        assert_eq!(e.dst as u64, want["dst"].as_u64().unwrap(), "{at} dst");
        close(e.label_x, &want["labelX"], &format!("{at} label x"));
        close(e.label_y, &want["labelY"], &format!("{at} label y"));
        let route = want["route"].as_array().unwrap();
        assert_eq!(
            e.route.len(),
            route.len(),
            "{at} route length: {:?} vs {route:?}",
            e.route
        );
        for (k, (&actual, want)) in e.route.iter().zip(route).enumerate() {
            close(actual, want, &format!("{at} route[{k}]"));
        }
    }
}

#[test]
fn seeded_compound_layouts_match_pinned_pele_geometry() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/compound-reference.json")).unwrap();
    assert_eq!(corpus["upstreamRevision"], pele::UPSTREAM_REVISION);
    assert_eq!(corpus["seed"], 987654321);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 160);
    let mut wrapped = 0;
    for (k, case) in cases.iter().enumerate() {
        let context = format!("case {k}");
        let mut nodes: Vec<CNode> = case["nodes"].as_array().unwrap().iter().map(node).collect();
        let mut edges: Vec<CEdge> = case["edges"].as_array().unwrap().iter().map(edge).collect();
        let opt = &case["opt"];
        let options = LayeredOptions {
            node_sep: number(&opt["nodeSep"]),
            edge_sep: number(&opt["edgeSep"]),
            rank_sep: number(&opt["rankSep"]),
            port_sep: number(&opt["portSep"]),
            tracks: opt["tracks"].as_bool().unwrap(),
            straight: opt["straight"].as_bool().unwrap(),
            head_room: number(&opt["headRoom"]),
            ..LayeredOptions::default()
        };
        let root: Dir = direction(case["rootDir"].as_str().unwrap());
        if case["inherit"].as_bool().unwrap() {
            let ends: Vec<_> = edges.iter().map(|e| (e.src, e.dst)).collect();
            inherit_crossed_directions(&mut nodes, ends);
        }
        let result = compound_layout(&mut nodes, &mut edges, root, &options);
        let expected = &case["expected"];
        close(
            result.width,
            &expected["result"]["width"],
            &format!("{context} width"),
        );
        close(
            result.height,
            &expected["result"]["height"],
            &format!("{context} height"),
        );
        check(&nodes, &edges, expected, &context);
        let wrap = &case["wrap"];
        if wrap.is_object() {
            let outcome = wrap_components(
                &mut nodes,
                &mut edges,
                number(&wrap["width"]),
                number(&wrap["gap"]),
            );
            let context = format!("{context} wrap");
            match (&outcome, &wrap["result"]) {
                (None, Value::Null) => {}
                (Some(size), want) if want.is_object() => {
                    wrapped += 1;
                    close(size.width, &want["width"], &format!("{context} width"));
                    close(size.height, &want["height"], &format!("{context} height"));
                }
                (actual, want) => panic!("{context}: {actual:?} vs {want}"),
            }
            if outcome.is_some() {
                check(&nodes, &edges, wrap, &context);
            }
        }
    }
    assert!(wrapped > 0, "no case exercised wrapping");
}
