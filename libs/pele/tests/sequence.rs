use pele::{RenderOptions, render};
use regex::Regex;
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/sequence.json")).expect("valid sequence fixtures")
}

fn matches(pattern: &str, svg: &str, line: impl Fn(&regex::Captures) -> String) -> Vec<String> {
    Regex::new(pattern)
        .unwrap()
        .captures_iter(svg)
        .map(|captures| line(&captures))
        .collect()
}

fn geometry(svg: &str) -> Value {
    let paint = Regex::new(r#" ?(fill|stroke|class|rx|style)="[^"]*""#).unwrap();
    let pair = |captures: &regex::Captures| format!("{} {}", &captures[1], &captures[2]);
    let mut lifelines = matches(
        r#"<path class="pele-lifeline" data-id="([^"]*)" d="([^"]*)""#,
        svg,
        pair,
    );
    let mut destroys = matches(
        r#"<path class="pele-destroy" data-id="([^"]*)" d="([^"]*)""#,
        svg,
        pair,
    );
    let mut actors = matches(
        r#"<g class="pele-node pele-actor[^"]*" data-id="([^"]*)"><(rect|circle) ([^>]*?)/?>"#,
        svg,
        |captures| {
            format!(
                "{} {} {}",
                &captures[1],
                &captures[2],
                paint.replace_all(&captures[3], "").trim()
            )
        },
    );
    let messages = matches(
        r#"<g class="pele-edge pele-message"[^>]*><path d="([^"]*)""#,
        svg,
        |captures| captures[1].to_owned(),
    );
    lifelines.sort();
    destroys.sort();
    actors.sort();
    serde_json::json!({
        "lifelines": lifelines,
        "destroys": destroys,
        "actors": actors,
        "messages": messages,
    })
}

#[test]
fn created_and_destroyed_participants_match_upstream() {
    let data = fixtures();
    assert_eq!(data["upstreamRevision"], pele::UPSTREAM_REVISION);
    let cases = data["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let options: RenderOptions = serde_json::from_value(case["options"].clone()).unwrap();
        let expected = &case["expected"];
        let result = render(case["source"].as_str().unwrap(), &options);
        if expected["ok"] == false {
            match result {
                Ok(_) => failures.push(format!("{id}: rendered, upstream rejected it")),
                Err(error) if error.code != expected["code"].as_str().unwrap() => {
                    failures.push(format!("{id}: {} {}", error.code, error.message));
                }
                Err(_) => {}
            }
            continue;
        }
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                failures.push(format!("{id}: {} {}", error.code, error.message));
                continue;
            }
        };
        let size = (result.width, result.height);
        let want = (
            expected["width"].as_f64().unwrap(),
            expected["height"].as_f64().unwrap(),
        );
        if size != want {
            failures.push(format!("{id}: size {size:?}, upstream {want:?}"));
            continue;
        }
        let native = geometry(&result.svg);
        for part in ["lifelines", "destroys", "actors", "messages"] {
            if native[part] != expected[part] {
                failures.push(format!(
                    "{id}: {part}\n  upstream: {}\n  native:   {}",
                    expected[part], native[part]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from upstream:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn a_destroyed_participant_loses_its_footer_and_ends_in_a_cross() {
    let result = render(
        "sequenceDiagram\n A->>B: hi\n create participant C\n B->>C: hello\n destroy C\n B-xC: bye",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(result.svg.matches("class=\"pele-destroy\"").count(), 1);
    assert_eq!(
        result
            .svg
            .matches("class=\"pele-node pele-actor pele-actor-participant pele-actor-footer\"")
            .count(),
        2
    );
}
