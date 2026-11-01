use pele::{PeleError, RenderOptions, render};
use serde_json::Value;

fn corpus() -> Value {
    serde_json::from_str(include_str!("fixtures/compatibility.json"))
        .expect("valid compatibility corpus")
}

fn normalized(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_equivalent(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{context}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: array length");
            for (actual, expected) in a.iter().zip(b) {
                assert_equivalent(actual, expected, context);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: object members");
            for (key, actual) in a {
                assert_equivalent(actual, &b[key], context);
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}

fn assert_static_svg(svg: &str, width: f64, height: f64, id: &str) {
    assert!(width.is_finite() && width > 0.0, "{id}: width {width}");
    assert!(height.is_finite() && height > 0.0, "{id}: height {height}");
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|error| panic!("{id}: {error}"));
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg", "{id}");
    assert_eq!(
        root.tag_name().namespace(),
        Some("http://www.w3.org/2000/svg"),
        "{id}"
    );
    for (name, expected) in [("width", width), ("height", height)] {
        let dimension: f64 = root
            .attribute(name)
            .expect("SVG dimension")
            .parse()
            .expect("numeric dimension");
        assert!(
            (dimension - expected).abs() < 0.011,
            "{id}: {name} differs from result"
        );
    }
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        assert!(
            !matches!(node.tag_name().name(), "script" | "foreignObject"),
            "{id}: executable SVG element"
        );
        for attr in node.attributes() {
            if matches!(
                attr.name(),
                "style"
                    | "fill"
                    | "stroke"
                    | "color"
                    | "font"
                    | "font-family"
                    | "font-size"
                    | "font-weight"
                    | "font-style"
                    | "rx"
                    | "ry"
                    | "x"
                    | "y"
                    | "cx"
                    | "cy"
                    | "r"
                    | "width"
                    | "height"
                    | "d"
                    | "transform"
            ) {
                assert!(
                    !attr.value().contains("var("),
                    "{id}: unresolved CSS variable in {}",
                    attr.name()
                );
                assert!(
                    !attr.value().contains("color-mix("),
                    "{id}: unresolved CSS mix in {}",
                    attr.name()
                );
            }
            assert!(
                !attr.name().starts_with("on"),
                "{id}: event attribute {}",
                attr.name()
            );
            if attr.name() == "href" {
                let url: String = attr
                    .value()
                    .chars()
                    .filter(|ch| !ch.is_whitespace() && !ch.is_control())
                    .collect();
                let url = url.to_ascii_lowercase();
                assert!(
                    !url.starts_with("javascript:")
                        && !url.starts_with("vbscript:")
                        && !url.starts_with("data:"),
                    "{id}: unsafe hyperlink scheme"
                );
            }
            if matches!(
                attr.name(),
                "d" | "x" | "y" | "width" | "height" | "cx" | "cy" | "r" | "transform"
            ) {
                assert!(
                    !attr.value().contains("NaN") && !attr.value().contains("Infinity"),
                    "{id}: non-finite geometry"
                );
            }
        }
    }
}

#[test]
fn oracle_corpus_is_pinned_to_the_port_revision() {
    let data = corpus();
    assert_eq!(data["upstream"]["revision"], pele::UPSTREAM_REVISION);
    let cases = data["fixtures"].as_array().unwrap();
    assert!(
        cases
            .iter()
            .any(|case| case["oracle"]["type"] == "flowchart")
    );
    assert!(
        cases
            .iter()
            .any(|case| case["oracle"]["type"] == "sequence")
    );
    assert!(cases.iter().any(|case| case["oracle"]["type"] == "pie"));
    assert!(
        cases
            .iter()
            .any(|case| case["oracle"]["type"] == "quadrantChart")
    );
    assert!(
        cases
            .iter()
            .any(|case| case["native_expectation"] == "unsupported")
    );
}

#[test]
fn status_inventory_matches_the_public_supported_types() {
    let status: Value = serde_json::from_str(include_str!("../PORT_STATUS.json")).unwrap();
    assert_eq!(status["upstream"]["revision"], pele::UPSTREAM_REVISION);
    let implemented: Vec<_> = status["implemented"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["type"].as_str().unwrap())
        .collect();
    assert_eq!(implemented, pele::SUPPORTED_DIAGRAMS);
    let mut all = implemented;
    all.extend(
        status["not_ported"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry.as_str().unwrap()),
    );
    let count = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(
        all.len(),
        count,
        "duplicate diagram types in port inventory"
    );
    assert_eq!(
        count as u64,
        status["upstream"]["registered_diagram_types"]
            .as_u64()
            .unwrap()
    );
}

#[test]
fn supported_fixtures_preserve_labels_in_deterministic_static_svg() {
    let data = corpus();
    let options: RenderOptions =
        serde_json::from_value(data["upstream"]["render_options"].clone()).unwrap();
    for case in data["fixtures"].as_array().unwrap() {
        if case["native_expectation"] != "render" {
            continue;
        }
        let id = case["id"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let result = render(source, &options).unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(
            result,
            render(source, &options).unwrap(),
            "{id}: nondeterministic rendering"
        );
        assert_eq!(
            result.diagram_type,
            case["oracle"]["type"].as_str().unwrap(),
            "{id}"
        );
        assert_eq!(
            result.mermaid_type,
            case["oracle"]["mermaid_type"].as_str().unwrap(),
            "{id}"
        );
        assert_static_svg(&result.svg, result.width, result.height, id);
        let doc = roxmltree::Document::parse(&result.svg).unwrap();
        let text = normalized(
            &doc.descendants()
                .filter(roxmltree::Node::is_text)
                .filter_map(|node| node.text())
                .collect::<Vec<_>>()
                .join(" "),
        );
        for label in case["labels"].as_array().unwrap() {
            let expected = normalized(label.as_str().unwrap());
            assert!(
                text.contains(&expected),
                "{id}: missing visible label {expected:?}; SVG text was {text:?}"
            );
        }
    }
}

#[test]
fn unsupported_diagram_fixtures_return_explicit_errors() {
    let data = corpus();
    for case in data["fixtures"].as_array().unwrap() {
        if case["native_expectation"] != "unsupported" {
            continue;
        }
        let error = render(case["source"].as_str().unwrap(), &RenderOptions::default())
            .expect_err("unported diagrams cannot claim rendering support");
        assert_eq!(error.code, "unsupported-diagram", "{}: {error}", case["id"]);
    }
}

#[test]
fn malformed_diagrams_keep_upstream_error_categories() {
    let data = corpus();
    for case in data["fixtures"].as_array().unwrap() {
        if case["native_expectation"] != "error" {
            continue;
        }
        let error = render(case["source"].as_str().unwrap(), &RenderOptions::default())
            .expect_err("malformed fixture must fail");
        assert_eq!(
            error.code,
            case["oracle"]["error"]["code"].as_str().unwrap(),
            "{}: {error}",
            case["id"]
        );
    }
}

#[test]
fn source_output_and_cartesian_edge_limits_are_enforced() {
    let source = "flowchart LR\n A & B --> C & D";
    let mut options = RenderOptions {
        limit: 4,
        ..RenderOptions::default()
    };
    assert_eq!(render(source, &options).unwrap_err().code, "limit");
    options.limit = 50_000;
    options.output_limit = 20;
    assert_eq!(render(source, &options).unwrap_err().code, "limit");
    options.output_limit = 4_000_000;
    options.max_edges = 3;
    assert_eq!(render(source, &options).unwrap_err().code, "limit");
}

#[test]
fn non_bmp_source_limits_count_utf16_code_units() {
    let source = "pie\n \"😀\": 1";
    let options = RenderOptions {
        limit: source.chars().count(),
        ..RenderOptions::default()
    };
    assert_eq!(render(source, &options).unwrap_err().code, "limit");
}

#[test]
fn untrusted_labels_cannot_create_xml_elements_or_events() {
    for source in [
        "flowchart LR\n A[\"<script>alert(1)</script>\"] --> B[\"a & b\"]",
        "pie\n \"<script>alert(1)</script>\": 1\n \"&quot; onload=&quot;alert(1)\": 2",
        "sequenceDiagram\n participant A as <script>alert(1)</script>\n A->>B: &quot; onload=&quot;alert(1)",
    ] {
        let result = render(source, &RenderOptions::default())
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_static_svg(&result.svg, result.width, result.height, "untrusted-label");
    }
}

#[test]
fn theme_values_are_materialized_before_rasterization() {
    let mut options = RenderOptions::default();
    options.theme.foreground = "#ffeedd".into();
    options.theme.surface = "#102030".into();
    options.theme.line = "#abcdef".into();
    let result = render("flowchart LR\n A[First] --> B[Second]", &options).unwrap();
    assert!(result.svg.contains("#ffeedd"));
    assert!(result.svg.contains("#102030"));
    assert!(result.svg.contains("#abcdef"));
    assert_static_svg(&result.svg, result.width, result.height, "theme");
}

#[test]
fn css_looking_labels_remain_valid_text_in_static_svg() {
    for source in [
        "flowchart LR\n A[\"var(--example) and color-mix(in srgb,red,blue)\"]",
        "pie\n \"var(--example)\": 1",
    ] {
        let result = render(source, &RenderOptions::default()).unwrap();
        assert_static_svg(
            &result.svg,
            result.width,
            result.height,
            "css-looking-label",
        );
        assert!(result.svg.contains("var(--example)"));
    }
}

#[test]
fn diagram_hyperlinks_preserve_safe_urls_without_executable_schemes() {
    let safe = render(
        "flowchart LR\n A[Docs]\n click A \"https://example.com/docs\"",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_static_svg(&safe.svg, safe.width, safe.height, "safe-link");
    let doc = roxmltree::Document::parse(&safe.svg).unwrap();
    assert!(
        doc.descendants()
            .flat_map(|node| node.attributes())
            .any(|attr| attr.name() == "href" && attr.value() == "https://example.com/docs")
    );
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,bad",
        "vbscript:msgbox(1)",
    ] {
        let source = format!("flowchart LR\n A[Docs]\n click A \"{url}\"");
        if let Ok(result) = render(&source, &RenderOptions::default()) {
            assert_static_svg(&result.svg, result.width, result.height, "unsafe-link");
        }
    }
}

#[test]
fn native_output_rasterizes_with_resvg() {
    let data = corpus();
    let options: RenderOptions =
        serde_json::from_value(data["upstream"]["render_options"].clone()).unwrap();
    let mut svg_options = resvg::usvg::Options::default();
    svg_options.fontdb_mut().load_system_fonts();
    for case in data["fixtures"].as_array().unwrap() {
        if case["native_expectation"] != "render" {
            continue;
        }
        let id = case["id"].as_str().unwrap();
        let result = render(case["source"].as_str().unwrap(), &options).unwrap();
        let tree = resvg::usvg::Tree::from_str(&result.svg, &svg_options)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        let scale = (256.0 / tree.size().width()).min(256.0 / tree.size().height());
        let mut pixmap = resvg::tiny_skia::Pixmap::new(256, 256).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let foreground = pixmap
            .pixels()
            .iter()
            .filter(|pixel| {
                pixel.alpha() > 0
                    && (pixel.red() < 250 || pixel.green() < 250 || pixel.blue() < 250)
            })
            .count();
        assert!(
            foreground > 10,
            "{id}: raster contains no visible diagram content beyond its background"
        );
        if !case["labels"].as_array().unwrap().is_empty() {
            let doc = roxmltree::Document::parse(&result.svg).unwrap();
            assert!(
                doc.descendants().any(|node| node.is_element()
                    && node.tag_name().name() == "text"
                    && node
                        .descendants()
                        .filter_map(|child| child.text())
                        .any(|text| !text.trim().is_empty())),
                "{id}: missing visible text nodes"
            );
        }
    }
}

#[test]
fn errors_roundtrip_through_the_public_json_contract() {
    let error = render("pie\n \"A\":", &RenderOptions::default()).unwrap_err();
    let value = serde_json::to_value(&error).unwrap();
    assert_eq!(value["code"], "syntax");
    assert!(value["message"].is_string());
    assert!(value["line"].is_number());
    assert!(value["column"].is_number());
    assert_eq!(serde_json::from_value::<PeleError>(value).unwrap(), error);
}

#[test]
fn parsed_pie_quadrant_and_sequence_models_match_the_pinned_oracle() {
    let data = corpus();
    for case in data["fixtures"].as_array().unwrap() {
        if case["native_expectation"] != "render" {
            continue;
        }
        let source = case["source"].as_str().unwrap();
        let id = case["id"].as_str().unwrap();
        let oracle = &case["oracle"]["model"];
        match case["oracle"]["type"].as_str().unwrap() {
            "pie" if source.starts_with("pie") => {
                let model = pele::pie::parse(source).unwrap();
                assert_equivalent(
                    &serde_json::to_value(model.sections).unwrap(),
                    &oracle["sections"],
                    &format!("{id}: pie sections"),
                );
                assert_eq!(
                    model.show_data,
                    oracle["show_data"].as_bool().unwrap(),
                    "{id}: showData"
                );
            }
            "quadrantChart" if source.starts_with("quadrantChart") => {
                let model = pele::quadrant::parse(source).unwrap();
                let points: Vec<_> = model.points.iter().map(|point| serde_json::json!({"label":point.text.text,"x":point.x,"y":point.y})).collect();
                assert_equivalent(
                    &serde_json::to_value(points).unwrap(),
                    &oracle["points"],
                    &format!("{id}: quadrant points"),
                );
            }
            "sequence" if source.starts_with("sequenceDiagram") => {
                let model = pele::sequence::parse(source, 5000).unwrap();
                let actors: Vec<_> = model.actors.iter().map(|actor| serde_json::json!({"id":actor.id,"label":actor.description,"type":actor.kind})).collect();
                assert_eq!(
                    serde_json::to_value(actors).unwrap(),
                    oracle["actors"],
                    "{id}: participants"
                );
            }
            _ => {}
        }
    }
}

fn close(actual: f64, expected: &Value, context: &str) {
    let expected = expected.as_f64().unwrap();
    assert!(
        (actual - expected).abs() < 1e-8,
        "{context}: {actual} != {expected}"
    );
}

#[test]
fn text_metrics_and_styled_wrapping_match_upstream() {
    let data: Value = serde_json::from_str(include_str!("fixtures/text.json")).unwrap();
    assert_eq!(data["upstream_revision"], pele::UPSTREAM_REVISION);
    for item in data["measurements"].as_array().unwrap() {
        let text = item["text"].as_str().unwrap();
        let width = pele::text::measure(
            text,
            item["size"].as_f64().unwrap(),
            item["style"].as_u64().unwrap() as u8,
        );
        close(width, &item["expected"], text);
    }
    for item in data["labels"].as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        let actual = pele::text::layout_label(
            item["raw"].as_str().unwrap(),
            item["markdown"].as_bool().unwrap(),
            &pele::text::MetricsMeasurer,
            item["size"].as_f64().unwrap(),
            item["max_width"].as_f64().unwrap(),
            item["base_style"].as_u64().unwrap() as u8,
        );
        let expected = &item["expected"];
        close(actual.width, &expected["width"], id);
        close(actual.height, &expected["height"], id);
        close(actual.line_height, &expected["line_height"], id);
        assert_eq!(
            actual.lines.len(),
            expected["lines"].as_array().unwrap().len(),
            "{id}: line count"
        );
        for (line_index, line) in actual.lines.iter().enumerate() {
            let expected_line = expected["lines"][line_index].as_array().unwrap();
            assert_eq!(
                line.len(),
                expected_line.len(),
                "{id}: span count on line {line_index}"
            );
            close(
                actual.widths[line_index],
                &expected["widths"][line_index],
                id,
            );
            for (span, expected_span) in line.iter().zip(expected_line) {
                assert_eq!(
                    span.text,
                    expected_span["text"].as_str().unwrap(),
                    "{id}: span text"
                );
                assert_eq!(
                    span.style as u64,
                    expected_span["style"].as_u64().unwrap(),
                    "{id}: span style"
                );
                close(span.width, &expected_span["width"], id);
            }
        }
    }
}

#[test]
fn preprocessing_keeps_errors_on_original_source_lines() {
    let source = "---\ntitle: Broken\n---\n%% ignore\nflowchart LR\n A[broken";
    let error = render(source, &RenderOptions::default()).unwrap_err();
    assert_eq!(error.code, "syntax");
    assert_eq!(error.line, 6);
    assert!(error.snippet.starts_with(" A[broken\n"));
}

#[test]
fn inline_and_multiline_directives_preserve_original_error_columns() {
    let base = "pie \"A\":";
    let base_error = render(base, &RenderOptions::default()).unwrap_err();
    let prefix = "%%{init: {\"theme\":\"base\"}}%%  ";
    let source = format!("{prefix}{base}");
    let error = render(&source, &RenderOptions::default()).unwrap_err();
    assert_eq!(error.line, 1);
    assert_eq!(
        error.column,
        base_error.column + prefix.encode_utf16().count()
    );
    assert!(error.snippet.starts_with(&source));
    let prefix = "%%{init:\n {\"theme\":\"base\"}\n}%%  ";
    let source = format!("{prefix}{base}");
    let error = render(&source, &RenderOptions::default()).unwrap_err();
    assert_eq!(error.line, 3);
    assert_eq!(error.column, base_error.column + "}%%  ".len());
    assert!(error.snippet.starts_with("}%%  pie \"A\":\n"));
}

#[test]
fn cr_only_source_errors_use_utf16_columns_and_original_line_text() {
    let source = "pie\r \"😀\":";
    let error = render(source, &RenderOptions::default()).unwrap_err();
    assert_eq!(error.line, 2);
    assert_eq!(error.column, " \"😀\":".encode_utf16().count() + 1);
    assert!(error.snippet.starts_with(" \"😀\":\n"));
}

#[test]
fn explicit_responsiveness_overrides_source_use_max_width_configuration() {
    let source = "---\nconfig:\n  pie:\n    useMaxWidth: false\n---\npie\n \"A\": 1";
    let mut options = RenderOptions::default();
    let fixed = render(source, &options).unwrap();
    let doc = roxmltree::Document::parse(&fixed.svg).unwrap();
    assert!(
        !doc.root_element()
            .attribute("style")
            .unwrap_or("")
            .contains("max-width")
    );
    options.responsive = Some(true);
    let fluid = render(source, &options).unwrap();
    let doc = roxmltree::Document::parse(&fluid.svg).unwrap();
    assert!(
        doc.root_element()
            .attribute("style")
            .unwrap_or("")
            .contains("max-width")
    );
    options.responsive = Some(false);
    let explicit_fixed = render("pie\n \"A\": 1", &options).unwrap();
    let doc = roxmltree::Document::parse(&explicit_fixed.svg).unwrap();
    assert!(
        !doc.root_element()
            .attribute("style")
            .unwrap_or("")
            .contains("max-width")
    );
}
