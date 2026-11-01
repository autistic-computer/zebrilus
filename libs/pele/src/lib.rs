pub mod class;
pub mod components;
pub mod compound;
pub mod er;
pub mod fit_width;
pub mod flow_shapes;
pub mod flowchart;
pub mod gantt;
pub mod layout;
pub mod pie;
mod preprocess;
pub mod quadrant;
pub mod sequence;
pub mod state;
pub mod svg;
pub mod text;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::sync::{Arc, LazyLock};

pub const UPSTREAM_REVISION: &str = "36e584d59152181927e0d8bf70fddcd2a5f33d6c";
pub const SUPPORTED_DIAGRAMS: &[&str] = &[
    "flowchart",
    "sequence",
    "pie",
    "quadrantChart",
    "class",
    "state",
    "er",
    "gantt",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Theme {
    pub background: String,
    pub foreground: String,
    pub muted: String,
    pub line: String,
    pub surface: String,
    pub surface_alt: String,
    pub border: String,
    pub accent: String,
    pub series: Vec<String>,
    pub radius: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#ffffff".into(),
            foreground: "#1f1f1f".into(),
            muted: "#6e6e6e".into(),
            line: "#1f1f1f".into(),
            surface: "#f3f3f3".into(),
            surface_alt: "#e6e6e6".into(),
            border: "#8a8a8a".into(),
            accent: "#5b7bd5".into(),
            series: [
                "#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2",
                "#9d755d",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            radius: 4.0,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RenderOptions {
    pub font_size: f64,
    pub padding: f64,
    pub font_family: String,
    pub font_family_mono: String,
    pub id_prefix: String,
    pub max_width: Option<f64>,
    pub max_edges: usize,
    pub limit: usize,
    pub output_limit: usize,
    pub responsive: Option<bool>,
    pub auto_direction: bool,
    pub direction_breakpoint: f64,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub config: Value,
    pub theme: Theme,
    pub transparent: bool,
    #[serde(skip)]
    pub measurer: Option<Arc<dyn text::TextMeasurer>>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            padding: 8.0,
            font_family: "sans-serif".into(),
            font_family_mono: "monospace".into(),
            id_prefix: "pele".into(),
            max_width: None,
            max_edges: 5000,
            limit: 50_000,
            output_limit: 4_000_000,
            responsive: None,
            auto_direction: true,
            direction_breakpoint: 640.0,
            title: None,
            acc_title: None,
            acc_descr: None,
            config: Value::Object(Map::new()),
            theme: Theme::default(),
            transparent: false,
            measurer: None,
        }
    }
}

impl RenderOptions {
    pub fn measure(&self, value: &str, size: f64, style: u8) -> f64 {
        self.measurer
            .as_deref()
            .unwrap_or(&text::MetricsMeasurer)
            .width(value, size, style)
    }

    pub fn text_measurer(&self) -> &dyn text::TextMeasurer {
        self.measurer.as_deref().unwrap_or(&text::MetricsMeasurer)
    }

    pub fn label(&self, value: &str, size: f64, max_width: Option<f64>) -> text::Label {
        text::label_with_measurer(
            value,
            size,
            max_width,
            self.measurer.as_deref().unwrap_or(&text::MetricsMeasurer),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkInfo {
    pub id: String,
    pub href: String,
    pub internal: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderResult {
    #[serde(rename = "type")]
    pub diagram_type: String,
    #[serde(rename = "mermaidType")]
    pub mermaid_type: String,
    pub svg: String,
    pub width: f64,
    pub height: f64,
    pub links: Vec<LinkInfo>,
}

impl RenderResult {
    pub fn new(diagram_type: &str, svg: String, width: f64, height: f64) -> Self {
        Self {
            diagram_type: diagram_type.into(),
            mermaid_type: diagram_type.into(),
            svg,
            width,
            height,
            links: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    UnsupportedDiagram,
    Syntax,
    Semantic,
    Limit,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedDiagram => "unsupported-diagram",
            Self::Syntax => "syntax",
            Self::Semantic => "semantic",
            Self::Limit => "limit",
        }
    }
}

impl PartialEq<&str> for ErrorCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeleError {
    pub message: String,
    pub code: ErrorCode,
    #[serde(rename = "type")]
    pub diagram_type: Option<String>,
    #[serde(rename = "mermaidType")]
    pub mermaid_type: Option<String>,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

impl PeleError {
    fn new(message: impl Into<String>, code: ErrorCode) -> Self {
        Self {
            message: message.into(),
            code,
            diagram_type: None,
            mermaid_type: None,
            line: 0,
            column: 0,
            snippet: String::new(),
        }
    }

    pub fn syntax(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            ..Self::new(message, ErrorCode::Syntax)
        }
    }

    pub fn semantic(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::Semantic)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::UnsupportedDiagram)
    }
    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::Limit)
    }
}

impl fmt::Display for PeleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PeleError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub diagram_type: &'static str,
    pub mermaid_type: &'static str,
}

static DETECTORS: LazyLock<Vec<(&str, &str, regex::Regex)>> = LazyLock::new(|| {
    [
        ("flowchart", "flowchart-elk", r"^flowchart-elk"),
        ("mindmap", "mindmap", r"^mindmap"),
        ("architecture", "architecture", r"^architecture"),
        ("agentflow", "agentflow", r"^agentflow-beta\b"),
        (
            "c4",
            "c4",
            r"^(C4Context|C4Container|C4Component|C4Dynamic|C4Deployment)",
        ),
        ("kanban", "kanban", r"^kanban"),
        ("class", "classDiagram", r"^classDiagram"),
        ("er", "er", r"^erDiagram"),
        ("gantt", "gantt", r"^gantt"),
        ("info", "info", r"^info"),
        ("pie", "pie", r"^pie"),
        ("requirement", "requirement", r"^requirement(Diagram)?"),
        ("sequence", "sequence", r"^sequenceDiagram"),
        ("swimlane", "swimlane", r"^swimlane-beta\b"),
        ("flowchart", "flowchart-v2", r"^(graph|flowchart)"),
        ("timeline", "timeline", r"^timeline"),
        ("gitGraph", "gitGraph", r"^gitGraph"),
        ("state", "stateDiagram", r"^stateDiagram"),
        ("journey", "journey", r"^journey"),
        ("quadrantChart", "quadrantChart", r"^quadrantChart"),
        ("sankey", "sankey", r"^sankey(-beta)?"),
        ("packet", "packet", r"^packet(-beta)?"),
        ("xychart", "xychart", r"^xychart(-beta)?"),
        ("block", "block", r"^block(-beta)?"),
        ("eventmodeling", "eventmodeling", r"^eventmodeling"),
        ("treeView", "treeView", r"^treeView-beta"),
        ("radar", "radar", r"^radar-beta"),
        ("ishikawa", "ishikawa", r"(?i)^ishikawa(-beta)?\b"),
        ("treemap", "treemap", r"^treemap"),
        ("railroad", "railroad", r"(?i)^railroad-beta"),
        ("railroad", "railroadEbnf", r"(?i)^railroad-ebnf-beta"),
        ("railroad", "railroadAbnf", r"(?i)^railroad-abnf-beta"),
        ("railroad", "railroadPeg", r"(?i)^railroad-peg-beta"),
        ("venn", "venn", r"^venn-beta"),
        ("wardley", "wardley", r"(?i)^wardley-beta"),
        ("cynefin", "cynefin", r"^cynefin-beta(?:[\s:]|$)"),
        ("usecase", "usecase", r"^usecase-beta(?:\s|$)"),
    ]
    .into_iter()
    .map(|(kind, mermaid, pattern)| {
        (
            kind,
            mermaid,
            regex::Regex::new(pattern).expect("valid detector"),
        )
    })
    .collect()
});

pub fn detect_type(source: &str) -> Result<Option<Detected>, PeleError> {
    let prepared = preprocess::preprocess(source)?;
    Ok(detect_cleaned(&prepared.text))
}

fn detect_cleaned(source: &str) -> Option<Detected> {
    DETECTORS.iter().find_map(|(kind, mermaid, re)| {
        re.is_match(source.trim_start()).then_some(Detected {
            diagram_type: kind,
            mermaid_type: mermaid,
        })
    })
}

pub fn supports(source: &str) -> bool {
    detect_type(source)
        .ok()
        .flatten()
        .is_some_and(|found| SUPPORTED_DIAGRAMS.contains(&found.diagram_type))
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    if source.encode_utf16().count() > options.limit {
        return Err(PeleError::limit(format!(
            "Diagram source is longer than the limit of {} characters.",
            options.limit
        )));
    }
    validate_options(options)?;
    let prepared = preprocess::preprocess(source)?;
    let found = detect_cleaned(&prepared.text)
        .ok_or_else(|| PeleError::unsupported("No diagram type detected for the given text."))?;
    let mut resolved = options.clone();
    if let Value::Object(config) = &prepared.config {
        if !resolved.config.is_object() {
            resolved.config = Value::Object(Map::new());
        }
        for (key, value) in config {
            resolved.config[key] = value.clone();
        }
    }
    if prepared.title.is_some() {
        resolved.title = prepared.title;
    }
    if resolved.responsive.is_none() {
        let section = found.diagram_type;
        resolved.responsive = Some(resolved.config[section]["useMaxWidth"] != false);
    }
    let result = match found.diagram_type {
        "flowchart" => flowchart::render(&prepared.text, &resolved),
        "sequence" => sequence::render(&prepared.text, &resolved),
        "pie" => pie::render(&prepared.text, &resolved),
        "quadrantChart" => quadrant::render(&prepared.text, &resolved),
        "class" => class::render(&prepared.text, &resolved),
        "state" => state::render(&prepared.text, &resolved),
        "er" => er::render(&prepared.text, &resolved),
        "gantt" => gantt::render(&prepared.text, &resolved),
        other => Err(PeleError::unsupported(format!(
            "Diagram type \"{other}\" has not been ported to Rust yet."
        ))),
    };
    let mut result = result.map_err(|mut error| {
        error.diagram_type = Some(found.diagram_type.into());
        error.mermaid_type = Some(found.mermaid_type.into());
        if error.line > 0 {
            let mapped = prepared
                .source_positions
                .get(error.line - 1)
                .and_then(|positions| {
                    positions
                        .get(error.column.saturating_sub(1))
                        .or_else(|| positions.last())
                })
                .copied();
            if let Some((line, column)) = mapped {
                error.line = line;
                error.column = column;
            } else {
                error.line = prepared
                    .source_lines
                    .get(error.line - 1)
                    .copied()
                    .unwrap_or(error.line);
            }
            let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
            if let Some(line) = normalized.lines().nth(error.line - 1) {
                error.snippet = format!(
                    "{line}\n{}^",
                    "-".repeat(error.column.saturating_sub(1).min(4096))
                );
            }
        }
        error
    })?;
    if !result.width.is_finite()
        || !result.height.is_finite()
        || result.width <= 0.0
        || result.height <= 0.0
    {
        return Err(PeleError {
            diagram_type: Some(found.diagram_type.into()),
            mermaid_type: Some(found.mermaid_type.into()),
            ..PeleError::semantic("Diagram produced invalid dimensions.")
        });
    }
    if result.svg.encode_utf16().count() > options.output_limit {
        return Err(PeleError {
            diagram_type: Some(found.diagram_type.into()),
            mermaid_type: Some(found.mermaid_type.into()),
            ..PeleError::limit(format!(
                "Diagram output is longer than the limit of {} characters.",
                options.output_limit
            ))
        });
    }
    result.mermaid_type = found.mermaid_type.into();
    Ok(result)
}

fn validate_options(options: &RenderOptions) -> Result<(), PeleError> {
    if !options.font_size.is_finite()
        || !(1.0..=200.0).contains(&options.font_size)
        || !options.padding.is_finite()
        || !(0.0..=4096.0).contains(&options.padding)
        || options
            .max_width
            .is_some_and(|w| !w.is_finite() || w <= 0.0)
        || !options.direction_breakpoint.is_finite()
        || options.direction_breakpoint <= 0.0
        || !options.theme.radius.is_finite()
        || !(0.0..=4096.0).contains(&options.theme.radius)
        || options.theme.series.is_empty()
    {
        return Err(PeleError::semantic(
            "Render dimensions, font size, palette and radius must be finite and valid.",
        ));
    }
    for color in [
        &options.theme.background,
        &options.theme.foreground,
        &options.theme.muted,
        &options.theme.line,
        &options.theme.surface,
        &options.theme.surface_alt,
        &options.theme.border,
        &options.theme.accent,
    ]
    .into_iter()
    .chain(options.theme.series.iter())
    {
        if color != "none" && color.parse::<svgtypes::Color>().is_err() {
            return Err(PeleError::semantic(format!(
                "Theme paint must be a concrete SVG color: {color}"
            )));
        }
    }
    Ok(())
}
