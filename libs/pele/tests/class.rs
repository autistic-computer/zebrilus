use std::sync::{Arc, LazyLock};

use pele::{RenderOptions, render};
use regex::Regex;
use serde_json::Value;

static TEXT_SIZE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<text[^>]*?) font-size="1[46]""#).unwrap());

const STORYBOOK: &str = "classDiagram
    class AgentTimeline {
        +rows
        +active_turn
        +render()
    }
    class AgentTimelineStore {
        +markdown
        +expanded
        +set_streaming()
    }
    class ListState {
        +scroll_to_end()
    }
    AgentTimeline --> AgentTimelineStore : reads
    AgentTimeline --> ListState : scrolls";

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/class-reference.json"))
        .expect("valid class reference")
}

fn options(fixture: &Value, max_width: Option<f64>) -> RenderOptions {
    let mut options: RenderOptions =
        serde_json::from_value(fixture["options"].clone()).expect("render options");
    options.max_width = max_width;
    options
}

fn body(svg: &str) -> String {
    let marker = "<rect width=\"100%\" height=\"100%\"";
    let start = svg.find(marker).expect("background rect");
    let start = start + svg[start..].find("/>").expect("closed rect") + 2;
    let inner = svg[start..].strip_suffix("</svg>").expect("closed svg");
    TEXT_SIZE.replace_all(inner, "$1").into_owned()
}

fn fnv(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn assert_static(svg: &str, context: &str) {
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|error| panic!("{context}: {error}"));
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        assert!(
            !matches!(node.tag_name().name(), "script" | "foreignObject"),
            "{context}: executable element"
        );
        for attr in node.attributes() {
            let value = attr.value();
            assert!(
                !attr.name().starts_with("on"),
                "{context}: event attribute {}",
                attr.name()
            );
            if attr.name() != "data-id" {
                assert!(
                    !value.contains("var(") && !value.contains("color-mix("),
                    "{context}: unresolved CSS in {}",
                    attr.name()
                );
            }
            if matches!(
                attr.name(),
                "d" | "x" | "y" | "width" | "height" | "cx" | "cy" | "r" | "transform"
            ) {
                assert!(
                    !value.contains("NaN") && !value.contains("Infinity"),
                    "{context}: non-finite {}",
                    attr.name()
                );
            }
            if attr.name() == "href" {
                let url = value.trim().to_ascii_lowercase();
                assert!(
                    !url.starts_with("javascript:")
                        && !url.starts_with("data:")
                        && !url.starts_with("vbscript:")
                        && !url.starts_with("about:")
                        && !url.starts_with("//"),
                    "{context}: unsafe href {value}"
                );
            }
        }
    }
}

fn text_of(svg: &str) -> String {
    let doc = roxmltree::Document::parse(svg).unwrap();
    doc.descendants()
        .filter(roxmltree::Node::is_text)
        .filter_map(|node| node.text())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn upstream_class_reference_cases_match() {
    let fixture = reference();
    assert_eq!(fixture["upstreamRevision"], pele::UPSTREAM_REVISION);
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 4000);
    let mut failures = Vec::new();
    let mut bodies = 0;
    for (k, case) in cases.iter().enumerate() {
        let source = case["source"].as_str().unwrap();
        let options = options(&fixture, case["max_width"].as_f64());
        let context = format!("case {k} ({}) {source:?}", case["group"]);
        let outcome = render(source, &options);
        if let Some(error) = case["error"].as_array() {
            match outcome {
                Ok(_) => failures.push(format!("{context}: rendered, upstream rejects it")),
                Err(actual) => {
                    let expected = (
                        error[0].as_str().unwrap(),
                        error[1].as_u64().unwrap() as usize,
                        error[2].as_u64().unwrap() as usize,
                    );
                    if (actual.code.as_str(), actual.line, actual.column) != expected {
                        failures.push(format!(
                            "{context}: error {} {}:{} differs from {expected:?}",
                            actual.code, actual.line, actual.column
                        ));
                    }
                }
            }
            continue;
        }
        let result = match outcome {
            Ok(result) => result,
            Err(error) => {
                failures.push(format!("{context}: {} {error}", error.code));
                continue;
            }
        };
        assert_static(&result.svg, &context);
        let width = case["width"].as_f64().unwrap();
        let height = case["height"].as_f64().unwrap();
        if (result.width - width).abs() > 0.011 || (result.height - height).abs() > 0.011 {
            failures.push(format!(
                "{context}: {}x{} differs from {width}x{height}",
                result.width, result.height
            ));
        }
        let links: Vec<(String, String)> = result
            .links
            .iter()
            .map(|link| (link.id.clone(), link.href.clone()))
            .collect();
        let expected: Vec<(String, String)> =
            case["links"].as_array().map_or_else(Vec::new, |list| {
                list.iter()
                    .map(|pair| {
                        (
                            pair[0].as_str().unwrap().to_owned(),
                            pair[1].as_str().unwrap().to_owned(),
                        )
                    })
                    .collect()
            });
        if links != expected {
            failures.push(format!(
                "{context}: links {links:?} differ from {expected:?}"
            ));
        }
        let markup = body(&result.svg);
        if let Some(full) = case["body"].as_str() {
            bodies += 1;
            if markup != full {
                let at = markup
                    .bytes()
                    .zip(full.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                failures.push(format!(
                    "{context}: body differs at byte {at}: {:?} vs {:?}",
                    &markup[at.saturating_sub(40)..(at + 80).min(markup.len())],
                    &full[at.saturating_sub(40)..(at + 80).min(full.len())]
                ));
            }
        } else if fnv(&markup) != case["hash"].as_str().unwrap() {
            failures.push(format!("{context}: body differs from upstream"));
        }
    }
    assert_eq!(
        bodies, 38,
        "every documentation example keeps its upstream body"
    );
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from upstream:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

#[test]
fn storybook_sample_renders_every_class_member_and_relation() {
    let result = render(STORYBOOK, &RenderOptions::default()).unwrap();
    assert_eq!(
        result,
        render(STORYBOOK, &RenderOptions::default()).unwrap()
    );
    assert_eq!(result.diagram_type, "class");
    assert_eq!(result.mermaid_type, "classDiagram");
    assert_static(&result.svg, "storybook");
    let text = text_of(&result.svg);
    for label in [
        "AgentTimeline",
        "AgentTimelineStore",
        "ListState",
        "+rows",
        "+active_turn",
        "+render()",
        "+markdown",
        "+expanded",
        "+set_streaming()",
        "+scroll_to_end()",
        "reads",
        "scrolls",
    ] {
        assert!(text.contains(label), "missing {label:?} in {text:?}");
    }
    assert_eq!(
        result.svg.matches("class=\"pele-node pele-class\"").count(),
        3
    );
    assert_eq!(
        result
            .svg
            .matches("class=\"pele-edge pele-relation\"")
            .count(),
        2
    );
    assert_eq!(result.svg.matches("class=\"pele-marker\"").count(), 2);
}

#[test]
fn links_keep_safe_targets_and_callbacks_run_nothing() {
    let options = RenderOptions::default();
    let safe = render(
        "classDiagram\nclass A\nclass B\nlink A \"https://Example.com\" \"tip\" _self\nclick B href \"note.md\"",
        &options,
    )
    .unwrap();
    assert_static(&safe.svg, "safe link");
    assert!(safe.svg.contains(
        "<a href=\"https://example.com/\" target=\"_self\" rel=\"noopener\"><title>tip</title>"
    ));
    assert!(
        safe.svg
            .contains("<a href=\"note.md\" target=\"_blank\" rel=\"noopener\">")
    );
    assert_eq!(safe.links.len(), 2);
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "%6aavascript%3aalert(1)",
        "data:text/html,x",
        "vbscript:x",
        "//evil.example",
        "file:///etc/passwd",
    ] {
        let result = render(
            &format!("classDiagram\nclass A\nlink A \"{url}\""),
            &options,
        )
        .unwrap();
        assert_static(&result.svg, url);
        assert!(result.links.is_empty(), "{url}");
        assert!(!result.svg.contains("<a "), "{url}");
    }
    let callbacks = render(
        "classDiagram\nclass A\nclick A call alert(\"x\") \"tip\"\ncallback A \"steal\"",
        &options,
    )
    .unwrap();
    assert_static(&callbacks.svg, "callbacks");
    assert!(
        callbacks
            .svg
            .contains("class=\"pele-node pele-class clickable clickable\"")
    );
    assert!(callbacks.svg.contains("<title>tip</title>"));
    assert!(!callbacks.svg.contains("alert") && !callbacks.svg.contains("steal"));
    assert!(callbacks.links.is_empty());
}

#[test]
fn nesting_relation_and_member_limits_are_enforced() {
    let deep = format!(
        "classDiagram\n{}class X\n{}",
        "namespace n {\n".repeat(129),
        "}\n".repeat(129)
    );
    assert_eq!(
        render(&deep, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
    let shallow = format!(
        "classDiagram\n{}class X\n{}",
        "namespace n {\n".repeat(128),
        "}\n".repeat(128)
    );
    assert!(render(&shallow, &RenderOptions::default()).is_ok());
    let dotted = format!(
        "classDiagram\nnamespace {}b {{\nclass X\n}}",
        "a.".repeat(200)
    );
    assert_eq!(
        render(&dotted, &RenderOptions::default()).unwrap_err().code,
        "limit"
    );
    let relations = "classDiagram\nA --> B\nB --> C\nC --> D";
    let options = RenderOptions {
        max_edges: 2,
        ..RenderOptions::default()
    };
    assert_eq!(render(relations, &options).unwrap_err().code, "limit");
    let members = format!("classDiagram\nclass A {{\n{}}}", "+m\n".repeat(50_001));
    let options = RenderOptions {
        limit: 1_000_000,
        ..RenderOptions::default()
    };
    assert_eq!(render(&members, &options).unwrap_err().code, "limit");
}

#[test]
fn syntax_errors_keep_upstream_categories_and_positions() {
    let options = RenderOptions::default();
    let error = render("classDiagram\nnote link", &options).unwrap_err();
    assert_eq!(error.code, "syntax");
    assert_eq!((error.line, error.column), (2, 6));
    assert!(error.message.contains("Expecting 'STR', got 'LINK'"));
    let error = render("classDiagram\nclass A\n@", &options).unwrap_err();
    assert_eq!((error.line, error.column), (3, 1));
    assert!(error.message.contains("Lexical error"));
    let error = render("---\ntitle: T\n---\nclassDiagram\nclass A { x", &options).unwrap_err();
    assert!(error.message.contains("EOF_IN_STRUCT"));
    assert_eq!((error.line, error.column), (5, 12));
    let error = render("classDiagram\nnamespace N {\n}", &options).unwrap_err();
    assert!(
        error
            .message
            .contains("Expecting 'NAMESPACE', 'CLASS', 'NOTE_FOR', 'NOTE', got 'STRUCT_STOP'")
    );
}

#[test]
fn theme_colors_and_measurer_are_honored() {
    let source =
        "classDiagram\nnamespace N {\nclass A {\n+x\n}\n}\nA <|-- B : is\nnote for A \"n\"";
    let mut options = RenderOptions::default();
    options.theme.surface = "#102030".into();
    options.theme.surface_alt = "#203040".into();
    options.theme.border = "#304050".into();
    options.theme.line = "#405060".into();
    options.theme.muted = "#506070".into();
    let themed = render(source, &options).unwrap();
    assert_static(&themed.svg, "theme");
    for color in ["#102030", "#203040", "#304050", "#405060", "#506070"] {
        assert!(themed.svg.contains(color), "{color}");
    }
    let normal = render(source, &RenderOptions::default()).unwrap();
    let wide = render(
        source,
        &RenderOptions {
            measurer: Some(Arc::new(|text: &str, size: f64, _style: u8| {
                text.chars().count() as f64 * size * 2.0
            })),
            ..RenderOptions::default()
        },
    )
    .unwrap();
    assert!(wide.width > normal.width);
}
