use crate::compound::{
    CEdge, CNode, Dir, cnode, compound_layout, inherit_crossed_directions, shift_layout,
};
use crate::fit_width::{runs_across, tighten, title_room, turn_to_fit};
use crate::flow_shapes::{end_side, inset_route};
use crate::layout::LayeredOptions;
use crate::svg::cluster::{
    Crossings, cluster_title_x, mark_crossings, struck_title, title_crossed,
};
use crate::svg::edges::{
    EdgePath, Fill, casings_svg, edge_casing, edge_label_svg, loop_path, marker, marker_trim,
    over_fills, route_path, side_reach,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::theme::{ResolvedStyle, class_names, resolve_style, rounded, sets_fill};
use crate::svg::title::title_label;
use crate::{PeleError, RenderOptions, RenderResult, svg, text};
use std::collections::{HashMap, HashSet};

const MAX_DEPTH: usize = 128;
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;

fn parse_direction(value: &str) -> Option<Dir> {
    match value.trim() {
        "TD" | "TB" => Some(Dir::Tb),
        "BT" => Some(Dir::Bt),
        "LR" => Some(Dir::Lr),
        "RL" => Some(Dir::Rl),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    #[default]
    Rect,
    Rounded,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Ellipse,
    Diamond,
    Hexagon,
    LeanRight,
    LeanLeft,
    TrapBottom,
    TrapTop,
    Asymmetric,
    Text,
    Start,
    End,
    Fork,
    Extended(&'static str),
}

impl Shape {
    fn named(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "rect" | "proc" | "process" | "rectangle" | "squareRect" => Self::Rect,
            "rounded" | "event" | "roundedRect" => Self::Rounded,
            "stadium" | "terminal" | "pill" => Self::Stadium,
            "fr-rect" | "subprocess" | "subproc" | "framed-rectangle" | "subroutine" => {
                Self::Subroutine
            }
            "cyl" | "db" | "database" | "cylinder" => Self::Cylinder,
            "circle" | "circ" => Self::Circle,
            "dbl-circ" | "double-circle" | "doublecircle" => Self::DoubleCircle,
            "ellipse" => Self::Ellipse,
            "diam" | "decision" | "diamond" | "question" => Self::Diamond,
            "hex" | "hexagon" | "prepare" => Self::Hexagon,
            "lean-r" | "lean-right" | "in-out" | "lean_right" => Self::LeanRight,
            "lean-l" | "lean-left" | "out-in" | "lean_left" => Self::LeanLeft,
            "trap-b" | "priority" | "trapezoid-bottom" | "trapezoid" => Self::TrapBottom,
            "trap-t" | "manual" | "trapezoid-top" | "inv-trapezoid" | "inv_trapezoid" => {
                Self::TrapTop
            }
            "odd" | "rect_left_inv_arrow" => Self::Asymmetric,
            "text" => Self::Text,
            "sm-circ" | "start" | "small-circle" | "stateStart" => Self::Start,
            "fr-circ" | "stop" | "framed-circle" | "stateEnd" => Self::End,
            "fork" | "join" | "forkJoin" => Self::Fork,
            _ => Self::Extended(crate::flow_shapes::canonical(value)?),
        })
    }

    fn size(self, width: f64, height: f64) -> (f64, f64, f64) {
        let base_h = height + 24.0;
        match self {
            Self::Rect | Self::Rounded => ((width + 36.0).max(48.0), base_h, 0.0),
            Self::Stadium => (width + base_h * 0.5 + 28.0, base_h, 0.0),
            Self::Subroutine => (width + 52.0, base_h, 0.0),
            Self::Cylinder => {
                let w = (width + 36.0).max(56.0);
                let ry = (w / 14.0).clamp(6.0, 12.0);
                (w, base_h + 2.0 * ry, ry / 2.0)
            }
            Self::Circle | Self::DoubleCircle => {
                let extra = if self == Self::Circle { 16.0 } else { 26.0 };
                let side = width.hypot(height).max(28.0) + extra;
                (side, side, 0.0)
            }
            Self::Ellipse => (width * 1.3 + 36.0, height * 1.2 + 24.0, 0.0),
            Self::Diamond => {
                let a = width + 16.0;
                let b = height + 8.0;
                (a + 1.6 * b, a / 1.6 + b, 0.0)
            }
            Self::Hexagon | Self::LeanRight | Self::LeanLeft | Self::TrapBottom | Self::TrapTop => {
                (width + 36.0 + base_h / 2.0, base_h, 0.0)
            }
            Self::Asymmetric => (width + 36.0 + base_h / 4.0, base_h, 0.0),
            Self::Text => (width + 8.0, height + 8.0, 0.0),
            Self::Start => (14.0, 14.0, 0.0),
            Self::End => (18.0, 18.0, 0.0),
            Self::Fork => (72.0, 8.0, 0.0),
            Self::Extended(name) => {
                crate::flow_shapes::size(name, width, height).expect("canonical extended shape")
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Rounded => "rounded",
            Self::Stadium => "stadium",
            Self::Subroutine => "fr-rect",
            Self::Cylinder => "cyl",
            Self::Circle => "circle",
            Self::DoubleCircle => "dbl-circ",
            Self::Ellipse => "ellipse",
            Self::Diamond => "diam",
            Self::Hexagon => "hex",
            Self::LeanRight => "lean-r",
            Self::LeanLeft => "lean-l",
            Self::TrapBottom => "trap-b",
            Self::TrapTop => "trap-t",
            Self::Asymmetric => "odd",
            Self::Text => "text",
            Self::Start => "sm-circ",
            Self::End => "fr-circ",
            Self::Fork => "fork",
            Self::Extended(name) => name,
        }
    }

    fn filled(self) -> bool {
        match self {
            Self::Text => false,
            Self::Extended(name) => crate::flow_shapes::filled(name),
            _ => true,
        }
    }

    fn has_label(self) -> bool {
        match self {
            Self::Start | Self::End | Self::Fork => false,
            Self::Extended(name) => crate::flow_shapes::has_label(name),
            _ => true,
        }
    }

    fn span(self, w: f64, h: f64, sideways: bool) -> f64 {
        let side = if sideways { h } else { w };
        match self {
            Self::Rect | Self::Rounded | Self::Subroutine | Self::Text => side - 24.0,
            Self::Circle
            | Self::DoubleCircle
            | Self::Ellipse
            | Self::Start
            | Self::End
            | Self::Diamond => side * 0.6,
            Self::Stadium => {
                if sideways {
                    h * 0.6
                } else {
                    w - h
                }
            }
            Self::Cylinder => {
                if sideways {
                    h - 36.0
                } else {
                    w * 0.6
                }
            }
            Self::Hexagon => {
                if sideways {
                    h * 0.5
                } else {
                    w - h / 2.0 - 12.0
                }
            }
            Self::LeanRight | Self::LeanLeft | Self::TrapBottom | Self::TrapTop => {
                if sideways {
                    0.0
                } else {
                    w - h / 2.0 - 24.0
                }
            }
            Self::Asymmetric => {
                if sideways {
                    0.0
                } else {
                    w - 24.0
                }
            }
            Self::Fork => side - 8.0,
            Self::Extended(name) => crate::flow_shapes::span(name, w, h, sideways),
        }
        .max(0.0)
    }

    fn inset(self, w: f64, h: f64, side: usize, t: f64) -> f64 {
        let odd = !side.is_multiple_of(2);
        let bulge = |half: f64, depth: f64| {
            let k = (t.abs() / half).min(1.0);
            depth * (1.0 - (1.0 - k * k).sqrt())
        };
        match self {
            Self::Circle | Self::DoubleCircle | Self::Ellipse | Self::Start | Self::End => {
                if odd {
                    bulge(h / 2.0, w / 2.0)
                } else {
                    bulge(w / 2.0, h / 2.0)
                }
            }
            Self::Stadium => {
                if odd {
                    bulge(h / 2.0, h / 2.0)
                } else {
                    0.0
                }
            }
            Self::Cylinder => {
                if odd {
                    0.0
                } else {
                    bulge(w / 2.0, (w / 14.0).clamp(6.0, 12.0))
                }
            }
            Self::Diamond => {
                if odd {
                    t.abs() * w / h
                } else {
                    t.abs() * h / w
                }
            }
            Self::Hexagon => {
                if odd {
                    t.abs() / 2.0
                } else {
                    0.0
                }
            }
            Self::LeanRight | Self::LeanLeft | Self::TrapBottom | Self::TrapTop => {
                if odd {
                    h / 8.0
                } else {
                    0.0
                }
            }
            Self::Asymmetric => {
                if side == 3 {
                    h / 4.0
                } else {
                    0.0
                }
            }
            Self::Extended(name) => crate::flow_shapes::inset(name, w, h, side, t),
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Marker {
    #[default]
    None,
    Point,
    Circle,
    Cross,
}

impl Marker {
    fn kind(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Point => "arrow_point",
            Self::Circle => "arrow_circle",
            Self::Cross => "arrow_cross",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stroke {
    #[default]
    Normal,
    Dotted,
    Thick,
    Invisible,
}

#[derive(Clone, Debug)]
struct Node {
    id: String,
    label: String,
    shape: Shape,
    styles: Vec<String>,
    classes: Vec<String>,
    link: Option<String>,
    target: Option<String>,
    tooltip: Option<String>,
}

#[derive(Clone, Debug)]
struct Edge {
    id: String,
    tail: usize,
    head: usize,
    label: String,
    stroke: Stroke,
    start: Marker,
    end: Marker,
    minlen: usize,
    styles: Vec<String>,
    classes: Vec<String>,
    curve: Option<String>,
}

#[derive(Clone, Debug)]
struct Group {
    id: String,
    title: String,
    members: Vec<String>,
    direction: Option<Dir>,
    classes: Vec<String>,
    collapsed: bool,
}

#[derive(Clone, Debug)]
struct Frame {
    id: Option<String>,
    title: String,
    list: Vec<String>,
    direction: Option<Dir>,
}

#[derive(Clone, Debug, Default)]
struct Model {
    direction: Dir,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    groups: Vec<Group>,
    classes: HashMap<String, Vec<String>>,
    default_edge_style: Vec<String>,
    default_curve: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
}

#[derive(Clone, Debug)]
struct Statement {
    text: String,
    line: usize,
    column: usize,
}

fn statements(source: &str) -> Result<Vec<Statement>, PeleError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = false;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;
    let mut start_line = 1usize;
    let mut start_column = 1usize;
    let mut has_content = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if !quote && ch == '%' && chars.peek() == Some(&'%') {
            chars.next();
            for ch in chars.by_ref() {
                if ch == '\n' {
                    if depth == 0 && !current.trim().is_empty() {
                        result.push(Statement {
                            text: current.trim().to_owned(),
                            line: start_line,
                            column: start_column,
                        });
                        current.clear();
                        has_content = false;
                    } else if depth > 0 {
                        current.push(ch);
                    }
                    line += 1;
                    column = 1;
                    break;
                }
            }
            continue;
        }
        if !has_content && !ch.is_whitespace() {
            start_line = line;
            start_column = column;
            has_content = true;
        }
        if ch == '"' && !escaped {
            quote = !quote;
        }
        if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth = depth.saturating_sub(1),
                '\n' | ';' if depth == 0 => {
                    if !current.trim().is_empty() {
                        result.push(Statement {
                            text: current.trim().to_owned(),
                            line: start_line,
                            column: start_column,
                        });
                    }
                    current.clear();
                    has_content = false;
                    if ch == '\n' {
                        line += 1;
                        column = 1;
                    } else {
                        column += 1;
                    }
                    escaped = false;
                    continue;
                }
                _ => {}
            }
        }
        current.push(ch);
        escaped = ch == '\\' && !escaped;
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += ch.len_utf16();
        }
    }
    if quote || depth != 0 {
        return Err(PeleError::syntax(
            "Unterminated flowchart label or shape",
            start_line,
            start_column,
        ));
    }
    if !current.trim().is_empty() {
        result.push(Statement {
            text: current.trim().to_owned(),
            line: start_line,
            column: start_column,
        });
    }
    Ok(result)
}

fn keyword<'a>(source: &'a str, word: &str) -> Option<&'a str> {
    source
        .strip_prefix(word)
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        .map(str::trim)
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_owned()
    }
}

fn split_values(source: &str, delimiter: char) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for ch in source.chars() {
        if matches!(ch, '"' | '\'') && !escaped {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            }
        }
        if quote.is_none() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ch if ch == delimiter && depth == 0 && !escaped => {
                    values.push(current.trim().to_owned());
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }
        if !escaped || ch != delimiter {
            current.push(ch);
        } else {
            current.pop();
            current.push(ch);
        }
        escaped = ch == '\\' && !escaped;
    }
    if !current.trim().is_empty() {
        values.push(current.trim().to_owned());
    }
    values
}

fn styles(source: &str) -> Result<Vec<String>, PeleError> {
    let mut result = Vec::new();
    for item in split_values(source, ',') {
        for declaration in item.split(';') {
            let Some((property, value)) = declaration.split_once(':') else {
                continue;
            };
            let property = property.trim().to_lowercase();
            let value = value.trim().replace("!important", "");
            let lower = value.to_lowercase();
            if lower.contains("var(") || lower.contains("calc(") || lower.contains("color-mix(") {
                return Err(PeleError::unsupported(
                    "Dynamic CSS expressions in diagram styles are not yet ported",
                ));
            }
            if lower.contains("url(")
                || lower.contains("javascript:")
                || lower.contains("expression(")
                || value.contains(['<', '>', '{', '}', '\\', '@'])
                || lower.contains("/*")
            {
                continue;
            }
            if matches!(
                property.as_str(),
                "fill"
                    | "fill-opacity"
                    | "stroke"
                    | "stroke-width"
                    | "stroke-dasharray"
                    | "stroke-dashoffset"
                    | "stroke-linecap"
                    | "stroke-linejoin"
                    | "stroke-opacity"
                    | "opacity"
                    | "rx"
                    | "ry"
                    | "color"
                    | "font-size"
                    | "font-family"
                    | "font-weight"
                    | "font-style"
                    | "text-decoration"
                    | "letter-spacing"
                    | "word-spacing"
            ) {
                result.push(format!("{property}:{}", value.trim()));
            }
        }
    }
    Ok(result)
}

fn safe_href(value: &str) -> Option<String> {
    let original = text::decode_entities(value).trim().to_owned();
    if original.is_empty() {
        return None;
    }
    let mut decoded = original.clone();
    for _ in 0..32 {
        if decoded.starts_with("//")
            || decoded.contains('\\')
            || decoded
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '\u{2000}'..='\u{200f}' | '\u{feff}'))
        {
            return None;
        }
        if let Some(colon) = decoded.find(':') {
            let prefix = &decoded[..colon];
            if !prefix.contains(['/', '?', '#'])
                && !matches!(
                    prefix.to_ascii_lowercase().as_str(),
                    "http" | "https" | "mailto" | "tel"
                )
            {
                return None;
            }
        }
        let bytes = decoded.as_bytes();
        let mut unwrapped = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let high = (bytes[i + 1] as char).to_digit(16);
                let low = (bytes[i + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    unwrapped.push((high * 16 + low) as u8);
                    i += 3;
                    continue;
                }
            }
            unwrapped.push(bytes[i]);
            i += 1;
        }
        let unwrapped = text::decode_entities(std::str::from_utf8(&unwrapped).ok()?);
        if unwrapped == decoded {
            return Some(original);
        }
        decoded = unwrapped;
    }
    None
}

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
    line: usize,
    column: usize,
}

type ParsedLink = (String, Stroke, Marker, Marker, usize, Option<String>);

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn space(&mut self) {
        self.pos += self.rest().len() - self.rest().trim_start().len();
    }

    fn error(&self, message: impl Into<String>) -> PeleError {
        let prefix = &self.text[..self.pos];
        let newlines = prefix.chars().filter(|ch| *ch == '\n').count();
        let column = if newlines == 0 {
            self.column + prefix.encode_utf16().count()
        } else {
            prefix
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .encode_utf16()
                .count()
                + 1
        };
        PeleError::syntax(message, self.line + newlines, column)
    }

    fn id(&mut self) -> Result<String, PeleError> {
        self.space();
        let start = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            let rest = self.rest();
            if ch.is_whitespace()
                || "[](){}|&;@<>=\"".contains(ch)
                || rest.starts_with(":::")
                || rest.starts_with("--")
                || rest.starts_with("-.")
                || rest.starts_with("==")
                || rest.starts_with("~~")
            {
                break;
            }
            self.pos += ch.len_utf8();
        }
        if self.pos == start {
            return Err(self.error("Expected a flowchart node identifier"));
        }
        let id = self.text[start..self.pos].to_owned();
        if matches!(id.as_str(), "end" | "subgraph") {
            return Err(self.error("Reserved flowchart keyword used as a node identifier"));
        }
        Ok(id)
    }

    fn delimited(&mut self, open: &str, close: &str) -> Result<String, PeleError> {
        self.pos += open.len();
        let start = self.pos;
        let mut quoted = false;
        let mut escaped = false;
        while !self.rest().is_empty() {
            if !quoted && self.rest().starts_with(close) {
                let value = self.text[start..self.pos].to_owned();
                self.pos += close.len();
                return Ok(unquote(&value));
            }
            let ch = self.rest().chars().next().unwrap();
            if ch == '"' && !escaped {
                quoted = !quoted;
            }
            escaped = ch == '\\' && !escaped;
            self.pos += ch.len_utf8();
        }
        Err(self.error(format!("Expected closing delimiter {close}")))
    }

    fn vertex(
        &mut self,
        model: &mut Model,
        indices: &mut HashMap<String, usize>,
        groups: &HashMap<String, usize>,
    ) -> Result<usize, PeleError> {
        let id = self.id()?;
        let mut label = None;
        let mut shape = None;
        let delimiters = [
            ("(((", ")))", Shape::DoubleCircle),
            ("((", "))", Shape::Circle),
            ("(-", "-)", Shape::Ellipse),
            ("([", "])", Shape::Stadium),
            ("[[", "]]", Shape::Subroutine),
            ("[(", ")]", Shape::Cylinder),
            ("{{", "}}", Shape::Hexagon),
            ("[", "]", Shape::Rect),
            ("(", ")", Shape::Rounded),
            ("{", "}", Shape::Diamond),
            (">", "]", Shape::Asymmetric),
        ];
        if self.rest().starts_with("[/") || self.rest().starts_with("[\\") {
            let right = self.rest().starts_with("[/");
            self.pos += 2;
            let start = self.pos;
            let mut found = None;
            while self.pos < self.text.len() {
                if self.rest().starts_with("/]") || self.rest().starts_with("\\]") {
                    let close_right = self.rest().starts_with("/]");
                    label = Some(unquote(&self.text[start..self.pos]));
                    shape = Some(match (right, close_right) {
                        (true, true) => Shape::LeanRight,
                        (false, false) => Shape::LeanLeft,
                        (true, false) => Shape::TrapBottom,
                        (false, true) => Shape::TrapTop,
                    });
                    self.pos += 2;
                    found = Some(());
                    break;
                }
                self.pos += self.rest().chars().next().unwrap().len_utf8();
            }
            if found.is_none() {
                return Err(self.error("Unterminated sloped flowchart shape"));
            }
        } else {
            for (open, close, kind) in delimiters {
                if self.rest().starts_with(open) {
                    label = Some(self.delimited(open, close)?);
                    shape = Some(kind);
                    break;
                }
            }
        }
        if self.rest().starts_with("@{") {
            let data = self.delimited("@{", "}")?;
            for field in split_values(&data, ',') {
                let Some((name, value)) = field.split_once(':') else {
                    return Err(self.error("Expected shape metadata key:value"));
                };
                let value = unquote(value.trim().trim_matches('\''));
                if let Some(&group) = groups.get(&id) {
                    if name.trim() == "view" {
                        model.groups[group].collapsed = value == "collapsed";
                    }
                    continue;
                }
                match name.trim() {
                    "shape" => {
                        shape = Some(Shape::named(&value).ok_or_else(|| {
                            PeleError::unsupported(format!(
                                "Flowchart shape {value} is not yet ported"
                            ))
                        })?)
                    }
                    "label" => label = Some(value),
                    _ => {
                        return Err(PeleError::unsupported(format!(
                            "Flowchart metadata {} is not yet ported",
                            name.trim()
                        )));
                    }
                }
            }
        }
        let index = if let Some(&index) = indices.get(&id) {
            index
        } else {
            let index = model.nodes.len();
            model.nodes.push(Node {
                label: id.clone(),
                id: id.clone(),
                shape: Shape::Rect,
                styles: Vec::new(),
                classes: Vec::new(),
                link: None,
                target: None,
                tooltip: None,
            });
            indices.insert(id, index);
            index
        };
        let node = &mut model.nodes[index];
        if let Some(label) = label {
            node.label = label;
        }
        if let Some(shape) = shape {
            node.shape = shape;
        }
        if self.rest().starts_with(":::") {
            self.pos += 3;
            let class = self.id()?;
            node.classes.extend(class.split(',').map(str::to_owned));
        }
        Ok(index)
    }

    fn node_set(
        &mut self,
        model: &mut Model,
        indices: &mut HashMap<String, usize>,
        groups: &HashMap<String, usize>,
    ) -> Result<Vec<usize>, PeleError> {
        let mut nodes = vec![self.vertex(model, indices, groups)?];
        loop {
            self.space();
            if !self.rest().starts_with('&') {
                break;
            }
            self.pos += 1;
            nodes.push(self.vertex(model, indices, groups)?);
        }
        Ok(nodes)
    }

    fn link(&mut self) -> Result<ParsedLink, PeleError> {
        self.space();
        let mut id = None;
        if let Some(at) = self.rest().find('@') {
            let before = &self.rest()[..at];
            if !before.is_empty()
                && before.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                && self.rest()[at + 1..].starts_with(['-', '=', '~', '<'])
            {
                id = Some(before.to_owned());
                self.pos += at + 1;
            }
        }
        let mut start = Marker::None;
        if self.rest().starts_with('<') {
            start = Marker::Point;
            self.pos += 1;
        } else if (self.rest().starts_with('o') || self.rest().starts_with('x'))
            && self.rest().get(1..).is_some_and(|s| s.starts_with("--"))
        {
            start = if self.rest().starts_with('o') {
                Marker::Circle
            } else {
                Marker::Cross
            };
            self.pos += 1;
        }
        let begin = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            if !matches!(ch, '-' | '=' | '.' | '~') {
                break;
            }
            self.pos += ch.len_utf8();
        }
        if self.pos == begin {
            return Err(self.error("Expected a flowchart edge"));
        }
        let first = &self.text[begin..self.pos];
        let mut end = Marker::None;
        if let Some(ch) = self.rest().chars().next() {
            end = match ch {
                '>' => Marker::Point,
                'o' => Marker::Circle,
                'x' => Marker::Cross,
                _ => Marker::None,
            };
            if end != Marker::None {
                self.pos += 1;
            }
        }
        let mut source_link = first.to_owned();
        let label;
        if self.rest().trim_start().starts_with('|') {
            self.space();
            label = self.delimited("|", "|")?;
        } else if end == Marker::None && matches!(first, "--" | "-." | "==") {
            self.space();
            let label_start = self.pos;
            let mut quote = false;
            loop {
                if self.rest().is_empty() {
                    return Err(self.error("Expected the end of a labeled edge"));
                }
                if !quote
                    && (self.rest().starts_with("--")
                        || self.rest().starts_with("-.")
                        || self.rest().starts_with(".-")
                        || self.rest().starts_with("=="))
                {
                    label = unquote(&self.text[label_start..self.pos]);
                    let tail_begin = self.pos;
                    while self.rest().starts_with(['-', '.', '=']) {
                        self.pos += 1;
                    }
                    source_link = self.text[tail_begin..self.pos].to_owned();
                    if let Some(ch) = self.rest().chars().next() {
                        end = match ch {
                            '>' => Marker::Point,
                            'o' => Marker::Circle,
                            'x' => Marker::Cross,
                            _ => Marker::None,
                        };
                        if end != Marker::None {
                            self.pos += 1;
                        }
                    }
                    break;
                }
                let ch = self.rest().chars().next().unwrap();
                if ch == '"' {
                    quote = !quote;
                }
                self.pos += ch.len_utf8();
            }
        } else {
            label = String::new();
        }
        if source_link.len() < 2 {
            return Err(self.error("Invalid flowchart edge"));
        }
        let stroke = if source_link.contains('~') {
            Stroke::Invisible
        } else if source_link.contains('=') {
            Stroke::Thick
        } else if source_link.contains('.') {
            Stroke::Dotted
        } else {
            Stroke::Normal
        };
        let count = match stroke {
            Stroke::Dotted => source_link.chars().filter(|ch| *ch == '.').count(),
            Stroke::Thick => source_link
                .chars()
                .filter(|ch| *ch == '=')
                .count()
                .saturating_sub(if end == Marker::None { 2 } else { 1 }),
            _ => source_link
                .chars()
                .filter(|ch| *ch == '-' || *ch == '~')
                .count()
                .saturating_sub(if end == Marker::None { 2 } else { 1 }),
        };
        Ok((label, stroke, start, end, count.clamp(1, 10), id))
    }
}

fn parse(source: &str, max_edges: usize) -> Result<Model, PeleError> {
    let statements = statements(source)?;
    let Some(header) = statements.first() else {
        return Err(PeleError::syntax("Expected a flowchart declaration", 1, 1));
    };
    let header_text = keyword(&header.text, "flowchart")
        .or_else(|| keyword(&header.text, "graph"))
        .ok_or_else(|| {
            PeleError::syntax("Expected flowchart or graph", header.line, header.column)
        })?;
    let direction = if header_text.is_empty() {
        Dir::Tb
    } else {
        parse_direction(header_text).ok_or_else(|| {
            PeleError::syntax("Expected TB, TD, BT, LR or RL", header.line, header.column)
        })?
    };
    let mut model = Model {
        direction,
        ..Model::default()
    };
    let mut indices = HashMap::<String, usize>::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut group_indices = HashMap::<String, usize>::new();
    let mut members = HashSet::<String>::new();
    let mut sub_count = 0usize;
    let mut pair_count = HashMap::<(usize, usize), usize>::new();
    for statement in statements.iter().skip(1) {
        let line = statement.text.as_str();
        if let Some(rest) = keyword(line, "subgraph") {
            if frames.len() >= MAX_DEPTH {
                return Err(PeleError::limit(
                    "Flowchart subgraphs are nested too deeply",
                ));
            }
            let (id, title) = if rest.is_empty() {
                (None, String::new())
            } else if let Some(open) = rest.find('[') {
                if !rest.ends_with(']') {
                    return Err(PeleError::syntax(
                        "Unterminated subgraph title",
                        statement.line,
                        statement.column,
                    ));
                }
                let id = rest[..open].trim();
                (
                    (!id.is_empty()).then(|| id.to_owned()),
                    unquote(&rest[open + 1..rest.len() - 1]),
                )
            } else {
                let title = unquote(rest);
                let (_, plain) = markdown_label(&title);
                let id = (!plain.is_empty() && !plain.chars().any(char::is_whitespace))
                    .then(|| plain.to_owned());
                (id, title)
            };
            frames.push(Frame {
                id,
                title,
                list: Vec::new(),
                direction: None,
            });
            continue;
        }
        if line == "end" {
            let Some(frame) = frames.pop() else {
                return Err(PeleError::syntax(
                    "Unexpected subgraph end",
                    statement.line,
                    statement.column,
                ));
            };
            let id = frame.id.unwrap_or_else(|| format!("subGraph{sub_count}"));
            sub_count += 1;
            let mut seen = HashSet::new();
            let fresh: Vec<String> = frame
                .list
                .into_iter()
                .filter(|member| seen.insert(member.clone()))
                .filter(|member| !members.contains(member) && *member != id)
                .collect();
            members.extend(fresh.iter().cloned());
            if let Some(&index) = group_indices.get(&id) {
                model.groups[index].members.extend(fresh);
            } else {
                group_indices.insert(id.clone(), model.groups.len());
                model.groups.push(Group {
                    id: id.clone(),
                    title: frame.title.trim().to_owned(),
                    members: fresh,
                    direction: frame.direction,
                    classes: Vec::new(),
                    collapsed: false,
                });
            }
            if let Some(parent) = frames.last_mut() {
                parent.list.push(id);
            }
            continue;
        }
        if let Some(rest) = keyword(line, "direction") {
            let direction = parse_direction(rest).ok_or_else(|| {
                PeleError::syntax("Invalid direction", statement.line, statement.column)
            })?;
            if let Some(frame) = frames.last_mut() {
                frame.direction = Some(direction);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("accTitle:") {
            model.acc_title = Some(rest.trim().to_owned());
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescr:") {
            model.acc_descr = Some(rest.trim().to_owned());
            continue;
        }
        if let Some(rest) = keyword(line, "classDef") {
            let Some((names, declarations)) = rest.split_once(char::is_whitespace) else {
                return Err(PeleError::syntax(
                    "Expected class names and styles",
                    statement.line,
                    statement.column,
                ));
            };
            let declarations = styles(declarations.trim())?;
            for name in names.split(',') {
                model
                    .classes
                    .entry(name.to_owned())
                    .or_default()
                    .extend(declarations.clone());
            }
            continue;
        }
        if let Some(rest) = keyword(line, "class") {
            let Some((ids, classes)) = rest.split_once(char::is_whitespace) else {
                return Err(PeleError::syntax(
                    "Expected identifiers and class names",
                    statement.line,
                    statement.column,
                ));
            };
            for id in ids.split(',') {
                let names: Vec<String> = classes.trim().split(',').map(str::to_owned).collect();
                if let Some(&index) = indices.get(id) {
                    model.nodes[index].classes.extend(names.iter().cloned());
                }
                if let Some(&index) = group_indices.get(id) {
                    model.groups[index].classes.extend(names.iter().cloned());
                }
                if let Some(edge) = model.edges.iter_mut().find(|edge| edge.id == id) {
                    edge.classes.extend(names);
                }
            }
            continue;
        }
        if let Some(rest) = keyword(line, "style") {
            let Some((id, declarations)) = rest.split_once(char::is_whitespace) else {
                return Err(PeleError::syntax(
                    "Expected an identifier and styles",
                    statement.line,
                    statement.column,
                ));
            };
            let declarations = styles(declarations.trim())?;
            let index = if let Some(&index) = indices.get(id) {
                index
            } else {
                let index = model.nodes.len();
                model.nodes.push(Node {
                    id: id.to_owned(),
                    label: id.to_owned(),
                    shape: Shape::Rect,
                    styles: Vec::new(),
                    classes: Vec::new(),
                    link: None,
                    target: None,
                    tooltip: None,
                });
                indices.insert(id.to_owned(), index);
                index
            };
            model.nodes[index].styles.extend(declarations);
            continue;
        }
        if let Some(rest) = keyword(line, "linkStyle") {
            let Some((ids, declarations)) = rest.split_once(char::is_whitespace) else {
                return Err(PeleError::syntax(
                    "Expected edge indices and styles",
                    statement.line,
                    statement.column,
                ));
            };
            let mut declarations = declarations.trim();
            let mut interpolate = None;
            if let Some(rest) = keyword(declarations, "interpolate") {
                let (name, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                interpolate = Some(name.to_owned());
                declarations = tail.trim();
            }
            let declarations = if declarations.is_empty() {
                None
            } else {
                Some(styles(declarations)?)
            };
            for id in ids.split(',') {
                if id == "default" {
                    if interpolate.is_some() {
                        model.default_curve.clone_from(&interpolate);
                    }
                    if let Some(declarations) = &declarations {
                        model.default_edge_style.clone_from(declarations);
                    }
                } else {
                    let index = id.parse::<usize>().map_err(|_| {
                        PeleError::syntax(
                            "Expected an edge index",
                            statement.line,
                            statement.column,
                        )
                    })?;
                    let Some(edge) = model.edges.get_mut(index) else {
                        return Err(PeleError::syntax(
                            format!("Edge index {index} is out of bounds"),
                            statement.line,
                            statement.column,
                        ));
                    };
                    if interpolate.is_some() {
                        edge.curve.clone_from(&interpolate);
                    }
                    if let Some(declarations) = &declarations {
                        edge.styles.clone_from(declarations);
                    }
                }
            }
            continue;
        }
        if let Some(rest) = keyword(line, "click") {
            let mut cursor = Cursor {
                text: rest,
                pos: 0,
                line: statement.line,
                column: statement.column + 6,
            };
            let id = cursor.id()?;
            cursor.space();
            if keyword(cursor.rest(), "href").is_some() {
                cursor.pos += 4;
                cursor.space();
            }
            let link = if cursor.rest().starts_with('"') {
                Some(cursor.delimited("\"", "\"")?)
            } else {
                if cursor.rest().is_empty() {
                    return Err(cursor.error("Expected a click callback or URL"));
                }
                if let Some(at) = cursor.rest().find('"') {
                    cursor.pos += at;
                } else {
                    cursor.pos = rest.len();
                }
                None
            };
            cursor.space();
            let tooltip = if cursor.rest().starts_with('"') {
                Some(cursor.delimited("\"", "\"")?)
            } else {
                None
            };
            cursor.space();
            let target = if cursor.rest().is_empty() {
                None
            } else if matches!(cursor.rest(), "_blank" | "_self" | "_parent" | "_top") {
                Some(cursor.rest().to_owned())
            } else {
                return Err(cursor.error("Invalid link target"));
            };
            if let Some(&index) = indices.get(&id) {
                let node = &mut model.nodes[index];
                node.link = link.and_then(|href| safe_href(&href));
                node.target = target;
                node.tooltip = tooltip;
                node.classes.push("clickable".to_owned());
            }
            continue;
        }
        if let Some(at) = line.find("@{") {
            let id = line[..at].trim();
            if let Some(edge_index) = model.edges.iter().position(|edge| edge.id == id) {
                let Some(data) = line[at + 2..].strip_suffix('}') else {
                    return Err(PeleError::syntax(
                        "Unterminated edge metadata",
                        statement.line,
                        statement.column + at + 1,
                    ));
                };
                for field in split_values(data, ',') {
                    let Some((name, value)) = field.split_once(':') else {
                        return Err(PeleError::syntax(
                            "Expected edge metadata key:value",
                            statement.line,
                            statement.column + at + 1,
                        ));
                    };
                    match name.trim() {
                        "animate" | "animation" => {}
                        "curve" => {
                            model.edges[edge_index].curve =
                                Some(unquote(value.trim().trim_matches('\'')));
                        }
                        name => {
                            return Err(PeleError::unsupported(format!(
                                "Flowchart edge metadata {name} is not yet ported"
                            )));
                        }
                    }
                }
                continue;
            }
        }
        let mut cursor = Cursor {
            text: line,
            pos: 0,
            line: statement.line,
            column: statement.column,
        };
        let mut tails = cursor.node_set(&mut model, &mut indices, &group_indices)?;
        let mut mentioned: Vec<usize> = tails.clone();
        loop {
            cursor.space();
            if cursor.rest().is_empty() {
                break;
            }
            let (label, stroke, start, end, minlen, id) = cursor.link()?;
            let heads = cursor.node_set(&mut model, &mut indices, &group_indices)?;
            mentioned.extend(&heads);
            let extra = tails
                .len()
                .checked_mul(heads.len())
                .ok_or_else(|| PeleError::limit("Flowchart edge limit exceeded"))?;
            if extra > max_edges.saturating_sub(model.edges.len()) {
                return Err(PeleError::limit(format!(
                    "Edge limit exceeded. The limit is {max_edges}."
                )));
            }
            for &tail in &tails {
                for &head in &heads {
                    let count = pair_count.entry((tail, head)).or_default();
                    let id = id.clone().unwrap_or_else(|| {
                        format!(
                            "L_{}_{}_{}",
                            model.nodes[tail].id,
                            model.nodes[head].id,
                            if *count == 0 { 0 } else { *count + 1 }
                        )
                    });
                    *count += 1;
                    model.edges.push(Edge {
                        id,
                        tail,
                        head,
                        label: label.clone(),
                        stroke,
                        start,
                        end,
                        minlen,
                        styles: Vec::new(),
                        classes: Vec::new(),
                        curve: None,
                    });
                }
            }
            tails = heads;
        }
        if let Some(frame) = frames.last_mut() {
            frame
                .list
                .extend(mentioned.iter().map(|&index| model.nodes[index].id.clone()));
        }
    }
    if !frames.is_empty() {
        return Err(PeleError::syntax(
            "Expected subgraph end",
            source.lines().count().max(1),
            1,
        ));
    }
    Ok(model)
}

fn shape_svg(
    shape: Shape,
    w: f64,
    h: f64,
    attrs: &str,
    line_attrs: &str,
    radius: f64,
    line_color: &str,
) -> String {
    let line_fill = svg::esc(line_color);
    let n = svg::num;
    let rect = |rx: f64| {
        format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{attrs}/>",
            n(-w / 2.0),
            n(-h / 2.0),
            n(w),
            n(h),
            n(rx)
        )
    };
    let square = || {
        format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
            n(-w / 2.0),
            n(-h / 2.0),
            n(w),
            n(h),
            rounded(attrs, radius)
        )
    };
    let polygon = |points: &[(f64, f64)]| {
        format!(
            "<polygon points=\"{}\"{attrs}/>",
            points
                .iter()
                .map(|(x, y)| format!("{},{}", n(*x), n(*y)))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    match shape {
        Shape::Rect => square(),
        Shape::Rounded => rect(12.0_f64.min(h / 2.0)),
        Shape::Stadium => rect(h / 2.0),
        Shape::Subroutine => {
            let x = w / 2.0 - 8.0;
            format!(
                "{}<path d=\"M{},{}V{}M{},{}V{}\" fill=\"none\"{line_attrs}/>",
                square(),
                n(-x),
                n(-h / 2.0),
                n(h / 2.0),
                n(x),
                n(-h / 2.0),
                n(h / 2.0)
            )
        }
        Shape::Circle => format!("<circle r=\"{}\"{attrs}/>", n(w / 2.0)),
        Shape::DoubleCircle => format!(
            "<circle r=\"{}\"{attrs}/><circle r=\"{}\" fill=\"none\"{line_attrs}/>",
            n(w / 2.0),
            n(w / 2.0 - 5.0)
        ),
        Shape::Ellipse => format!(
            "<ellipse rx=\"{}\" ry=\"{}\"{attrs}/>",
            n(w / 2.0),
            n(h / 2.0)
        ),
        Shape::Diamond => polygon(&[
            (0.0, -h / 2.0),
            (w / 2.0, 0.0),
            (0.0, h / 2.0),
            (-w / 2.0, 0.0),
        ]),
        Shape::Hexagon => {
            let m = h / 4.0;
            polygon(&[
                (-w / 2.0 + m, -h / 2.0),
                (w / 2.0 - m, -h / 2.0),
                (w / 2.0, 0.0),
                (w / 2.0 - m, h / 2.0),
                (-w / 2.0 + m, h / 2.0),
                (-w / 2.0, 0.0),
            ])
        }
        Shape::LeanRight => polygon(&[
            (-w / 2.0 + h / 4.0, -h / 2.0),
            (w / 2.0, -h / 2.0),
            (w / 2.0 - h / 4.0, h / 2.0),
            (-w / 2.0, h / 2.0),
        ]),
        Shape::LeanLeft => polygon(&[
            (-w / 2.0, -h / 2.0),
            (w / 2.0 - h / 4.0, -h / 2.0),
            (w / 2.0, h / 2.0),
            (-w / 2.0 + h / 4.0, h / 2.0),
        ]),
        Shape::TrapBottom => polygon(&[
            (-w / 2.0 + h / 4.0, -h / 2.0),
            (w / 2.0 - h / 4.0, -h / 2.0),
            (w / 2.0, h / 2.0),
            (-w / 2.0, h / 2.0),
        ]),
        Shape::TrapTop => polygon(&[
            (-w / 2.0, -h / 2.0),
            (w / 2.0, -h / 2.0),
            (w / 2.0 - h / 4.0, h / 2.0),
            (-w / 2.0 + h / 4.0, h / 2.0),
        ]),
        Shape::Asymmetric => polygon(&[
            (-w / 2.0, -h / 2.0),
            (w / 2.0, -h / 2.0),
            (w / 2.0, h / 2.0),
            (-w / 2.0, h / 2.0),
            (-w / 2.0 + h / 4.0, 0.0),
        ]),
        Shape::Cylinder => {
            let rx = w / 2.0;
            let ry = (w / 14.0).clamp(6.0, 12.0);
            let top = -h / 2.0 + ry;
            let bottom = h / 2.0 - ry;
            format!(
                "<path d=\"M{},{}A{},{} 0 0 1 {},{}V{}A{},{} 0 0 1 {},{}Z\"{attrs}/><path d=\"M{},{}A{},{} 0 0 0 {},{}\" fill=\"none\"{line_attrs}/>",
                n(-rx),
                n(top),
                n(rx),
                n(ry),
                n(rx),
                n(top),
                n(bottom),
                n(rx),
                n(ry),
                n(-rx),
                n(bottom),
                n(-rx),
                n(top),
                n(rx),
                n(ry),
                n(rx),
                n(top)
            )
        }
        Shape::Text => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"none\"/>",
            n(-w / 2.0),
            n(-h / 2.0),
            n(w),
            n(h)
        ),
        Shape::Start => format!("<circle r=\"{}\"{attrs}/>", n(w / 2.0)),
        Shape::End => format!(
            "<circle r=\"{}\"{attrs}/><circle r=\"{}\" fill=\"{line_fill}\"{line_attrs}/>",
            n(w / 2.0),
            n(w / 2.0 - 4.0)
        ),
        Shape::Fork => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"2\" fill=\"{line_fill}\"{line_attrs}/>",
            n(-w / 2.0),
            n(-h / 2.0),
            n(w),
            n(h)
        ),
        Shape::Extended(name) => crate::flow_shapes::draw_with_line_color(
            name, w, h, attrs, line_attrs, radius, line_color,
        )
        .expect("canonical extended shape"),
    }
}

fn compiled(model: &Model, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for name in names {
        let Some(styles) = model.classes.get(*name) else {
            continue;
        };
        out.extend(styles.iter().map(|style| style.trim().to_owned()));
        out.extend(
            styles
                .iter()
                .filter(|style| style.contains("color"))
                .map(|style| style.replacen("fill", "bgFill", 1).trim().to_owned()),
        );
    }
    out
}

fn markdown_label(raw: &str) -> (bool, &str) {
    if raw.len() >= 2 && raw.starts_with('`') && raw.ends_with('`') {
        (true, &raw[1..raw.len() - 1])
    } else {
        (false, raw)
    }
}

struct GraphNode<'a> {
    id: &'a str,
    label: &'a str,
    shape: Shape,
    is_group: bool,
    parent: Option<&'a str>,
    styles: &'a [String],
    compiled: Vec<String>,
    classes: String,
    dir: Option<Dir>,
    vertex: Option<&'a Node>,
}

struct GraphEdge<'a> {
    edge: &'a Edge,
    start: &'a str,
    end: &'a str,
    style: Option<ResolvedStyle>,
    curve: Option<&'a str>,
}

struct Graph<'a> {
    nodes: Vec<GraphNode<'a>>,
    edges: Vec<GraphEdge<'a>>,
}

fn build_graph<'a>(
    model: &'a Model,
    default_curve: Option<&'a str>,
    inherit_dir: bool,
) -> Graph<'a> {
    let groups = &model.groups;
    let group_of: HashMap<&str, usize> = groups
        .iter()
        .enumerate()
        .map(|(i, group)| (group.id.as_str(), i))
        .collect();
    let mut group_parent = HashMap::<&str, &str>::new();
    for group in groups {
        for child in &group.members {
            if group_of.contains_key(child.as_str()) {
                group_parent.insert(child, &group.id);
            }
        }
    }
    let collapsed = |id: &str| group_of.get(id).is_some_and(|&g| groups[g].collapsed);
    let mut hidden = HashSet::<&str>::new();
    let mut collapsed_into = HashMap::<&str, &str>::new();
    if groups.iter().any(|group| group.collapsed) {
        let mut outermost = HashMap::<&str, Option<&str>>::new();
        for group in groups {
            let mut chain = Vec::new();
            let mut seen = HashSet::new();
            let mut current = Some(group.id.as_str());
            while let Some(id) = current {
                if outermost.contains_key(id) || !seen.insert(id) {
                    break;
                }
                chain.push(id);
                current = group_parent.get(id).copied();
            }
            let mut result = current.and_then(|id| outermost.get(id).copied().flatten());
            for &id in chain.iter().rev() {
                if result.is_none() && collapsed(id) {
                    result = Some(id);
                }
                outermost.insert(id, result);
            }
            let Some(ancestor) = outermost[group.id.as_str()] else {
                continue;
            };
            if group.id != ancestor {
                hidden.insert(&group.id);
                collapsed_into.insert(&group.id, ancestor);
            }
            for child in &group.members {
                if child == ancestor {
                    continue;
                }
                hidden.insert(child);
                collapsed_into.insert(child, ancestor);
            }
        }
    }
    let mut parent_of = HashMap::<&str, &str>::new();
    for group in groups.iter().rev() {
        if hidden.contains(group.id.as_str()) {
            continue;
        }
        for id in &group.members {
            parent_of.insert(id, &group.id);
        }
    }
    let mut nodes = Vec::new();
    let mut by_id = HashMap::<&str, usize>::new();
    for group in groups.iter().rev() {
        if hidden.contains(group.id.as_str()) {
            continue;
        }
        let names: Vec<&str> = group.classes.iter().map(String::as_str).collect();
        by_id.insert(&group.id, nodes.len());
        nodes.push(GraphNode {
            id: &group.id,
            label: &group.title,
            shape: if group.collapsed {
                Shape::Extended("collapsedGroup")
            } else {
                Shape::Rect
            },
            is_group: !group.collapsed,
            parent: parent_of.get(group.id.as_str()).copied(),
            styles: &[],
            compiled: compiled(model, &names),
            classes: group.classes.join(" "),
            dir: group.direction.or(inherit_dir.then_some(model.direction)),
            vertex: None,
        });
    }
    for vertex in &model.nodes {
        if hidden.contains(vertex.id.as_str()) {
            continue;
        }
        let names: Vec<&str> = vertex.classes.iter().map(String::as_str).collect();
        if let Some(&existing) = by_id.get(vertex.id.as_str()) {
            let node = &mut nodes[existing];
            node.styles = &vertex.styles;
            node.compiled = compiled(model, &names);
            node.classes = vertex.classes.join(" ");
            continue;
        }
        let mut all = vec!["default", "node"];
        all.extend(&names);
        nodes.push(GraphNode {
            id: &vertex.id,
            label: &vertex.label,
            shape: vertex.shape,
            is_group: false,
            parent: parent_of.get(vertex.id.as_str()).copied(),
            styles: &vertex.styles,
            compiled: compiled(model, &all),
            classes: vertex.classes.join(" "),
            dir: None,
            vertex: Some(vertex),
        });
    }
    let mut edges = Vec::new();
    for raw in &model.edges {
        let head = |id: &'a str| collapsed_into.get(id).copied().unwrap_or(id);
        let start = head(&model.nodes[raw.tail].id);
        let end = head(&model.nodes[raw.head].id);
        if start == end
            && (collapsed_into.contains_key(model.nodes[raw.tail].id.as_str())
                || collapsed_into.contains_key(model.nodes[raw.head].id.as_str()))
        {
            continue;
        }
        let names: Vec<&str> = raw.classes.iter().map(String::as_str).collect();
        let classes = compiled(model, &names);
        let mut own = model.default_edge_style.clone();
        own.extend(raw.styles.iter().cloned());
        let style = (!own.is_empty() || !classes.is_empty()).then(|| {
            let mut all = classes;
            all.extend(own);
            resolve_style(&all)
        });
        edges.push(GraphEdge {
            edge: raw,
            start,
            end,
            style,
            curve: raw
                .curve
                .as_deref()
                .filter(|curve| !curve.is_empty())
                .or(model.default_curve.as_deref())
                .or(default_curve),
        });
    }
    Graph { nodes, edges }
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let model = parse(source, options.max_edges)?;
    let fitted = |down: bool| tighten(options, |tight| draw(&model, options, down, tight));
    if runs_across(model.direction.as_str()) {
        turn_to_fit(options, fitted)
    } else {
        fitted(false)
    }
}

struct View {
    style: ResolvedStyle,
    label: text::Label,
    shape: Shape,
    w: f64,
    h: f64,
    dy: f64,
}

fn flows_sideways(cnodes: &[CNode], i: usize, root: Dir) -> bool {
    let mut dir = None;
    let mut p = cnodes[i].parent;
    let mut hops = 0;
    while let Some(q) = p {
        if dir.is_some() || hops >= 10_000 {
            break;
        }
        dir = cnodes[q].dir;
        p = cnodes[q].parent;
        hops += 1;
    }
    dir.unwrap_or(root).sideways()
}

fn draw(
    model: &Model,
    options: &RenderOptions,
    turned: bool,
    tight: bool,
) -> Result<RenderResult, PeleError> {
    let n = svg::num;
    let theme = &options.theme;
    let group_pad = if tight { 12.0 } else { GROUP_PAD };
    let flow = &options.config["flowchart"];
    let flow_curve = flow["curve"].as_str();
    let graph = build_graph(model, flow_curve, flow["inheritDir"] == true);
    let size = options.font_size;
    let edge_size = (size * 0.875).round();
    let measurer = options.text_measurer();
    let number_option = |key: &str, fallback: f64| {
        flow[key]
            .as_f64()
            .filter(|value| value.is_finite() && *value > 0.0)
            .map_or(fallback, |value| value.min(2000.0))
    };
    let wrap_width = number_option("wrappingWidth", 200.0);
    let pad = options.padding;

    let index: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id, i))
        .collect();
    let mut views = Vec::with_capacity(graph.nodes.len());
    let mut cnodes = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let style = if node.styles.is_empty() {
            resolve_style(&node.compiled)
        } else {
            let mut all = node.compiled.clone();
            all.extend(node.styles.iter().cloned());
            resolve_style(&all)
        };
        let text_style =
            if style.bold { text::BOLD } else { 0 } | if style.italic { text::ITALIC } else { 0 };
        let (markdown, raw) = markdown_label(node.label);
        let font_size = style.font_size.unwrap_or(size);
        let (view, mut c) = if node.is_group {
            let label = text::layout_label(raw, markdown, measurer, font_size, 4000.0, text_style);
            let mut c = cnode(0.0, 0.0, None);
            c.is_group = true;
            c.dir = node.dir;
            c.pad_x = group_pad;
            c.pad_top = if label.height > 0.0 {
                label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = group_pad;
            c.min_w = label.width + 2.0 * group_pad - 12.0;
            (
                View {
                    style,
                    label,
                    shape: Shape::Rect,
                    w: 0.0,
                    h: 0.0,
                    dy: 0.0,
                },
                c,
            )
        } else {
            let shape = node.shape;
            let label = if shape.has_label() {
                text::layout_label(raw, markdown, measurer, font_size, wrap_width, text_style)
            } else {
                text::Label::default()
            };
            let (w, h, dy) = shape.size(label.width, label.height);
            (
                View {
                    style,
                    label,
                    shape,
                    w,
                    h,
                    dy,
                },
                cnode(w, h, None),
            )
        };
        c.seq = i;
        views.push(view);
        cnodes.push(c);
    }
    for (c, node) in cnodes.iter_mut().zip(&graph.nodes) {
        c.parent = node.parent.and_then(|parent| index.get(parent).copied());
    }
    inherit_crossed_directions(
        &mut cnodes,
        graph
            .edges
            .iter()
            .filter_map(|edge| Some((*index.get(edge.start)?, *index.get(edge.end)?))),
    );
    let root_dir = if turned { Dir::Tb } else { model.direction };
    for (i, view) in views.iter().enumerate() {
        cnodes[i].span = if graph.nodes[i].is_group {
            f64::INFINITY
        } else {
            view.shape
                .span(view.w, view.h, flows_sideways(&cnodes, i, root_dir))
        };
    }

    let mut cedges = Vec::new();
    let mut edge_labels = Vec::new();
    let mut drawn = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(edge.start), index.get(edge.end)) else {
            continue;
        };
        let label =
            text::label_with_measurer(&edge.edge.label, edge_size, Some(wrap_width), measurer);
        if src == dst {
            if flows_sideways(&cnodes, src, root_dir) {
                cnodes[src].h += 2.0
                    * (LOOP
                        + if label.height > 0.0 {
                            label.height + 8.0
                        } else {
                            0.0
                        });
            } else {
                cnodes[src].w += 2.0
                    * (LOOP
                        + if label.width > 0.0 {
                            label.width + 12.0
                        } else {
                            0.0
                        });
            }
        }
        cedges.push(CEdge {
            src,
            dst,
            minlen: edge.edge.minlen,
            label_w: if label.width > 0.0 {
                label.width + 12.0
            } else {
                0.0
            },
            label_h: if label.height > 0.0 {
                label.height + 4.0
            } else {
                0.0
            },
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        drawn.push(edge);
        edge_labels.push(label);
    }

    let layout_options = LayeredOptions {
        node_sep: number_option("nodeSpacing", NODE_SEP) * if tight { 0.5 } else { 1.0 },
        edge_sep: EDGE_SEP,
        rank_sep: number_option("rankSpacing", RANK_SEP) * if tight { 0.75 } else { 1.0 },
        port_sep: 20.0,
        tracks: flow_curve.is_some_and(|curve| curve.starts_with("step")),
        straight: flow_curve == Some("linear"),
        head_room: marker_trim("arrow_point"),
        ..LayeredOptions::default()
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options);

    let title = title_label(
        options.title.as_deref(),
        options,
        size,
        if tight { title_room(options) } else { 4000.0 },
    );
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = layout.width.max(title.width);
    shift_layout(
        &mut cnodes,
        &mut cedges,
        pad + (inner - layout.width) / 2.0,
        pad + title_height,
    );
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();
    let mut links = Vec::new();

    let mut crossings = Crossings::new();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let surface = svg::esc(&theme.surface);
    let border = svg::esc(&theme.border);
    let line = svg::esc(&theme.line);
    let shape_attrs = format!(" fill=\"{surface}\" stroke=\"{border}\"");
    let line_attrs = format!(" stroke=\"{border}\"");
    let mut clusters = String::new();
    let mut struck_titles = String::new();
    let mut nodes_out = String::new();
    let mut fills = Vec::new();
    for (i, view) in views.iter().enumerate() {
        let c = &cnodes[i];
        let node = &graph.nodes[i];
        let (x, y) = (c.x, c.y);
        let classes = class_names(&node.classes);
        let id = svg::esc_text(node.id);
        if c.is_group {
            if paints(&view.style.shape) {
                fills.push(Fill {
                    bounds: [x - c.w / 2.0, y - c.h / 2.0, x + c.w / 2.0, y + c.h / 2.0],
                    shape: view.style.shape.clone(),
                });
            }
            let title_x = cluster_title_x(c, view.label.width, &crossings, group_pad);
            let title_y = y - c.h / 2.0 + 8.0 + view.label.height / 2.0;
            let struck = title_crossed(c, view.label.width, title_x, &crossings);
            let muted = svg::esc(&theme.muted);
            let title_attrs = format!(
                " class=\"pele-cluster-label\"{}{}",
                if struck {
                    format!(" fill=\"{muted}\"")
                } else {
                    ink_fill(&view.style, Some(&theme.muted))
                },
                view.style.text
            );
            if struck {
                struck_titles.push_str(&struck_title(
                    &view.label,
                    title_x,
                    title_y,
                    &title_attrs,
                    &id,
                    &cnodes,
                    i,
                    options,
                ));
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster{classes}\" data-id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} stroke=\"{border}\"{}/>{}</g>",
                n(x - c.w / 2.0),
                n(y - c.h / 2.0),
                n(c.w),
                n(c.h),
                svg::esc(&theme.surface_alt),
                if sets_fill(&view.style.shape) {
                    ""
                } else {
                    " fill-opacity=\"0.5\""
                },
                rounded(&view.style.shape, theme.radius),
                if struck {
                    String::new()
                } else {
                    svg::label_with_options(&view.label, title_x, title_y, &title_attrs, options)
                }
            ));
            continue;
        }
        let shape_style = format!("{shape_attrs}{}", view.style.shape);
        let line_style = format!("{line_attrs}{}", view.style.line);
        let inner = shape_svg(
            view.shape,
            view.w,
            view.h,
            &shape_style,
            &line_style,
            theme.radius,
            &theme.line,
        );
        let ink = if view.shape.filled() {
            ink_fill(&view.style, None)
        } else {
            String::new()
        };
        let text = svg::label_with_options(
            &view.label,
            0.0,
            view.dy,
            &format!(" class=\"pele-label\"{ink}{}", view.style.text),
            options,
        );
        let mut body = format!("{inner}{text}");
        if let Some(vertex) = node.vertex {
            if let Some(tooltip) = &vertex.tooltip {
                body = format!("<title>{}</title>{body}", svg::esc_text(tooltip));
            }
            if let Some(href) = &vertex.link {
                let target = vertex.target.as_ref().map_or(String::new(), |value| {
                    format!(" target=\"{}\"", svg::esc(value))
                });
                body = format!(
                    "<a href=\"{}\"{target} rel=\"noopener\">{body}</a>",
                    svg::esc(href)
                );
                links.push(crate::LinkInfo {
                    id: vertex.id.clone(),
                    href: href.clone(),
                    internal: false,
                });
            }
        }
        nodes_out.push_str(&format!(
            "<g class=\"pele-node pele-shape-{}{classes}\" data-id=\"{id}\" transform=\"translate({},{})\">{body}</g>",
            view.shape.name(),
            n(x),
            n(y)
        ));
    }

    let mut reach = side_reach();
    for (ce, edge) in cedges.iter().zip(&drawn) {
        if ce.src == ce.dst || edge.edge.stroke == Stroke::Invisible || ce.route.len() < 6 {
            continue;
        }
        reach.add(
            ce.src,
            end_side(&ce.route, 0, &cnodes[ce.src]),
            edge.edge.start.kind(),
        );
        reach.add(
            ce.dst,
            end_side(&ce.route, ce.route.len() - 3, &cnodes[ce.dst]),
            edge.edge.end.kind(),
        );
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen = HashMap::<usize, usize>::new();
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let style = edge.style.as_ref();
        let stroke = edge.edge.stroke;
        if stroke != Stroke::Invisible {
            let start = edge.edge.start.kind();
            let end = edge.edge.end.kind();
            let ce = &cedges[i];
            let path: EdgePath = if ce.src == ce.dst {
                let src = ce.src;
                let k = loop_seen.entry(src).or_insert(0);
                let at = *k;
                *k += 1;
                let view = &views[src];
                let sideways = flows_sideways(&cnodes, src, root_dir);
                let c = cnodes[src].clone();
                loop_path(
                    &c,
                    view.w,
                    view.h,
                    at,
                    sideways,
                    LOOP,
                    marker_trim(end),
                    &mut cedges[i],
                )
            } else {
                let mut route = ce.route.clone();
                let last = route.len().saturating_sub(3);
                let (src, dst) = (&cnodes[ce.src], &cnodes[ce.dst]);
                let leaves = reach.of(ce.src, end_side(&route, 0, src));
                let arrives = reach.of(ce.dst, end_side(&route, last, dst));
                let (from, to) = (&views[ce.src], &views[ce.dst]);
                inset_route(&mut route, 0, from.w, from.h, src, |side, t| {
                    from.shape.inset(from.w, from.h, side, t)
                });
                inset_route(&mut route, last, to.w, to.h, dst, |side, t| {
                    to.shape.inset(to.w, to.h, side, t)
                });
                route_path(
                    &route,
                    edge.curve,
                    marker_trim(start),
                    marker_trim(end),
                    Some(leaves),
                    Some(arrives),
                )
            };
            let line_style = style.map_or("", |style| style.line.as_str());
            let over = over_fills(
                &path.d,
                &[path.sx, path.sy, path.ex, path.ey],
                &fills,
                line_style,
                path.tokens.as_deref(),
            );
            let color = match &over {
                Some(ink) if !ink.is_empty() => ink.clone(),
                _ => style
                    .and_then(|style| style.stroke.as_deref())
                    .map_or_else(|| line.clone(), svg::esc),
            };
            let attrs = match stroke {
                Stroke::Thick => " stroke-width=\"2.5\"",
                Stroke::Dotted => " stroke-dasharray=\"3 4\"",
                _ => "",
            };
            if over.as_deref() == Some("") {
                casings.push_str(&edge_casing(
                    &path,
                    start,
                    end,
                    line_style,
                    stroke == Stroke::Thick,
                    &theme.background,
                ));
            }
            let over_attr = over
                .as_deref()
                .filter(|ink| !ink.is_empty())
                .map_or(String::new(), |ink| format!(" stroke=\"{ink}\""));
            edges_out.push_str(&format!(
                "<g class=\"pele-edge\" data-id=\"{}\"{over_attr}{line_style}><path d=\"{}\"{attrs}/>{}{}</g>",
                svg::esc_text(&edge.edge.id),
                path.d,
                marker(start, path.sx, path.sy, path.sdx, path.sdy, &color, "none"),
                marker(end, path.ex, path.ey, path.edx, path.edy, &color, "none"),
            ));
        }
        let label = &edge_labels[i];
        if label.width > 0.0 {
            labels_out.push_str(&edge_label_svg(
                &edge.edge.id,
                label,
                cedges[i].label_x,
                cedges[i].label_y,
                style.map_or("", |style| style.text.as_str()),
                options,
            ));
        }
    }

    let mut contents = svg::label_with_options(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"bold\"",
        options,
    );
    if !clusters.is_empty() {
        contents.push_str(&format!("<g class=\"pele-clusters\">{clusters}</g>"));
    }
    contents.push_str(&casings_svg(&casings, &theme.background));
    if !edges_out.is_empty() {
        contents.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"{line}\" stroke-linecap=\"round\">{edges_out}</g>"
        ));
    }
    if !struck_titles.is_empty() {
        contents.push_str(&format!(
            "<g class=\"pele-cluster-titles\">{struck_titles}</g>"
        ));
    }
    if !labels_out.is_empty() {
        contents.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{labels_out}</g>",
            n(edge_size)
        ));
    }
    contents.push_str(&format!("<g class=\"pele-nodes\">{nodes_out}</g>"));
    let mut effective = options.clone();
    effective.acc_title = model.acc_title.clone().or(effective.acc_title);
    effective.acc_descr = model.acc_descr.clone().or(effective.acc_descr);
    let svg = svg::document("flowchart", width, height, &effective, &contents);
    let mut result = RenderResult::new("flowchart", svg, width, height);
    result.links = links;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chain_labels_shapes_and_direction() {
        let model = parse(
            "flowchart LR\n A([Start]) -->|go| B{Choice} --> C[(Database)]",
            10,
        )
        .unwrap();
        assert_eq!(model.direction, Dir::Lr);
        assert_eq!(model.nodes.len(), 3);
        assert_eq!(model.nodes[0].shape, Shape::Stadium);
        assert_eq!(model.nodes[1].shape, Shape::Diamond);
        assert_eq!(model.nodes[2].shape, Shape::Cylinder);
        assert_eq!(model.edges[0].label, "go");
        assert_eq!(model.edges[1].head, 2);
    }

    #[test]
    fn cartesian_edges_are_bounded_before_expansion() {
        let source = "graph TD;A & B --> C & D";
        assert!(parse(source, 3).is_err());
        assert_eq!(parse(source, 4).unwrap().edges.len(), 4);
    }

    #[test]
    fn redefinitions_preserve_insertion_order_and_last_label() {
        let model = parse("graph TD;A[old]-->B;A[new];B-->A", 10).unwrap();
        assert_eq!(model.nodes.len(), 2);
        assert_eq!(model.nodes[0].label, "new");
        assert_eq!(model.edges.len(), 2);
    }

    #[test]
    fn quoted_labels_keep_semicolons_and_newlines() {
        let model = parse("graph TD;A[\"x;y\nz\"]-->B", 10).unwrap();
        assert_eq!(model.nodes[0].label, "x;y\nz");
    }

    #[test]
    fn supports_labeled_link_forms_and_stroke_patterns() {
        let model = parse(
            "graph LR;A -- hello --> B;B -. retry .-> C;C ==> D;D o--o E;E ~~~ F",
            20,
        )
        .unwrap();
        assert_eq!(model.edges[0].label, "hello");
        assert_eq!(model.edges[1].stroke, Stroke::Dotted);
        assert_eq!(model.edges[1].label, "retry");
        assert_eq!(model.edges[2].stroke, Stroke::Thick);
        assert_eq!(model.edges[3].start, Marker::Circle);
        assert_eq!(model.edges[3].end, Marker::Circle);
        assert_eq!(model.edges[4].stroke, Stroke::Invisible);
    }

    #[test]
    fn repeated_subgraphs_merge_and_nesting_is_preserved() {
        let model = parse(
            "graph TD;subgraph S[Outer];subgraph T[Inner];A-->B;end;end;subgraph S;C;end",
            10,
        )
        .unwrap();
        assert_eq!(model.groups.len(), 2);
        assert_eq!(model.groups[0].id, "T");
        assert_eq!(model.groups[0].members, ["A", "B"]);
        assert_eq!(model.groups[1].id, "S");
        assert_eq!(model.groups[1].members, ["T", "C"]);
    }

    #[test]
    fn class_and_explicit_styles_are_resolved_in_order() {
        let model = parse(
            "flowchart TD;A:::hot-->B;classDef hot fill:#f00,color:#fff;style A fill:#0f0",
            10,
        )
        .unwrap();
        let mut styles = compiled(&model, &["default", "node", "hot"]);
        styles.extend(model.nodes[0].styles.iter().cloned());
        let style = resolve_style(&styles);
        assert_eq!(style.shape, " style=\"fill:#0f0;\"");
        assert_eq!(style.text, " style=\"fill:#fff;\"");
    }

    #[test]
    fn metadata_selects_shapes_and_quoted_labels() {
        let model = parse("flowchart LR;A@{shape: diamond, label: \"x,y\"}-->B", 10).unwrap();
        assert_eq!(model.nodes[0].shape, Shape::Diamond);
        assert_eq!(model.nodes[0].label, "x,y");
        assert!(parse("flowchart LR;A@{shape: made-up}", 10).is_err());
    }

    #[test]
    fn reports_invalid_syntax_and_unported_features() {
        for source in [
            "flowchart LR;A[broken",
            "flowchart LR;A-->[bad]",
            "flowchart LR;end",
            "flowchart LR;subgraph S;A",
        ] {
            assert!(parse(source, 20).is_err(), "{source}");
        }
        assert!(parse("flowchart LR;A;style A fill:var(--anything)", 20).is_err());
        let deep = format!(
            "flowchart LR;{}A;{}",
            "subgraph S;".repeat(200),
            "end;".repeat(200)
        );
        assert_eq!(parse(&deep, 20).unwrap_err().code, "limit");
    }

    #[test]
    fn static_svg_is_escaped_deterministic_and_oriented() {
        let options = RenderOptions::default();
        let source = "flowchart LR;A[\"<script>&quoted\"]-->B[Done]";
        let a = render(source, &options).unwrap();
        let b = render(source, &options).unwrap();
        assert_eq!(a.svg, b.svg);
        assert!(a.width.is_finite() && a.height.is_finite());
        assert!(a.svg.contains("&amp;quoted"));
        assert!(!a.svg.contains("<script>"));
        assert!(!a.svg.contains("var("));
        assert!(!a.svg.contains("<foreignObject"));
        assert!(a.svg.contains("data-id=\"A\""));
        let generic = render("flowchart LR;A[\"List<T>\"]", &options).unwrap();
        assert!(generic.svg.contains("List&lt;T&gt;"));
    }

    #[test]
    fn links_keep_metadata_and_tooltips_without_executing_callbacks() {
        let options = RenderOptions::default();
        let result=render("flowchart LR;A-->B;click A \"https://example.com?q=A%26B\" \"Go here\" _blank;click B call callback() \"A tooltip\"",&options).unwrap();
        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].href, "https://example.com?q=A%26B");
        assert!(result.svg.contains("target=\"_blank\" rel=\"noopener\""));
        assert!(result.svg.contains("<title>A tooltip</title>"));
        assert!(!result.svg.contains("callback()"));
    }

    #[test]
    fn unsafe_link_schemes_and_obfuscations_are_omitted() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "//example.com",
            "\\\\example.com",
            "%6aavascript%3aalert(1)",
            "javascript&colon;alert(1)",
            "%252f%252fexample.com",
            "file:///tmp/private",
        ] {
            let result = render(
                &format!("flowchart LR;A;click A \"{url}\""),
                &RenderOptions::default(),
            )
            .unwrap();
            assert!(result.links.is_empty(), "{url}");
            assert!(!result.svg.contains("<a href"), "{url}");
        }
    }

    #[test]
    fn explicit_font_measurements_affect_layout() {
        let normal = render("flowchart LR;A[Long label]-->B", &RenderOptions::default()).unwrap();
        let options = RenderOptions {
            measurer: Some(std::sync::Arc::new(|text: &str, size: f64, _style: u8| {
                text.chars().count() as f64 * size * 2.0
            })),
            ..RenderOptions::default()
        };
        let wide = render("flowchart LR;A[Long label]-->B", &options).unwrap();
        assert!(wide.width > normal.width);
    }

    #[test]
    fn narrow_hosts_turn_horizontal_flow_and_honor_opt_out() {
        let source = "flowchart LR;A-->B-->C-->D-->E";
        let options = RenderOptions {
            max_width: Some(300.0),
            ..RenderOptions::default()
        };
        let down = render(source, &options).unwrap();
        assert!(down.width < down.height);
        assert!(down.width <= 300.0);
        let fixed = render(
            source,
            &RenderOptions {
                auto_direction: false,
                ..options.clone()
            },
        )
        .unwrap();
        assert!(fixed.width > fixed.height);
        let below_breakpoint = render(
            source,
            &RenderOptions {
                direction_breakpoint: 250.0,
                ..options
            },
        )
        .unwrap();
        assert!(below_breakpoint.width > below_breakpoint.height);
    }

    #[test]
    fn extended_shape_port_renders_all_named_definitions() {
        for name in crate::flow_shapes::SHAPES {
            let result = render(
                &format!("flowchart TD;A@{{shape: {name}, label: \"Label\"}}"),
                &RenderOptions::default(),
            )
            .unwrap();
            assert!(
                result.width.is_finite() && result.height.is_finite(),
                "{name}"
            );
            assert!(!result.svg.contains("var("), "{name}");
            assert!(!result.svg.contains("currentColor"), "{name}");
        }
    }

    #[test]
    fn numeric_style_fragments_and_headless_edge_metadata_match_upstream() {
        let model=parse("flowchart LR;A e1@--> B;e1@{animate:true};classDef animate stroke-dasharray:9,5,stroke-dashoffset:900,animation:dash 25s linear infinite;class e1 animate",20).unwrap();
        assert_eq!(model.nodes.len(), 2);
        assert_eq!(model.edges.len(), 1);
        assert_eq!(model.edges[0].classes, ["animate"]);
        let style = resolve_style(&compiled(&model, &["animate"]));
        assert!(style.line.contains("stroke-dasharray:9;"));
        assert!(style.line.contains("stroke-dashoffset:900;"));
    }

    #[test]
    fn edges_to_subgraphs_and_subgraph_directions_render() {
        let options = RenderOptions::default();
        let result = render(
            "flowchart LR\n subgraph TOP\n direction TB\n subgraph B1\n direction RL\n i1 --> f1\n end\n subgraph B2\n direction BT\n i2 --> f2\n end\n end\n A --> TOP --> B\n B1 --> B2",
            &options,
        )
        .unwrap();
        for id in ["TOP", "B1", "B2"] {
            assert!(
                result
                    .svg
                    .contains(&format!("class=\"pele-cluster\" data-id=\"{id}\"")),
                "{id}"
            );
        }
        assert!(!result.svg.contains("data-id=\"TOP\" transform"));
        assert!(result.svg.contains("data-id=\"L_A_TOP_0\""));
        assert!(!result.svg.contains("var(") && !result.svg.contains("NaN"));
        let model = parse("flowchart LR\n subgraph S\n direction TB\n a\n end", 10).unwrap();
        assert_eq!(model.groups[0].direction, Some(Dir::Tb));
    }

    #[test]
    fn collapsed_subgraphs_draw_one_node_and_redirect_edges() {
        let result = render(
            "flowchart TD\n Start --> one\n subgraph one [My Group]\n A --> B\n end\n one --> End\n Start --> A\n one@{ view: collapsed }",
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(result.svg.contains("pele-shape-collapsedGroup"));
        assert!(!result.svg.contains("class=\"pele-cluster\""));
        assert!(!result.svg.contains("data-id=\"A\""));
        assert_eq!(result.svg.matches("class=\"pele-edge\"").count(), 3);
    }

    #[test]
    fn subgraph_membership_follows_the_first_subgraph_to_close() {
        let model = parse(
            "flowchart TB\n c1-->a2\n subgraph one\n a1-->a2\n end\n subgraph two\n subgraph inner\n a1\n b1\n end\n b1-->b2\n end",
            20,
        )
        .unwrap();
        let ids: Vec<_> = model.groups.iter().map(|group| group.id.as_str()).collect();
        assert_eq!(ids, ["one", "inner", "two"]);
        assert_eq!(model.groups[0].members, ["a1", "a2"]);
        assert_eq!(model.groups[1].members, ["b1"]);
        assert_eq!(model.groups[2].members, ["inner", "b2"]);
    }

    #[test]
    fn syntax_columns_use_utf16_offsets() {
        let error = parse("flowchart LR\n A[\"👋\"] --> @", 20).unwrap_err();
        assert_eq!(error.line, 2);
        assert_eq!(error.column, 14);
    }
}
