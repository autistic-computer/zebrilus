use pele::{RenderOptions, render};
use regex::Regex;
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/state.json")).expect("valid state fixtures")
}

fn body(svg: &str) -> String {
    let root = Regex::new(
        r#"^<svg[^>]*>(<title[^<]*</title>)?(<desc[^<]*</desc>)?<rect width="100%" height="100%" fill="[^"]*"/>"#,
    )
    .unwrap();
    let size = Regex::new(r#"(<text[^>]*?) font-size="[^"]*""#).unwrap();
    let inner = root.replace(svg, "");
    let inner = inner.strip_suffix("</svg>").unwrap_or(&inner);
    size.replace_all(inner, "$1").into_owned()
}

fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((at, _), _)| at);
    let from = a.floor_char_boundary(at.saturating_sub(120));
    let to = |s: &str| s.ceil_char_boundary((at + 160).min(s.len()));
    format!(
        "\n  upstream: {}\n  native:   {}",
        &a[from..to(a)],
        &b[b.floor_char_boundary(from)..to(b)]
    )
}

fn assert_static(svg: &str, id: &str) {
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|error| panic!("{id}: {error}"));
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        assert!(
            !matches!(node.tag_name().name(), "script" | "foreignObject"),
            "{id}: executable element"
        );
        for attr in node.attributes() {
            assert!(!attr.name().starts_with("on"), "{id}: event attribute");
            assert!(
                !attr.value().contains("var(") && !attr.value().contains("color-mix("),
                "{id}: unresolved CSS in {}",
                attr.name()
            );
            if matches!(
                attr.name(),
                "d" | "x" | "y" | "width" | "height" | "r" | "points" | "transform"
            ) {
                assert!(!attr.value().contains("NaN"), "{id}: NaN geometry");
            }
            if attr.name() == "href" {
                let href = attr.value().to_ascii_lowercase();
                assert!(
                    href.starts_with("http:")
                        || href.starts_with("https:")
                        || href.starts_with("mailto:")
                        || href.starts_with('/'),
                    "{id}: unexpected href {href}"
                );
            }
        }
    }
}

#[test]
fn renders_the_same_svg_as_upstream() {
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
                Err(error) => {
                    let got = (error.code.as_str(), error.line, error.column);
                    let want = (
                        expected["code"].as_str().unwrap(),
                        expected["line"].as_u64().unwrap() as usize,
                        expected["column"].as_u64().unwrap() as usize,
                    );
                    if got != want {
                        failures.push(format!("{id}: error {got:?}, upstream {want:?}"));
                    }
                }
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
        assert_static(&result.svg, id);
        let size = (result.width, result.height);
        let want = (
            expected["width"].as_f64().unwrap(),
            expected["height"].as_f64().unwrap(),
        );
        if size != want {
            failures.push(format!("{id}: size {size:?}, upstream {want:?}"));
            continue;
        }
        let native = body(&result.svg);
        let upstream = expected["body"].as_str().unwrap();
        if native != upstream {
            failures.push(format!("{id}:{}", first_difference(upstream, &native)));
            continue;
        }
        if serde_json::to_value(&result.links).unwrap() != expected["links"] {
            failures.push(format!("{id}: links {:?}", result.links));
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
fn agent_loop_draws_its_composite_and_transition_labels() {
    let source = "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Running : start\n    state Running {\n        [*] --> Working\n        Working --> Waiting : tool call\n        Waiting --> Working : result\n    }\n    Running --> Idle : stop\n    Running --> [*]";
    let result = render(source, &RenderOptions::default()).unwrap();
    assert_eq!(result.diagram_type, "state");
    assert_static(&result.svg, "agent-loop");
    assert!(
        result
            .svg
            .contains("class=\"pele-cluster pele-state-composite\" data-id=\"Running\"")
    );
    for id in ["Idle", "Working", "Waiting", "root_start", "Running_start"] {
        assert!(result.svg.contains(&format!("data-id=\"{id}\"")), "{id}");
    }
    for label in [">start<", ">tool call<", ">result<", ">stop<"] {
        assert!(result.svg.contains(label), "{label}");
    }
    assert_eq!(result.svg.matches("class=\"pele-edge ").count(), 7);
}

#[test]
fn links_keep_safe_targets_only() {
    let result = render(
        "stateDiagram-v2\n a --> b\n b --> c\n c --> d\n click a \"https://Example.com\" \"Docs &amp; more\"\n click b href \"javascript:alert(1)\"\n click c \"&#106;avascript:alert(1)\" \"x\"\n click d \"file:///etc/passwd\" \"y\"",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(result.links.len(), 1);
    assert_eq!(result.links[0].id, "a");
    assert_eq!(result.links[0].href, "https://example.com/");
    assert!(result.svg.contains("<title>Docs &amp; more</title>"));
    assert!(result.svg.contains("target=\"_blank\" rel=\"noopener\""));
    assert!(!result.svg.to_ascii_lowercase().contains("javascript"));
    assert!(!result.svg.contains("file:"));
}

#[test]
fn nesting_states_and_transitions_are_bounded() {
    let deep = format!(
        "stateDiagram-v2\n{}a\n{}",
        "state s {\n".repeat(200),
        "}\n".repeat(200)
    );
    assert_eq!(
        render(&deep, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
    let mut ring = String::from("stateDiagram-v2\n");
    for i in 0..400 {
        ring.push_str(&format!(
            "state a{i} {{\nstate a{} {{\n}}\n}}\n",
            (i + 1) % 400
        ));
    }
    assert_eq!(
        render(&ring, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
    let busy = format!("stateDiagram-v2\n{}", "a --> b\n".repeat(6000));
    assert_eq!(
        render(&busy, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
    let wide: String = (0..12_000).map(|i| format!("s{i}\n")).collect();
    let options = RenderOptions {
        limit: 1_000_000,
        ..RenderOptions::default()
    };
    assert_eq!(
        render(&format!("stateDiagram-v2\n{wide}"), &options)
            .unwrap_err()
            .code,
        "limit"
    );
}

#[test]
fn theme_colors_are_written_concretely() {
    let mut options = RenderOptions::default();
    options.theme.surface = "#102030".into();
    options.theme.surface_alt = "#203040".into();
    options.theme.line = "#abcdef".into();
    options.theme.border = "#405060".into();
    let result = render(
        "stateDiagram-v2\n [*] --> A\n state C {\n A --> B\n }\n note right of B : n",
        &options,
    )
    .unwrap();
    for color in ["#102030", "#203040", "#abcdef", "#405060"] {
        assert!(result.svg.contains(color), "{color}");
    }
    assert_static(&result.svg, "theme");
}

#[test]
fn malformed_sources_keep_upstream_error_categories() {
    for (source, code) in [
        ("stateDiagram-v2\n a --> \n", "syntax"),
        ("stateDiagram-v2\n state a b {\n }", "syntax"),
        ("stateDiagram-v2\n state S {\n a --> b\n", "syntax"),
        (
            "stateDiagram-v2\n state C {\n x\n }\n state \"a\" as C:extra\n",
            "semantic",
        ),
    ] {
        assert_eq!(
            render(source, &RenderOptions::default()).unwrap_err().code,
            code,
            "{source}"
        );
    }
}
