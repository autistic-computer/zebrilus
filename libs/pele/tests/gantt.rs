use std::sync::{Arc, LazyLock};

use pele::gantt::{self, Unit};
use pele::{RenderOptions, render};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Fixture {
    upstream_revision: String,
    time_zone: String,
    now: f64,
    render_options: Value,
    renders: Vec<Case>,
    schedules: Vec<Schedule>,
    dates: Dates,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    source: String,
    #[serde(default)]
    options: Option<Value>,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    #[serde(default)]
    links: Vec<pele::LinkInfo>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    error: Option<Expected>,
}

#[derive(Deserialize)]
struct Expected {
    code: String,
    line: usize,
    column: usize,
    message: String,
}

#[derive(Deserialize)]
struct Schedule {
    id: String,
    source: String,
    tasks: Vec<Task>,
}

#[derive(Deserialize)]
struct Task {
    id: String,
    start: Option<f64>,
    end: Option<f64>,
    render_end: Option<f64>,
}

type IntervalTicks = (String, String, f64, f64, f64, Option<Vec<f64>>);

#[derive(Deserialize)]
struct Dates {
    format: Vec<(Option<f64>, String, Option<String>)>,
    parse: Vec<(String, String, Option<f64>)>,
    axis: Vec<(String, f64, String)>,
    durations: Vec<(String, Option<f64>, Option<String>)>,
    add: Vec<(f64, String, Option<f64>)>,
    js_dates: Vec<(String, Option<f64>)>,
    auto_ticks: Vec<(f64, f64, f64, Vec<f64>)>,
    interval_ticks: Vec<IntervalTicks>,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let fixture: Fixture = serde_json::from_str(include_str!("fixtures/gantt.json")).unwrap();
    assert_eq!(fixture.upstream_revision, pele::UPSTREAM_REVISION);
    assert_eq!(fixture.time_zone, "UTC");
    assert_eq!(fixture.now, gantt::NOW);
    fixture
});

static TEXT_SIZE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<text[^>]*?) font-size="[^"]*""#).unwrap());

fn options(extra: Option<&Value>) -> RenderOptions {
    let mut value = FIXTURE.render_options.clone();
    if let (Some(Value::Object(extra)), Value::Object(base)) = (extra, &mut value) {
        base.extend(extra.clone());
    }
    serde_json::from_value(value).unwrap()
}

fn body(svg: &str) -> String {
    let start = svg.find('>').unwrap() + 1;
    let mut inner = &svg[start..svg.len() - "</svg>".len()];
    for (open, close) in [("<title", "</title>"), ("<desc", "</desc>")] {
        if inner.starts_with(open) {
            inner = &inner[inner.find(close).unwrap() + close.len()..];
        }
    }
    let inner = inner
        .strip_prefix("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>")
        .unwrap();
    TEXT_SIZE.replace_all(inner, "$1").into_owned()
}

fn assert_static(svg: &str, id: &str) {
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|error| panic!("{id}: {error}"));
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        assert!(
            !matches!(node.tag_name().name(), "script" | "foreignObject"),
            "{id}"
        );
        for attr in node.attributes() {
            assert!(!attr.name().starts_with("on"), "{id}: {}", attr.name());
            assert!(
                !attr.value().contains("var(") && !attr.value().contains("color-mix("),
                "{id}: {}={}",
                attr.name(),
                attr.value()
            );
            if matches!(attr.name(), "d" | "x" | "y" | "width" | "height") {
                assert!(
                    !attr.value().contains("NaN") && !attr.value().contains("Infinity"),
                    "{id}: {}={}",
                    attr.name(),
                    attr.value()
                );
            }
        }
    }
}

#[test]
fn renders_match_the_upstream_oracle() {
    assert!(FIXTURE.renders.len() >= 70);
    for case in &FIXTURE.renders {
        let id = &case.id;
        let result = render(&case.source, &options(case.options.as_ref()));
        if let Some(expected) = &case.error {
            let error = result.expect_err(id);
            assert_eq!(error.code, expected.code.as_str(), "{id}: {error}");
            assert_eq!(error.message, expected.message, "{id}");
            assert_eq!(
                (error.line, error.column),
                (expected.line, expected.column),
                "{id}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(result.diagram_type, "gantt", "{id}");
        assert_eq!(
            (result.width, result.height),
            (case.width, case.height),
            "{id}"
        );
        assert_eq!(body(&result.svg), case.body, "{id}");
        assert_eq!(result.links, case.links, "{id}");
        assert_static(&result.svg, id);
    }
}

#[test]
fn schedules_match_the_upstream_oracle() {
    for schedule in &FIXTURE.schedules {
        let id = &schedule.id;
        let tasks = gantt::parse(&schedule.source)
            .and_then(|model| model.schedule(gantt::NOW))
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(tasks.len(), schedule.tasks.len(), "{id}");
        let finite = |time: Option<f64>| time.filter(|time| !time.is_nan());
        for (task, expected) in tasks.iter().zip(&schedule.tasks) {
            assert_eq!(task.id, expected.id, "{id}");
            assert_eq!(finite(task.start), expected.start, "{id}: {}", task.id);
            assert_eq!(finite(task.end), expected.end, "{id}: {}", task.id);
            assert_eq!(
                finite(task.render_end),
                expected.render_end,
                "{id}: {}",
                task.id
            );
        }
    }
}

fn unit_name(unit: Unit) -> &'static str {
    match unit {
        Unit::Millisecond => "ms",
        Unit::Second => "s",
        Unit::Minute => "m",
        Unit::Hour => "h",
        Unit::Day => "d",
        Unit::Week => "w",
        Unit::Month => "M",
        Unit::Year => "y",
    }
}

#[test]
fn date_parsing_and_formatting_match_the_upstream_oracle() {
    let dates = &FIXTURE.dates;
    for (time, template, expected) in &dates.format {
        let time = time.unwrap_or(f64::NAN);
        assert_eq!(
            &gantt::format_date(time, template),
            expected,
            "{time} {template:?}"
        );
    }
    for (input, template, expected) in &dates.parse {
        assert_eq!(
            &gantt::parse_date(input, template, gantt::NOW),
            expected,
            "{input:?} {template:?}"
        );
    }
    for (text, value, unit) in &dates.durations {
        let parsed = gantt::parse_duration(text);
        assert_eq!(parsed.map(|(value, _)| value), *value, "{text:?}");
        assert_eq!(
            parsed.map(|(_, unit)| unit_name(unit)),
            unit.as_deref(),
            "{text:?}"
        );
    }
    for (time, text, expected) in &dates.add {
        let (amount, unit) = gantt::parse_duration(text).unwrap();
        let result = gantt::add(*time, amount, unit);
        assert_eq!(
            (!result.is_nan()).then_some(result),
            *expected,
            "{time} + {text}"
        );
    }
    for (text, expected) in &dates.js_dates {
        let result = gantt::parse_js_date(text);
        assert_eq!((!result.is_nan()).then_some(result), *expected, "{text:?}");
    }
}

#[test]
fn axis_formats_and_ticks_match_the_upstream_oracle() {
    let dates = &FIXTURE.dates;
    for (specifier, time, expected) in &dates.axis {
        assert_eq!(
            &gantt::time_format(specifier, *time),
            expected,
            "{specifier:?} {time}"
        );
    }
    for (start, stop, count, expected) in &dates.auto_ticks {
        assert_eq!(
            &gantt::auto_ticks(*start, *stop, *count),
            expected,
            "{start}..{stop}"
        );
    }
    for (interval, weekday, start, stop, limit, expected) in &dates.interval_ticks {
        assert_eq!(
            &gantt::interval_ticks(interval, weekday, *start, *stop, *limit),
            expected,
            "{interval:?} {weekday}"
        );
    }
}

#[test]
fn compatibility_doc_cases_render_deterministically() {
    for case in FIXTURE.renders.iter().filter(|case| case.error.is_none()) {
        let options = options(case.options.as_ref());
        let first = render(&case.source, &options).unwrap();
        assert_eq!(
            first,
            render(&case.source, &options).unwrap(),
            "{}",
            case.id
        );
    }
}

#[test]
fn undated_tasks_start_at_the_unix_epoch_without_a_today_marker() {
    let tasks =
        gantt::parse("gantt\n First :1d\n Second :after missing, 2d\n Third :until missing\n")
            .unwrap()
            .schedule(gantt::NOW)
            .unwrap();
    assert_eq!(tasks[0].start, Some(0.0));
    assert_eq!(tasks[0].end, Some(86_400_000.0));
    assert_eq!(tasks[1].start, Some(0.0));
    assert_eq!(tasks[2].end, Some(0.0));
    let result = render(
        "gantt\ndateFormat YYYY-MM-DD\ntodayMarker stroke:#f00\nA :1969-12-30, 5d\n",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(!result.svg.contains("pele-today"));
    let times = render(
        "gantt\ndateFormat HH:mm\naxisFormat %Y-%m-%d %H:%M\nA :17:49, 2m\n",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(times.svg.contains("1970-01-01 17:49"));
}

#[test]
fn theme_values_replace_every_upstream_css_variable() {
    let mut options = RenderOptions::default();
    options.theme.background = "#010101".into();
    options.theme.muted = "#020202".into();
    options.theme.line = "#030303".into();
    options.theme.surface = "#040404".into();
    options.theme.surface_alt = "#050505".into();
    options.theme.border = "#060606".into();
    options.theme.accent = "#070707".into();
    options.theme.series[3] = "#080808".into();
    options.theme.radius = 9.0;
    let result = render(
        "gantt\ndateFormat YYYY-MM-DD\nexcludes weekends\nsection A\nPlain :2024-01-01, 2d\nCritical :crit, 2024-01-02, 2d\nFinished :done, 2024-01-03, 1d\nMarker :vert, 2024-01-04, 1d\nsection B\nLater :after missing, 1d\n",
        &options,
    )
    .unwrap();
    for color in [
        "#010101", "#020202", "#030303", "#040404", "#050505", "#060606", "#070707", "#080808",
    ] {
        assert!(result.svg.contains(color), "missing {color}");
    }
    assert!(result.svg.contains("rx=\"9\""));
    assert_static(&result.svg, "theme");
}

#[test]
fn host_measurer_drives_label_placement() {
    let source = "gantt\ndateFormat YYYY-MM-DD\nsection Work\nShort :2024-01-01, 30d\n";
    let default = render(source, &RenderOptions::default()).unwrap();
    let wide = RenderOptions {
        measurer: Some(Arc::new(|text: &str, size: f64, _: u8| {
            text.chars().count() as f64 * size * 4.0
        })),
        ..RenderOptions::default()
    };
    let measured = render(source, &wide).unwrap();
    let bar_x = |svg: &str| -> f64 {
        let doc = roxmltree::Document::parse(svg).unwrap();
        let bar = doc
            .descendants()
            .find(|node| node.attribute("data-id") == Some("task1"))
            .and_then(|task| task.children().find(|node| node.has_tag_name("rect")))
            .unwrap();
        bar.attribute("x").unwrap().parse().unwrap()
    };
    assert!(bar_x(&measured.svg) > bar_x(&default.svg) + 100.0);
}

#[test]
fn links_keep_safe_targets_and_drop_executable_schemes() {
    let safe = render(
        "gantt\ndateFormat YYYY-MM-DD\nA :a, 2024-01-01, 2d\nB :b, 2024-01-03, 2d\nclick a href \"https://example.com/docs\"\nclick b call run()\n",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(safe.links.len(), 1);
    assert_eq!(safe.links[0].href, "https://example.com/docs");
    assert!(
        safe.svg
            .contains("<a href=\"https://example.com/docs\" rel=\"noopener\">")
    );
    assert_eq!(safe.svg.matches("pele-clickable").count(), 2);
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,bad",
        "vbscript:msgbox(1)",
        "java&#115;cript:alert(1)",
        "//evil.example/x",
    ] {
        let source =
            format!("gantt\ndateFormat YYYY-MM-DD\nA :a, 2024-01-01, 2d\nclick a href \"{url}\"\n");
        let result = render(&source, &RenderOptions::default()).unwrap();
        assert!(result.links.is_empty(), "{url}");
        assert!(!result.svg.contains("<a "), "{url}");
        assert_static(&result.svg, url);
    }
}

#[test]
fn hostile_labels_stay_inert() {
    let result = render(
        "gantt\ndateFormat YYYY-MM-DD\ntitle <script>alert(1)</script>\nsection \" onload=\"x\n<img src=x onerror=alert(1)> :a\" onclick=\"y, 2024-01-01, 2d\n",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_static(&result.svg, "hostile");
    assert!(!result.svg.contains("<script"));
}

#[test]
fn huge_inputs_fail_with_limits_or_finish_quickly() {
    let head = "gantt\ndateFormat YYYY-MM-DD\n";
    let big = RenderOptions {
        limit: usize::MAX,
        output_limit: usize::MAX,
        ..RenderOptions::default()
    };
    let tasks = format!("{head}{}", "T:1d\n".repeat(gantt::MAX_TASKS + 1));
    assert_eq!(render(&tasks, &big).unwrap_err().code, "limit");
    let work = format!(
        "{head}excludes weekends\n{}",
        "T :2024-01-01, 10950d\n".repeat(400)
    );
    assert_eq!(render(&work, &big).unwrap_err().code, "limit");
    for source in [
        format!("{head}tickInterval 1millisecond\nA :1950-01-01, 36500d\n"),
        format!("{head}tickInterval 99999999second\nA :1950-01-01, 36500d\n"),
        format!("{head}tickInterval 100000week\nA :0100-01-01, 99000000d\n"),
        format!("{head}A :0001-01-01, 1d\nB :2024-01-01, 99990000d\n"),
        format!("{head}excludes weekends\nT :2024-01-01, 99999999d\n"),
    ] {
        let result = render(&source, &big).unwrap();
        assert!(result.svg.matches("<text").count() < 64);
        assert_static(&result.svg, &source);
    }
    let every = format!(
        "{head}excludes weekends monday tuesday wednesday thursday friday\nT :2024-01-01, 7d\n"
    );
    assert_eq!(render(&every, &big).unwrap_err().code, "semantic");
    let stamp = format!("gantt\ndateFormat x\nA :{}, 1d\n", "9".repeat(5000));
    assert_eq!(render(&stamp, &big).unwrap_err().code, "semantic");
}

#[test]
fn syntax_errors_point_at_the_original_source() {
    let source = "---\ntitle: Plan\n---\n%% note\ngantt\n  dateFormat YYYY-MM-DD\n  Broken task\n";
    let error = render(source, &RenderOptions::default()).unwrap_err();
    assert_eq!(error.code, "syntax");
    assert_eq!((error.line, error.column), (7, 14));
    assert!(error.snippet.starts_with("  Broken task\n"));
    assert_eq!(error.message, "Expecting 'taskData', got 'NL'");
}
