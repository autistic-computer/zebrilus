use std::sync::{Arc, LazyLock};

use pele::{RenderOptions, Theme, render};
use regex::Regex;
use serde_json::Value;

static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^<svg[^>]*>(?:<title[^<]*</title>)?(?:<desc[^<]*</desc>)?<rect width="100%" height="100%" fill="[^"]*"/>"#)
        .unwrap()
});
static TEXT_SIZE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<text[^>]*?) font-size="[^"]*""#).unwrap());

fn body(svg: &str) -> String {
    let inner = PREFIX.replace(svg, "");
    let inner = inner.strip_suffix("</svg>").unwrap_or(&inner);
    TEXT_SIZE.replace_all(inner, "$1").into_owned()
}

fn assert_static(svg: &str, id: &str) {
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|error| panic!("{id}: {error}"));
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        assert!(
            !matches!(node.tag_name().name(), "script" | "foreignObject"),
            "{id}: executable element"
        );
        for attr in node.attributes() {
            let value = attr.value();
            assert!(!attr.name().starts_with("on"), "{id}: event attribute");
            assert!(
                !value.contains("var(") && !value.contains("color-mix("),
                "{id}: unresolved paint in {}",
                attr.name()
            );
            assert!(
                !value.contains("NaN") && !value.contains("Infinity"),
                "{id}: non-finite {}",
                attr.name()
            );
            if attr.name() == "href" {
                let url = value.trim().to_ascii_lowercase();
                assert!(
                    !url.starts_with("javascript:")
                        && !url.starts_with("data:")
                        && !url.starts_with("vbscript:")
                        && !url.starts_with("//"),
                    "{id}: unsafe href {value}"
                );
            }
        }
    }
}

#[test]
fn pinned_upstream_cases_match_native_output() {
    let data: Value = serde_json::from_str(include_str!("fixtures/er.json")).unwrap();
    assert_eq!(data["upstreamRevision"], pele::UPSTREAM_REVISION);
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() >= 70);
    for case in cases {
        let id = case["origin"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let options: RenderOptions = serde_json::from_value(case["options"].clone()).unwrap();
        let result = render(source, &options);
        if let Some(expected) = case.get("error") {
            let error = result.expect_err(id);
            assert_eq!(error.code, expected["code"].as_str().unwrap(), "{id}");
            assert_eq!(
                error.line as u64,
                expected["line"].as_u64().unwrap(),
                "{id}"
            );
            assert_eq!(
                error.column as u64,
                expected["column"].as_u64().unwrap(),
                "{id}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(result.diagram_type, "er", "{id}");
        assert_eq!(result.width, case["width"].as_f64().unwrap(), "{id}");
        assert_eq!(result.height, case["height"].as_f64().unwrap(), "{id}");
        assert_eq!(body(&result.svg), case["body"].as_str().unwrap(), "{id}");
        assert_static(&result.svg, id);
    }
}

#[test]
fn agent_written_diagram_shows_tables_keys_and_crows_feet() {
    let source = "erDiagram\n    USER ||--o{ SESSION : owns\n    SESSION ||--|{ PANE : contains\n    USER {\n        string id PK\n        string email UK\n        datetime created_at\n    }\n    SESSION {\n        string id PK\n        string user_id FK \"owner\"\n    }";
    let result = render(source, &RenderOptions::default()).unwrap();
    assert_static(&result.svg, "agent");
    let doc = roxmltree::Document::parse(&result.svg).unwrap();
    let text: Vec<_> = doc
        .descendants()
        .filter(roxmltree::Node::is_text)
        .filter_map(|node| node.text())
        .collect();
    for label in [
        "USER",
        "SESSION",
        "PANE",
        "owns",
        "contains",
        "email",
        "UK",
        "FK",
        "owner",
        "created_at",
    ] {
        assert!(text.contains(&label), "missing {label}");
    }
    assert_eq!(result.svg.matches("pele-er-identifying").count(), 2);
    assert_eq!(
        result.svg.matches("<circle class=\"pele-marker\"").count(),
        1
    );
    assert_eq!(result.svg.matches("pele-er-divider").count(), 2);
}

#[test]
fn internal_links_only_carry_safe_hrefs() {
    let source = "erDiagram\n  NOTE:::internal-link\n  \"javascript:alert(1)\":::internal-link\n  \" data:text/html,x\":::internal-link\n  \"//evil.example\":::internal-link\n  \"file:///etc/passwd\":::internal-link\n  \"https://example.com\":::internal-link\n  plain";
    let result = render(source, &RenderOptions::default()).unwrap();
    assert_static(&result.svg, "links");
    let hrefs: Vec<_> = result.links.iter().map(|link| link.href.as_str()).collect();
    assert_eq!(hrefs, ["NOTE", "https://example.com"]);
    assert!(result.links.iter().all(|link| link.internal));
    assert_eq!(result.svg.matches(" href=\"").count(), 2);
    assert_eq!(result.svg.matches("class=\"internal-link\"").count(), 6);
}

#[test]
fn counts_and_nesting_are_limited() {
    let options = RenderOptions {
        max_edges: 3,
        ..RenderOptions::default()
    };
    let edges = "erDiagram\n A ||--o{ B : x\n A ||--o{ B : x\n A ||--o{ B : x\n A ||--o{ B : x";
    assert_eq!(render(edges, &options).unwrap_err().code, "limit");
    let fewer = "erDiagram\n A ||--o{ B : x\n A ||--o{ B : x\n A ||--o{ B : x";
    assert!(render(fewer, &options).is_ok());
    let entities = format!(
        "erDiagram\n{}",
        (0..5001).map(|i| format!("E{i}\n")).collect::<String>()
    );
    assert_eq!(
        render(&entities, &RenderOptions::default())
            .unwrap_err()
            .code,
        "limit"
    );
    let nested = format!(
        "erDiagram\n{}A\n{}",
        "subgraph S\n".repeat(200),
        "end\n".repeat(200)
    );
    assert_eq!(
        render(&nested, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
}

#[test]
fn theme_colors_are_written_concretely() {
    let options = RenderOptions {
        theme: Theme {
            background: "#101010".into(),
            foreground: "#eeeeee".into(),
            muted: "#999999".into(),
            line: "#cccccc".into(),
            surface: "#202020".into(),
            surface_alt: "#303030".into(),
            border: "#555555".into(),
            ..Theme::default()
        },
        ..RenderOptions::default()
    };
    let source = "erDiagram\n  subgraph G\n    A ||--o{ A : self\n  end\n  A {\n    int id PK \"c\"\n  }\n  A ||--o{ B : x";
    let result = render(source, &options).unwrap();
    assert_static(&result.svg, "theme");
    for color in [
        "#101010", "#cccccc", "#202020", "#303030", "#555555", "#999999",
    ] {
        assert!(result.svg.contains(color), "missing {color}");
    }
    for default in ["#ffffff", "#f3f3f3", "#e6e6e6", "#8a8a8a", "#6e6e6e"] {
        assert!(!result.svg.contains(default), "default {default} leaked");
    }
}

#[test]
fn host_measurer_sizes_entities() {
    let source = "erDiagram\n  A {\n    string longish_attribute_name\n  }";
    let normal = render(source, &RenderOptions::default()).unwrap();
    let options = RenderOptions {
        measurer: Some(Arc::new(|text: &str, size: f64, _style: u8| {
            text.chars().count() as f64 * size
        })),
        ..RenderOptions::default()
    };
    let wide = render(source, &options).unwrap();
    assert!(wide.width > normal.width);
}
