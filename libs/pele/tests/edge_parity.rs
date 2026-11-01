use pele::compound::{CNode, cedge, cnode};
use pele::svg::cluster::{Crossings, cluster_title_x, mark_crossings, title_crossed};
use pele::svg::edges::{EdgePath, Fill, casing, loop_path, marker, over_fills, route_path};
use pele::svg::ink::{ON_DARK, ON_LIGHT};
use serde_json::Value;

fn number(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap(),
        Value::String(text) if text == "Infinity" => f64::INFINITY,
        Value::String(text) if text == "-Infinity" => f64::NEG_INFINITY,
        other => panic!("not a number: {other}"),
    }
}

fn close(actual: f64, expected: &Value, context: &str) {
    let expected = number(expected);
    assert!(
        (actual - expected).abs() < 1e-9,
        "{context}: {actual} != {expected}"
    );
}

fn same_path(actual: &EdgePath, expected: &Value, context: &str) {
    assert_eq!(actual.d, expected["d"].as_str().unwrap(), "{context}: d");
    for (value, key) in [
        (actual.sx, "sx"),
        (actual.sy, "sy"),
        (actual.sdx, "sdx"),
        (actual.sdy, "sdy"),
        (actual.ex, "ex"),
        (actual.ey, "ey"),
        (actual.edx, "edx"),
        (actual.edy, "edy"),
    ] {
        close(value, &expected[key], &format!("{context}: {key}"));
    }
}

fn placed(layout: &Value) -> &Value {
    if layout["wrap"]["result"].is_object() {
        &layout["wrap"]
    } else {
        &layout["expected"]
    }
}

fn group(fixture: &Value, layout: &Value, index: usize) -> CNode {
    let placed = &placed(layout)["nodes"][index];
    let mut c = cnode(number(&placed["w"]), number(&placed["h"]), None);
    c.is_group = fixture["isGroup"].as_bool().unwrap();
    c.x = number(&placed["x"]);
    c.y = number(&placed["y"]);
    c
}

#[test]
fn edge_drawing_matches_pinned_pele_output() {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/edge-reference.json")).unwrap();
    let layouts: Value =
        serde_json::from_str(include_str!("fixtures/compound-reference.json")).unwrap();
    assert_eq!(corpus["upstreamRevision"], pele::UPSTREAM_REVISION);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 80);
    let (mut routes, mut loops, mut over) = (0, 0, 0);
    for case in cases {
        let k = case["case"].as_u64().unwrap() as usize;
        let layout = &layouts["cases"][k];
        let curve = case["curve"].as_str();
        let line = case["line"].as_str().unwrap();
        let fills: Vec<Fill> = case["fills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fill| Fill {
                bounds: std::array::from_fn(|i| number(&fill["box"][i])),
                shape: fill["shape"].as_str().unwrap().to_owned(),
            })
            .collect();
        let mut crossings = Crossings::new();
        for e in placed(layout)["edges"].as_array().unwrap() {
            let route: Vec<f64> = e["route"].as_array().unwrap().iter().map(number).collect();
            mark_crossings(&mut crossings, &route);
        }
        for title in case["titles"].as_array().unwrap() {
            let index = title["index"].as_u64().unwrap() as usize;
            let c = group(&layout["nodes"][index], layout, index);
            let width = number(&title["width"]);
            let x = cluster_title_x(&c, width, &crossings, 20.0);
            close(x, &title["x"], &format!("case {k} title {index}"));
            assert_eq!(
                title_crossed(&c, width, x, &crossings),
                title["crossed"].as_bool().unwrap(),
                "case {k} title {index} crossed"
            );
        }
        for (i, drawn) in case["edges"].as_array().unwrap().iter().enumerate() {
            if drawn.is_null() {
                continue;
            }
            let context = format!("case {k} edge {i}");
            let expected_over = drawn["over"].as_str().map(|ink| {
                ink.replace("var(--pele-fg-on-light,#1f1f1f)", ON_LIGHT)
                    .replace("var(--pele-fg-on-dark,#fff)", ON_DARK)
            });
            let expected_over = expected_over.as_deref();
            if drawn["loop"].as_bool() == Some(true) {
                loops += 1;
                let e = &placed(layout)["edges"][i];
                let src = e["src"].as_u64().unwrap() as usize;
                let c = group(&layout["nodes"][src], layout, src);
                let mut edge = cedge(src, src);
                edge.label_w = number(&layout["edges"][i]["labelW"]);
                edge.label_h = number(&layout["edges"][i]["labelH"]);
                let path = loop_path(
                    &c,
                    c.w * 0.8,
                    c.h * 0.8,
                    drawn["k"].as_u64().unwrap() as usize,
                    drawn["sideways"].as_bool().unwrap(),
                    26.0,
                    pele::svg::edges::marker_trim(drawn["end"].as_str().unwrap()),
                    &mut edge,
                );
                same_path(&path, &drawn["path"], &context);
                close(
                    edge.label_x,
                    &drawn["labelX"],
                    &format!("{context}: label x"),
                );
                close(
                    edge.label_y,
                    &drawn["labelY"],
                    &format!("{context}: label y"),
                );
                let found = over_fills(
                    &path.d,
                    &[path.sx, path.sy, path.ex, path.ey],
                    &fills,
                    line,
                    None,
                );
                assert_eq!(found.as_deref(), expected_over, "{context}: over fills");
                continue;
            }
            routes += 1;
            let route: Vec<f64> = drawn["route"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect();
            let start = drawn["start"].as_str().unwrap();
            let end = drawn["end"].as_str().unwrap();
            let start_trim = pele::svg::edges::marker_trim(start);
            let end_trim = pele::svg::edges::marker_trim(end);
            let reach = drawn["reach"].as_array();
            let path = route_path(
                &route,
                curve,
                start_trim,
                end_trim,
                reach.map(|r| number(&r[0])),
                reach.map(|r| number(&r[1])),
            );
            same_path(&path, &drawn["path"], &context);
            let markers = marker(start, path.sx, path.sy, path.sdx, path.sdy, "C", "S")
                + &marker(end, path.ex, path.ey, path.edx, path.edy, "C", "none");
            assert_eq!(
                markers,
                drawn["markers"].as_str().unwrap(),
                "{context}: markers"
            );
            let found = over_fills(
                &path.d,
                &[path.sx, path.sy, path.ex, path.ey],
                &fills,
                line,
                path.tokens.as_deref(),
            );
            if found.is_some() {
                over += 1;
            }
            assert_eq!(found.as_deref(), expected_over, "{context}: over fills");
            assert_eq!(
                casing(&path.d, "", line, i % 5 == 0),
                drawn["casing"].as_str().unwrap(),
                "{context}: casing"
            );
        }
    }
    assert!(
        routes > 500 && loops > 10 && over > 50,
        "{routes} {loops} {over}"
    );
}
