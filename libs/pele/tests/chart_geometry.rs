use pele::{RenderOptions, render};
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    upstream_revision: String,
    fixtures: Vec<Fixture>,
}

#[derive(Deserialize)]
struct Fixture {
    id: String,
    source: String,
    options: RenderOptions,
    expected: Expected,
}

#[derive(Deserialize)]
struct Expected {
    width: f64,
    height: f64,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    points: Vec<Point>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Point {
    label: String,
    cx: String,
    cy: String,
    r: String,
}

#[test]
fn chart_geometry_matches_pinned_javascript_oracle() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/chart_geometry.json")).unwrap();
    assert_eq!(corpus.upstream_revision, pele::UPSTREAM_REVISION);
    assert_eq!(corpus.fixtures.len(), 162);
    for fixture in corpus.fixtures {
        let rendered = render(&fixture.source, &fixture.options)
            .unwrap_or_else(|error| panic!("{}: {error}", fixture.id));
        assert_eq!(
            (rendered.width, rendered.height),
            (fixture.expected.width, fixture.expected.height),
            "{}",
            fixture.id
        );
        let doc = roxmltree::Document::parse(&rendered.svg).unwrap();
        if rendered.diagram_type == "pie" {
            let paths: Vec<_> = doc
                .descendants()
                .filter(|node| {
                    node.has_tag_name("path") && node.attribute("class") == Some("pele-slice")
                })
                .map(|node| node.attribute("d").unwrap().to_owned())
                .collect();
            assert_eq!(paths, fixture.expected.paths, "{}", fixture.id);
        } else {
            let points: Vec<_> = doc
                .descendants()
                .filter(|node| {
                    node.has_tag_name("circle")
                        && node
                            .attribute("class")
                            .is_some_and(|class| class.starts_with("pele-point"))
                })
                .map(|node| Point {
                    label: node.attribute("data-id").unwrap().to_owned(),
                    cx: node.attribute("cx").unwrap().to_owned(),
                    cy: node.attribute("cy").unwrap().to_owned(),
                    r: node.attribute("r").unwrap().to_owned(),
                })
                .collect();
            assert_eq!(points, fixture.expected.points, "{}", fixture.id);
        }
        assert!(!rendered.svg.contains("var("));
        assert!(!rendered.svg.contains("color-mix("));
    }
}
