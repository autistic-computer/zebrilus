use std::collections::BTreeMap;
use std::fmt::Write;

use crate::{PeleError, RenderOptions, RenderResult, svg, text};

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub id: String,
    pub description: String,
    pub kind: String,
    pub group: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Head {
    None,
    Arrow,
    Cross,
    Open,
    HalfSolid,
    HalfStick,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Arrow {
    dotted: bool,
    head: Head,
    at_start: bool,
    at_end: bool,
    bottom: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Placement {
    Left,
    Right,
    Over,
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Message {
        from: usize,
        to: usize,
        text: String,
        arrow: Arrow,
        activate: bool,
        number: Option<f64>,
    },
    Note {
        first: usize,
        last: usize,
        text: String,
        placement: Placement,
    },
    Activate(usize),
    Deactivate(usize),
    Frame {
        kind: String,
        text: String,
    },
    Divider(String),
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActorGroup {
    pub title: String,
    pub fill: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SequenceModel {
    pub actors: Vec<Actor>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_description: Option<String>,
    pub groups: Vec<ActorGroup>,
    events: Vec<Event>,
    created: BTreeMap<usize, usize>,
    destroyed: BTreeMap<usize, usize>,
}

enum Pending {
    Create(usize),
    Destroy(String),
}

fn word<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let head = line.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let tail = &line[prefix.len()..];
    if tail.is_empty() || tail.starts_with(char::is_whitespace) {
        Some(tail.trim())
    } else {
        None
    }
}

fn failure(message: impl Into<String>, line: usize) -> PeleError {
    PeleError::syntax(message, line, 1)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(|c| "<>:;,\n\r+()\\/".contains(c))
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn actor(model: &mut SequenceModel, id: &str, line: usize) -> Result<usize, PeleError> {
    let id = id.trim();
    if !valid_id(id) {
        return Err(failure(format!("Invalid sequence participant: {id}"), line));
    }
    if let Some(index) = model.actors.iter().position(|actor| actor.id == id) {
        return Ok(index);
    }
    let index = model.actors.len();
    model.actors.push(Actor {
        id: id.to_owned(),
        description: id.to_owned(),
        kind: "participant".to_owned(),
        group: None,
    });
    Ok(index)
}

fn alias(value: &str) -> (&str, Option<&str>) {
    let lower = value.to_ascii_lowercase();
    match lower.find(" as ") {
        Some(at) => (value[..at].trim(), Some(value[at + 4..].trim())),
        None => (value.trim(), None),
    }
}

fn message_text(value: &str) -> String {
    let trimmed = value.trim();
    for prefix in ["wrap:", "nowrap:", ":wrap:", ":nowrap:"] {
        if let Some(value) = trimmed.strip_prefix(prefix) {
            return value.trim().to_owned();
        }
    }
    trimmed.to_owned()
}

fn participant(
    value: &str,
    kind: &str,
    line: usize,
) -> Result<(String, String, String), PeleError> {
    if let Some(at) = value.find("@{") {
        let id = value[..at].trim();
        let close = value
            .rfind('}')
            .ok_or_else(|| failure("Unclosed participant metadata", line))?;
        let metadata: serde_yaml::Value = serde_yaml::from_str(&value[at + 1..=close])
            .map_err(|error| failure(format!("Invalid participant metadata: {error}"), line))?;
        if !metadata.is_mapping() {
            return Err(failure("Participant metadata must be an object", line));
        }
        let remaining = value[close + 1..].trim();
        let explicit_alias = if remaining.is_empty() {
            None
        } else {
            Some(
                word(remaining, "as")
                    .filter(|alias| !alias.is_empty())
                    .ok_or_else(|| failure("Expected as after participant metadata", line))?,
            )
        };
        let configured_kind = metadata
            .get("type")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or(kind);
        let configured_kind = match configured_kind {
            "actor" | "boundary" | "control" | "entity" | "database" | "collections" | "queue" => {
                configured_kind
            }
            _ => "participant",
        };
        let configured_alias = metadata.get("alias").and_then(serde_yaml::Value::as_str);
        let description = explicit_alias
            .filter(|alias| *alias != id)
            .or(configured_alias)
            .or(explicit_alias)
            .unwrap_or(id);
        Ok((
            id.to_owned(),
            message_text(description),
            configured_kind.to_owned(),
        ))
    } else {
        let (id, label) = alias(value);
        if value.to_ascii_lowercase().ends_with(" as") || label.is_some_and(str::is_empty) {
            return Err(failure("Expected a participant alias after as", line));
        }
        Ok((
            id.to_owned(),
            message_text(label.unwrap_or(id)),
            kind.to_owned(),
        ))
    }
}

fn color(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        return matches!(lower.len(), 4 | 5 | 7 | 9) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if (lower.starts_with("rgb(")
        || lower.starts_with("rgba(")
        || lower.starts_with("hsl(")
        || lower.starts_with("hsla("))
        && lower.ends_with(')')
    {
        return lower
            .bytes()
            .all(|b| b.is_ascii_digit() || b"rgbahsl(),.% +-".contains(&b));
    }
    matches!(
        lower.as_str(),
        "transparent"
            | "black"
            | "white"
            | "red"
            | "green"
            | "blue"
            | "yellow"
            | "orange"
            | "purple"
            | "pink"
            | "gray"
            | "grey"
            | "lightgray"
            | "lightgrey"
            | "lightblue"
            | "lightyellow"
            | "lightgreen"
            | "aqua"
            | "cyan"
            | "magenta"
            | "brown"
            | "navy"
            | "silver"
            | "teal"
            | "lime"
            | "olive"
            | "maroon"
    )
}

fn group_data(value: &str) -> ActorGroup {
    let boundary = if value.contains('(') {
        value.rfind(')').map(|at| at + 1)
    } else {
        value.find(char::is_whitespace).or(Some(value.len()))
    };
    if let Some(boundary) = boundary {
        let candidate = value[..boundary].trim();
        if color(candidate) {
            return ActorGroup {
                title: value[boundary..].trim().to_owned(),
                fill: candidate.to_owned(),
            };
        }
    }
    ActorGroup {
        title: value.trim().to_owned(),
        fill: "transparent".to_owned(),
    }
}

fn arrow(token: &str) -> Arrow {
    let dotted = token.contains("--");
    let at_start = token.starts_with('<') || token.starts_with('/') || token.starts_with('\\');
    let at_end = token.ends_with('>')
        || token.ends_with('x')
        || token.ends_with('X')
        || token.ends_with(')')
        || token.ends_with('/')
        || token.ends_with('\\');
    let head = if token.contains("|\\")
        || token.contains("|/")
        || token.contains("/|")
        || token.contains("\\|")
    {
        Head::HalfSolid
    } else if token.contains("//") || token.contains("\\\\") {
        Head::HalfStick
    } else if token.ends_with('x') || token.ends_with('X') {
        Head::Cross
    } else if token.ends_with(')') {
        Head::Open
    } else if token.ends_with(">>") {
        Head::Arrow
    } else {
        Head::None
    };
    Arrow {
        dotted,
        head,
        at_start,
        at_end,
        bottom: token.ends_with('/') || token.starts_with('\\'),
    }
}

fn message_parts(line: &str) -> Option<(&str, &str, &str, &str)> {
    const ARROWS: &[&str] = &[
        "<<-->>", "<<->>", "--|\\", "--|/", "--\\\\", "--//", "/|--", "\\|--", "//--", "\\\\--",
        "-->>", "->>", "--x", "--X", "--)", "-x", "-X", "-)", "-|\\", "-|/", "-\\\\", "-//", "/|-",
        "\\|-", "//-", "\\\\-", "-->", "->",
    ];
    let colon = line.find(':')?;
    let endpoints = &line[..colon];
    let mut found: Option<(usize, &str)> = None;
    for token in ARROWS {
        if let Some(at) = endpoints.find(token)
            && found.is_none_or(|(old, old_token)| {
                at < old || (at == old && token.len() > old_token.len())
            })
        {
            found = Some((at, token));
        }
    }
    let (at, token) = found?;
    Some((
        endpoints[..at].trim(),
        token,
        endpoints[at + token.len()..].trim(),
        line[colon + 1..].trim(),
    ))
}

fn statements(source: &str) -> Vec<(usize, &str)> {
    let mut statements = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let mut start = 0;
        for (at, ch) in line.char_indices() {
            if ch != ';' {
                continue;
            }
            let preceding = &line[start..at];
            let is_entity = preceding.rsplit_once(['#', '&']).is_some_and(|(_, code)| {
                !code.is_empty() && code.bytes().all(|c| c.is_ascii_alphanumeric())
            });
            if !is_entity {
                statements.push((index + 1, line[start..at].trim()));
                start = at + 1;
            }
        }
        statements.push((index + 1, line[start..].trim()));
    }
    statements
}

pub fn parse(source: &str, max_edges: usize) -> Result<SequenceModel, PeleError> {
    let mut model = SequenceModel::default();
    let mut header = false;
    let mut frames: Vec<(String, usize)> = Vec::new();
    let mut current_group = None;
    let mut active: BTreeMap<usize, usize> = BTreeMap::new();
    let mut numbering = false;
    let mut number = 1.0;
    let mut step = 1.0;
    let mut messages = 0;
    let mut pending = None;
    for (line_number, line) in statements(source) {
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !header {
            if !line.eq_ignore_ascii_case("sequenceDiagram") {
                return Err(failure("Expected sequenceDiagram", line_number));
            }
            header = true;
            continue;
        }
        if let Some((kind, _)) = frames.last()
            && kind == "box"
            && !line.eq_ignore_ascii_case("end")
            && word(line, "participant").is_none()
            && word(line, "actor").is_none()
            && word(line, "destroy").is_none()
        {
            return Err(failure(
                "A participant box may only contain participant declarations",
                line_number,
            ));
        }
        if let Some((kind, tail)) = word(line, "participant")
            .map(|v| ("participant", v))
            .or_else(|| word(line, "actor").map(|v| ("actor", v)))
        {
            let (id, description, kind) = participant(tail, kind, line_number)?;
            let index = actor(&mut model, &id, line_number)?;
            let participant = &mut model.actors[index];
            if participant.group.is_some()
                && current_group.is_some()
                && participant.group != current_group
            {
                return Err(failure(
                    format!("Participant {id} belongs to more than one box"),
                    line_number,
                ));
            }
            participant.kind = kind;
            participant.description = description;
            participant.group = participant.group.or(current_group);
            continue;
        }
        if let Some(tail) = word(line, "create") {
            let (kind, tail) = word(tail, "participant")
                .map(|v| ("participant", v))
                .or_else(|| word(tail, "actor").map(|v| ("actor", v)))
                .ok_or_else(|| {
                    failure("Expected participant or actor after create", line_number)
                })?;
            let (id, description, kind) = participant(tail, kind, line_number)?;
            if model.actors.iter().any(|actor| actor.id == id) {
                return Err(PeleError::semantic(
                    "It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior",
                ));
            }
            let index = actor(&mut model, &id, line_number)?;
            model.actors[index].kind = kind;
            model.actors[index].description = description;
            pending = Some(Pending::Create(index));
            continue;
        }
        if let Some(tail) = word(line, "destroy") {
            let id = tail.trim();
            if !valid_id(id) {
                return Err(failure(
                    format!("Invalid sequence participant: {id}"),
                    line_number,
                ));
            }
            pending = Some(Pending::Destroy(id.to_owned()));
            continue;
        }
        if let Some(title) = word(line, "title").or_else(|| {
            line.get(..6)
                .filter(|head| head.eq_ignore_ascii_case("title:"))
                .map(|_| line[6..].trim())
        }) {
            model.title = Some(title.to_owned());
            continue;
        }
        if let Some(value) = line
            .get(..9)
            .filter(|head| head.eq_ignore_ascii_case("accTitle:"))
            .map(|_| line[9..].trim())
        {
            model.acc_title = Some(value.to_owned());
            continue;
        }
        if let Some(value) = line
            .get(..9)
            .filter(|head| head.eq_ignore_ascii_case("accDescr:"))
            .map(|_| line[9..].trim())
        {
            model.acc_description = Some(value.to_owned());
            continue;
        }
        if let Some(tail) = word(line, "autonumber") {
            if tail.eq_ignore_ascii_case("off") {
                numbering = false;
            } else {
                let values = tail
                    .split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| failure("Expected autonumber start and step", line_number))?;
                if values.len() > 2
                    || values
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.0)
                {
                    return Err(failure(
                        "Sequence numbers must be nonnegative finite values",
                        line_number,
                    ));
                }
                if let Some(value) = values.first().filter(|value| **value != 0.0) {
                    number = *value;
                }
                if let Some(value) = values.get(1).filter(|value| **value != 0.0) {
                    step = *value;
                }
                numbering = true;
            }
            continue;
        }
        if let Some((activate, tail)) = word(line, "activate")
            .map(|v| (true, v))
            .or_else(|| word(line, "deactivate").map(|v| (false, v)))
        {
            let who = actor(&mut model, tail, line_number)?;
            let count = active.entry(who).or_default();
            if activate {
                *count += 1;
                model.events.push(Event::Activate(who));
            } else {
                if *count == 0 {
                    return Err(failure(
                        format!("Trying to inactivate an inactive participant ({tail})"),
                        line_number,
                    ));
                }
                *count -= 1;
                model.events.push(Event::Deactivate(who));
            }
            continue;
        }
        if let Some(tail) = word(line, "note") {
            let (placement, tail) = word(tail, "over")
                .map(|v| (Placement::Over, v))
                .or_else(|| word(tail, "left of").map(|v| (Placement::Left, v)))
                .or_else(|| word(tail, "right of").map(|v| (Placement::Right, v)))
                .ok_or_else(|| failure("Expected note over, left of, or right of", line_number))?;
            let (who, value) = tail
                .split_once(':')
                .ok_or_else(|| failure("Expected ':' after note participants", line_number))?;
            let names = who.split(',').map(str::trim).collect::<Vec<_>>();
            if names.len() > 2 || (placement != Placement::Over && names.len() != 1) {
                return Err(failure("Invalid note participants", line_number));
            }
            let first = actor(&mut model, names[0], line_number)?;
            let last = actor(&mut model, names[names.len() - 1], line_number)?;
            model.events.push(Event::Note {
                first,
                last,
                text: message_text(value),
                placement,
            });
            continue;
        }
        if line.eq_ignore_ascii_case("end") {
            let Some((kind, _)) = frames.pop() else {
                return Err(failure("Unexpected end in sequence diagram", line_number));
            };
            if kind == "box" {
                current_group = None;
            } else {
                model.events.push(Event::End);
            }
            continue;
        }
        let mut block = None;
        for kind in [
            "loop", "alt", "opt", "par", "critical", "break", "rect", "box",
        ] {
            if let Some(tail) = word(line, kind) {
                block = Some((kind, tail));
                break;
            }
        }
        if let Some((kind, tail)) = block {
            if frames.len() >= 128 {
                return Err(PeleError::limit("Sequence frames are nested too deeply"));
            }
            frames.push((kind.to_owned(), line_number));
            if kind == "box" {
                if current_group.is_some() {
                    return Err(failure("Participant boxes cannot be nested", line_number));
                }
                current_group = Some(model.groups.len());
                model.groups.push(group_data(tail));
            } else {
                if kind == "rect" && !color(tail) {
                    return Err(PeleError::unsupported(
                        "Sequence rect color is not supported by the static renderer",
                    ));
                }
                model.events.push(Event::Frame {
                    kind: kind.to_owned(),
                    text: message_text(tail),
                });
            }
            continue;
        }
        let mut divider = None;
        for kind in ["else", "and", "option"] {
            if let Some(tail) = word(line, kind) {
                divider = Some((kind, tail));
                break;
            }
        }
        if let Some((kind, tail)) = divider {
            let required = match kind {
                "else" => "alt",
                "and" => "par",
                _ => "critical",
            };
            if frames.last().is_none_or(|(open, _)| open != required) {
                return Err(failure(
                    format!("Unexpected {kind} outside {required}"),
                    line_number,
                ));
            }
            model.events.push(Event::Divider(message_text(tail)));
            continue;
        }
        if ["link", "links", "properties", "details"]
            .iter()
            .any(|kind| word(line, kind).is_some())
        {
            return Err(PeleError::unsupported(
                "Sequence participant menu metadata is not ported yet",
            ));
        }
        if let Some((from, token, target, value)) = message_parts(line) {
            let from = actor(&mut model, from, line_number)?;
            let (activation, target) = if let Some(tail) = target.strip_prefix('+') {
                (1, tail)
            } else if let Some(tail) = target.strip_prefix('-') {
                (-1, tail)
            } else {
                (0, target)
            };
            let to = actor(&mut model, target, line_number)?;
            match pending.take() {
                Some(Pending::Create(created)) => {
                    if to != created {
                        return Err(PeleError::semantic(format!(
                            "The created participant {} does not have an associated creating message after its declaration. Please check the sequence diagram.",
                            model.actors[created].id
                        )));
                    }
                    model.created.insert(created, model.events.len());
                }
                Some(Pending::Destroy(id)) => {
                    let ends = [from, to]
                        .into_iter()
                        .find(|&index| model.actors[index].id == id)
                        .ok_or_else(|| {
                            PeleError::semantic(format!(
                                "The destroyed participant {id} does not have an associated destroying message after its declaration. Please check the sequence diagram."
                            ))
                        })?;
                    model.destroyed.insert(ends, model.events.len());
                }
                None => {}
            }
            messages += 1;
            if messages > max_edges {
                return Err(PeleError::limit(format!(
                    "Sequence exceeds the limit of {max_edges} messages"
                )));
            }
            model.events.push(Event::Message {
                from,
                to,
                text: message_text(value),
                arrow: arrow(token),
                activate: activation == 1,
                number: numbering.then_some(number),
            });
            number = ((number + step) * 100.0 + 0.5).floor() / 100.0;
            if activation == 1 {
                *active.entry(to).or_default() += 1;
                model.events.push(Event::Activate(to));
            } else if activation == -1 {
                let count = active.entry(from).or_default();
                if *count == 0 {
                    return Err(failure(
                        format!(
                            "Trying to inactivate an inactive participant ({})",
                            model.actors[from].id
                        ),
                        line_number,
                    ));
                }
                *count -= 1;
                model.events.push(Event::Deactivate(from));
            }
            continue;
        }
        return Err(failure(
            format!("Unsupported sequence statement: {line}"),
            line_number,
        ));
    }
    if !header {
        return Err(failure("Expected sequenceDiagram", 1));
    }
    if let Some((kind, line)) = frames.last() {
        return Err(failure(
            format!("Unclosed sequence {kind}; expected end"),
            *line,
        ));
    }
    Ok(model)
}

struct Column {
    label: text::Label,
    width: f64,
    height: f64,
    x: f64,
    top: f64,
    activations: Vec<f64>,
    created_at: Option<usize>,
    destroyed_at: Option<usize>,
    life_start: f64,
    life_end: Option<f64>,
}

struct Frame {
    kind: String,
    title: text::Label,
    top: f64,
    left: f64,
    right: f64,
    tab_width: f64,
    dividers: Vec<(f64, text::Label)>,
    fill: String,
}

fn figure(kind: &str) -> bool {
    matches!(
        kind,
        "actor" | "boundary" | "control" | "entity" | "database"
    )
}

fn setting(options: &RenderOptions, name: &str, fallback: f64) -> f64 {
    options
        .config
        .get("sequence")
        .and_then(|config| config.get(name))
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 2000.0)
        .unwrap_or(fallback)
}

fn flag(options: &RenderOptions, name: &str, fallback: bool) -> bool {
    options
        .config
        .get("sequence")
        .and_then(|config| config.get(name))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(fallback)
}

fn actor_size(kind: &str, label: &text::Label, size: f64, tight: bool) -> (f64, f64) {
    if figure(kind) {
        (
            40.0_f64.max(label.width + 8.0),
            34.0 + if label.height > 0.0 {
                4.0 + label.height
            } else {
                0.0
            },
        )
    } else {
        let extra = if kind == "collections" {
            5.0
        } else if kind == "queue" {
            12.0
        } else {
            0.0
        };
        (
            (if tight { 48.0_f64 } else { 64.0_f64 })
                .max(label.width + if tight { 16.0 } else { 32.0 })
                + extra,
            ((size * 1.5 + 0.5).floor() + 20.0).max(label.height + 20.0)
                + if kind == "collections" { 5.0 } else { 0.0 },
        )
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64, radius: f64, fill: &str, stroke: &str) -> String {
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" stroke=\"{}\"/>",
        svg::num(x),
        svg::num(y),
        svg::num(width.max(0.0)),
        svg::num(height.max(0.0)),
        svg::num(radius),
        svg::esc(fill),
        svg::esc(stroke)
    )
}

fn draw_actor(
    actor: &Actor,
    col: &Column,
    top: f64,
    footer: bool,
    options: &RenderOptions,
) -> String {
    let cx = col.x;
    let color = &options.theme;
    let mut body = String::new();
    let x = svg::num(cx);
    let y = |offset: f64| svg::num(top + offset);
    let shape = format!(
        " fill=\"{}\" stroke=\"{}\"",
        svg::esc(color.node_fill()),
        svg::esc(color.node_stroke())
    );
    let line = format!(
        " fill=\"none\" stroke=\"{}\"",
        svg::esc(color.node_stroke())
    );
    if actor.kind == "actor" {
        let _ = write!(
            body,
            "<circle cx=\"{x}\" cy=\"{}\" r=\"6\"{shape}/><path d=\"M{x},{}V{}M{},{}H{}M{},{}L{x},{}L{},{}\"{line}/>",
            y(7.0),
            y(13.0),
            y(24.0),
            svg::num(cx - 10.0),
            y(17.0),
            svg::num(cx + 10.0),
            svg::num(cx - 8.0),
            y(34.0),
            y(24.0),
            svg::num(cx + 8.0),
            y(34.0)
        );
    } else if actor.kind == "boundary" {
        let _ = write!(
            body,
            "<path d=\"M{},{}V{}M{},{}H{}\"{line}/><circle cx=\"{}\" cy=\"{}\" r=\"11\"{shape}/>",
            svg::num(cx - 19.0),
            y(6.0),
            y(28.0),
            svg::num(cx - 19.0),
            y(17.0),
            svg::num(cx - 8.0),
            svg::num(cx + 3.0),
            y(17.0)
        );
    } else if actor.kind == "control" {
        let _ = write!(
            body,
            "<circle cx=\"{x}\" cy=\"{}\" r=\"11\"{shape}/><path d=\"M{},{}L{},{}L{},{}\"{line}/>",
            y(21.0),
            svg::num(cx + 5.0),
            y(5.0),
            svg::num(cx - 1.0),
            y(10.0),
            svg::num(cx + 5.0),
            y(15.0)
        );
    } else if actor.kind == "entity" {
        let _ = write!(
            body,
            "<circle cx=\"{x}\" cy=\"{}\" r=\"11\"{shape}/><path d=\"M{},{}H{}\"{line}/>",
            y(15.0),
            svg::num(cx - 11.0),
            y(30.0),
            svg::num(cx + 11.0)
        );
    } else if actor.kind == "database" {
        let l = svg::num(cx - 14.0);
        let r = svg::num(cx + 14.0);
        let _ = write!(
            body,
            "<path d=\"M{l},{}V{}A14,5 0 0 0 {r},{}V{}A14,5 0 0 0 {l},{}Z\"{shape}/><path d=\"M{l},{}A14,5 0 0 0 {r},{}\"{line}/>",
            y(7.0),
            y(27.0),
            y(27.0),
            y(7.0),
            y(7.0),
            y(7.0),
            y(7.0)
        );
    } else if actor.kind == "collections" {
        let left = cx - col.width / 2.0;
        body.push_str(&rect(
            left + 5.0,
            top,
            col.width - 5.0,
            col.height - 5.0,
            color.radius,
            color.node_fill(),
            color.node_stroke(),
        ));
        body.push_str(&rect(
            left,
            top + 5.0,
            col.width - 5.0,
            col.height - 5.0,
            color.radius,
            color.node_fill(),
            color.node_stroke(),
        ));
    } else if actor.kind == "queue" {
        let left = svg::num(cx - col.width / 2.0 + 6.0);
        let right = svg::num(cx + col.width / 2.0 - 6.0);
        let bottom = svg::num(top + col.height);
        let ry = svg::num(col.height / 2.0);
        let _ = write!(
            body,
            "<path d=\"M{left},{}H{right}A6,{ry} 0 0 1 {right},{bottom}H{left}A6,{ry} 0 0 1 {left},{}Z\"{shape}/><path d=\"M{right},{}A6,{ry} 0 0 0 {right},{bottom}\"{line}/>",
            y(0.0),
            y(0.0),
            y(0.0)
        );
    } else {
        body.push_str(&rect(
            cx - col.width / 2.0,
            top,
            col.width,
            col.height,
            color.radius,
            color.node_fill(),
            color.node_stroke(),
        ));
    }
    let label_x = if actor.kind == "collections" {
        cx - 2.5
    } else if actor.kind == "queue" {
        cx - 3.0
    } else {
        cx
    };
    let label_y = if figure(&actor.kind) {
        top + 38.0 + col.label.height / 2.0
    } else if actor.kind == "collections" {
        top + col.height / 2.0 + 2.5
    } else {
        top + col.height / 2.0
    };
    body.push_str(&svg::label_with_options(
        &col.label,
        label_x,
        label_y,
        &format!(
            " class=\"pele-label\" fill=\"{}\"",
            svg::esc(&color.foreground)
        ),
        options,
    ));
    format!(
        "<g class=\"pele-node pele-actor pele-actor-{}{}\" data-id=\"{}\">{body}</g>",
        svg::esc(&actor.kind),
        if footer { " pele-actor-footer" } else { "" },
        svg::esc(&actor.id)
    )
}

fn head(kind: Head, x: f64, y: f64, direction: f64, up: f64, color: &str) -> String {
    let x0 = svg::num(x);
    let y0 = svg::num(y);
    let base = svg::num(x - direction * 8.0);
    let color = svg::esc(color);
    match kind {
        Head::None => String::new(),
        Head::Arrow => format!(
            "<path class=\"pele-marker\" d=\"M{x0},{y0}L{base},{}V{}Z\" fill=\"{color}\" stroke=\"none\"/>",
            svg::num(y - 3.36),
            svg::num(y + 3.36)
        ),
        Head::Cross => format!(
            "<path class=\"pele-marker\" d=\"M{},{}L{},{}M{},{}L{},{}\" stroke=\"{color}\"/>",
            svg::num(x - 4.0),
            svg::num(y - 4.0),
            svg::num(x + 4.0),
            svg::num(y + 4.0),
            svg::num(x + 4.0),
            svg::num(y - 4.0),
            svg::num(x - 4.0),
            svg::num(y + 4.0)
        ),
        Head::Open => format!(
            "<path class=\"pele-marker\" d=\"M{base},{}L{x0},{y0}L{base},{}\" fill=\"none\" stroke=\"{color}\"/>",
            svg::num(y - 4.5),
            svg::num(y + 4.5)
        ),
        Head::HalfSolid => format!(
            "<path class=\"pele-marker\" d=\"M{x0},{y0}L{base},{}V{y0}Z\" fill=\"{color}\" stroke=\"none\"/>",
            svg::num(y + up * 4.5)
        ),
        Head::HalfStick => format!(
            "<path class=\"pele-marker\" d=\"M{x0},{y0}L{base},{}\" fill=\"none\" stroke=\"{color}\"/>",
            svg::num(y + up * 5.0)
        ),
    }
}

fn activation_end(
    col: &mut Column,
    actor: &Actor,
    end: f64,
    options: &RenderOptions,
) -> (String, f64) {
    let start = col.activations.pop().unwrap_or(end);
    let bottom = end.max(start + 14.0);
    let shape = rect(
        col.x - 5.0 + col.activations.len() as f64 * 4.0,
        start,
        10.0,
        bottom - start,
        2.0,
        options.theme.node_fill(),
        options.theme.node_stroke(),
    );
    (
        format!(
            "<g class=\"pele-activation\" data-id=\"{}\">{shape}</g>",
            svg::esc(&actor.id)
        ),
        bottom,
    )
}

fn edges(col: &Column, extra: usize) -> (f64, f64) {
    let depth = col.activations.len() + extra;
    if depth == 0 {
        (col.x, col.x)
    } else {
        (col.x - 5.0, col.x + 5.0 + (depth - 1) as f64 * 4.0)
    }
}

fn bound(min: &mut f64, max: &mut f64, left: f64, right: f64) {
    *min = min.min(left);
    *max = max.max(right);
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let model = parse(source, options.max_edges)?;
    let natural = draw(&model, options, false)?;
    if options
        .max_width
        .is_some_and(|width| natural.width > width.floor())
    {
        draw(&model, options, true)
    } else {
        Ok(natural)
    }
}

fn label_for(value: &str, size: f64, width: Option<f64>, options: &RenderOptions) -> text::Label {
    text::label_with_measurer(
        value,
        size,
        width,
        options
            .measurer
            .as_deref()
            .unwrap_or(&text::MetricsMeasurer),
    )
}

fn width_for(value: &str, size: f64, style: u8, options: &RenderOptions) -> f64 {
    options
        .measurer
        .as_deref()
        .unwrap_or(&text::MetricsMeasurer)
        .width(value, size, style)
}

fn draw(
    model: &SequenceModel,
    options: &RenderOptions,
    tight: bool,
) -> Result<RenderResult, PeleError> {
    let size = options.font_size;
    let small = (size * 0.875 + 0.5).floor();
    let tiny = (size * 0.75 + 0.5).floor();
    let pad = options.padding;
    let mirror = flag(options, "mirrorActors", true);
    let message_gap = (14.0 + setting(options, "messageMargin", 35.0) - 35.0).max(4.0);
    let wrap = options.max_width.filter(|_| tight).map(|w| {
        ((w - 2.0 * pad) / model.actors.len().max(1) as f64 - if tight { 32.0 } else { 64.0 })
            .max(32.0)
    });
    let mut columns = model
        .actors
        .iter()
        .enumerate()
        .map(|(index, actor)| {
            let label = label_for(&actor.description, size, wrap, options);
            let (width, height) = actor_size(&actor.kind, &label, size, tight);
            Column {
                label,
                width,
                height,
                x: 0.0,
                top: 0.0,
                activations: Vec::new(),
                created_at: model.created.get(&index).copied(),
                destroyed_at: model.destroyed.get(&index).copied(),
                life_start: 0.0,
                life_end: None,
            }
        })
        .collect::<Vec<_>>();
    let labels = model
        .events
        .iter()
        .map(|event| match event {
            Event::Message { text: value, .. }
            | Event::Note { text: value, .. }
            | Event::Frame { text: value, .. }
            | Event::Divider(value) => Some(label_for(value, small, wrap, options)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut needs = BTreeMap::<(usize, usize), f64>::new();
    let mut need = |a: usize, b: usize, width: f64| {
        if a < b && b < columns.len() {
            let item = needs.entry((a, b)).or_default();
            *item = item.max(width);
        }
    };
    for (i, event) in model.events.iter().enumerate() {
        let Some(label) = &labels[i] else { continue };
        match event {
            Event::Message {
                from, to, number, ..
            } => {
                let badge = number.map_or(0.0, |n| {
                    16.0_f64.max(width_for(&svg::num(n), tiny, 0, options) + 8.0)
                });
                if from == to {
                    need(
                        *from,
                        from + 1,
                        28.0 + 8.0 + label.width + 16.0 + 10.0 + badge,
                    );
                } else {
                    let created = if columns[*to].created_at == Some(i) {
                        columns[*to].width / 2.0
                    } else {
                        0.0
                    };
                    need(
                        (*from).min(*to),
                        (*from).max(*to),
                        label.width + 24.0 + 10.0 + badge + created,
                    );
                }
            }
            Event::Note {
                first,
                last,
                placement,
                ..
            } => {
                let width = label.width + 20.0;
                match placement {
                    Placement::Left => {
                        if *first > 0 {
                            need(first - 1, *first, width + 30.0);
                        }
                    }
                    Placement::Right => need(*first, first + 1, width + 30.0),
                    Placement::Over => {
                        if first == last {
                            if *first > 0 {
                                need(first - 1, *first, width / 2.0 + 10.0);
                            }
                            need(*first, first + 1, width / 2.0 + 10.0);
                        } else {
                            need((*first).min(*last), (*first).max(*last), width - 32.0);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut gaps = (0..columns.len().saturating_sub(1))
        .map(|i| {
            let box_extra = if model.actors[i].group != model.actors[i + 1].group {
                (if model.actors[i].group.is_some() {
                    10.0
                } else {
                    0.0
                }) + (if model.actors[i + 1].group.is_some() {
                    10.0
                } else {
                    0.0
                })
            } else {
                0.0
            };
            (columns[i].width / 2.0
                + columns[i + 1].width / 2.0
                + setting(options, "actorMargin", 40.0).min(if tight {
                    8.0
                } else {
                    f64::INFINITY
                })
                + box_extra)
                .max(*needs.get(&(i, i + 1)).unwrap_or(&0.0))
        })
        .collect::<Vec<_>>();
    let mut spans = needs
        .iter()
        .filter(|((lo, hi), _)| hi - lo > 1)
        .map(|(&(lo, hi), &width)| (lo, hi, width))
        .collect::<Vec<_>>();
    spans.sort_by_key(|(lo, hi, _)| (hi - lo, *lo));
    for (lo, hi, width) in spans {
        let total = gaps[lo..hi].iter().sum::<f64>();
        if width > total {
            let share = (width - total) / (hi - lo) as f64;
            for gap in &mut gaps[lo..hi] {
                *gap += share;
            }
        }
    }
    for i in 1..columns.len() {
        columns[i].x = columns[i - 1].x + gaps[i - 1];
    }
    let mut min_x = 0.0_f64;
    let mut max_x = 40.0_f64;
    if !columns.is_empty() {
        min_x = f64::INFINITY;
        max_x = f64::NEG_INFINITY;
    }
    for col in &columns {
        bound(
            &mut min_x,
            &mut max_x,
            col.x - col.width / 2.0,
            col.x + col.width / 2.0,
        );
    }
    let box_title_height = model
        .groups
        .iter()
        .map(|g| label_for(&g.title, small, wrap, options).height)
        .fold(0.0, f64::max);
    let box_top = if model.groups.is_empty() {
        0.0
    } else {
        10.0 + if box_title_height > 0.0 {
            box_title_height + 4.0
        } else {
            0.0
        }
    };
    let row_height = columns
        .iter()
        .filter(|c| c.created_at.is_none())
        .map(|c| c.height)
        .fold(0.0, f64::max);
    let header_bottom = box_top + row_height;
    for col in &mut columns {
        col.top = header_bottom - col.height;
        col.life_start = header_bottom;
    }
    let mut y = header_bottom + 6.0;
    let mut frames = Vec::<Frame>::new();
    let mut frames_out = String::new();
    let mut rectangles = Vec::<String>::new();
    let mut activations = Vec::<String>::new();
    let mut messages = String::new();
    let mut message_labels = String::new();
    let mut notes = String::new();
    let mut crosses = String::new();
    let line = svg::esc(&options.theme.line);
    let foreground = svg::esc(&options.theme.foreground);
    let border = svg::esc(&options.theme.border);
    for (i, event) in model.events.iter().enumerate() {
        match event {
            Event::Activate(who) => columns[*who].activations.push(y),
            Event::Deactivate(who) => {
                let (shape, bottom) =
                    activation_end(&mut columns[*who], &model.actors[*who], y, options);
                activations.push(shape);
                y = y.max(bottom);
            }
            Event::Frame { kind, text: value } => {
                let title = labels[i].as_ref().unwrap().clone();
                let mut left = columns.first().map_or(0.0, |c| c.x - 20.0);
                let mut right = columns.last().map_or(120.0, |c| c.x + 20.0);
                let tab_width = 40.0_f64.max(width_for(kind, tiny, text::BOLD, options) + 16.0);
                if right - left < title.width + tab_width + 16.0 {
                    let extra = (title.width + tab_width + 16.0 - (right - left)) / 2.0;
                    left -= extra;
                    right += extra;
                }
                bound(&mut min_x, &mut max_x, left, right);
                let top = y + if kind == "rect" { 8.0 } else { 12.0 };
                y = top
                    + if kind == "rect" {
                        0.0
                    } else {
                        20.0_f64.max(title.height + 2.0)
                    };
                frames.push(Frame {
                    kind: kind.clone(),
                    title,
                    top,
                    left,
                    right,
                    tab_width,
                    dividers: Vec::new(),
                    fill: if kind == "rect" {
                        value.clone()
                    } else {
                        String::new()
                    },
                });
            }
            Event::Divider(_) => {
                let label = labels[i].as_ref().unwrap().clone();
                if let Some(frame) = frames.last_mut() {
                    frame.dividers.push((y + 12.0, label.clone()));
                    y += 12.0 + label.height;
                }
            }
            Event::End => {
                let Some(frame) = frames.pop() else { continue };
                if let Some(parent) = frames.last_mut() {
                    parent.left = parent.left.min(frame.left - 10.0);
                    parent.right = parent.right.max(frame.right + 10.0);
                    bound(&mut min_x, &mut max_x, parent.left, parent.right);
                }
                let bottom = y + 12.0;
                if frame.kind == "rect" {
                    rectangles.push(rect(
                        frame.left,
                        frame.top,
                        frame.right - frame.left,
                        bottom - frame.top,
                        options.theme.radius,
                        &frame.fill,
                        "none",
                    ));
                } else {
                    let mut inner = rect(
                        frame.left,
                        frame.top,
                        frame.right - frame.left,
                        bottom - frame.top,
                        options.theme.radius,
                        "none",
                        &options.theme.border,
                    );
                    for (at, label) in frame.dividers {
                        let _ = write!(
                            inner,
                            "<path d=\"M{},{}H{}\" fill=\"none\" stroke=\"{border}\" stroke-dasharray=\"4 4\"/>",
                            svg::num(frame.left),
                            svg::num(at),
                            svg::num(frame.right)
                        );
                        inner.push_str(&svg::label_with_options(
                            &label,
                            frame.left + 8.0 + label.width / 2.0,
                            at + 2.0 + label.height / 2.0,
                            &format!(
                                " class=\"pele-frame-label\" fill=\"{}\"",
                                svg::esc(&options.theme.muted)
                            ),
                            options,
                        ));
                    }
                    inner.push_str(&rect(
                        frame.left,
                        frame.top,
                        frame.tab_width,
                        20.0,
                        options.theme.radius,
                        &options.theme.surface,
                        &options.theme.border,
                    ));
                    inner.push_str(&svg::text(
                        &frame.kind,
                        frame.left + frame.tab_width / 2.0,
                        frame.top + 10.0 + tiny * 0.35,
                        tiny,
                        "middle",
                        &options.theme.muted,
                    ));
                    inner.push_str(&svg::label_with_options(
                        &frame.title,
                        frame.left + frame.tab_width + 8.0 + frame.title.width / 2.0,
                        frame.top + 20.0_f64.max(frame.title.height) / 2.0,
                        &format!(
                            " class=\"pele-frame-label\" fill=\"{}\"",
                            svg::esc(&options.theme.muted)
                        ),
                        options,
                    ));
                    let _ = write!(
                        frames_out,
                        "<g class=\"pele-frame pele-frame-{}\">{inner}</g>",
                        svg::esc(&frame.kind)
                    );
                }
                y = bottom;
            }
            Event::Note {
                first,
                last,
                placement,
                ..
            } => {
                let label = labels[i].as_ref().unwrap();
                let width = label.width + 20.0;
                let height = label.height + 12.0;
                let edge = edges(&columns[*first], 0);
                let x = match placement {
                    Placement::Left => edge.0 - width - 10.0,
                    Placement::Right => edge.1 + 10.0,
                    Placement::Over => (columns[*first].x + columns[*last].x - width) / 2.0,
                };
                let top = y + 10.0;
                bound(&mut min_x, &mut max_x, x, x + width);
                let shape = rect(
                    x,
                    top,
                    width,
                    height,
                    options.theme.radius,
                    &options.theme.surface_alt,
                    &options.theme.border,
                );
                let body = svg::label_with_options(
                    label,
                    x + width / 2.0,
                    top + height / 2.0,
                    &format!(" class=\"pele-label\" fill=\"{foreground}\""),
                    options,
                );
                let _ = write!(
                    notes,
                    "<g class=\"pele-note\" data-id=\"i{i}\">{shape}{body}</g>"
                );
                y = top + height;
            }
            Event::Message {
                from,
                to,
                arrow,
                activate,
                number,
                ..
            } => {
                let label = labels[i].as_ref().unwrap();
                let a = &columns[*from];
                let b = &columns[*to];
                let badge = number.map_or(0.0, |n| {
                    16.0_f64.max(width_for(&svg::num(n), tiny, 0, options) + 8.0)
                });
                let mut inner = String::new();
                let line_y;
                let mut created = None;
                let mut mark = String::new();
                let dotted = if arrow.dotted {
                    " stroke-dasharray=\"4 4\""
                } else {
                    ""
                };
                let up = if arrow.bottom { 1.0 } else { -1.0 };
                let trim = match arrow.head {
                    Head::Arrow => 7.0,
                    Head::Cross => 4.0,
                    _ => 0.0,
                };
                if from == to {
                    let top = y + message_gap + ((label.height - 22.0) / 2.0).max(0.0);
                    let bottom = top + 22.0;
                    let mut x0 = edges(a, 0).1;
                    let x1 = edges(a, usize::from(*activate)).1;
                    let outside = x0.max(x1) + 28.0 + badge / 2.0;
                    if let Some(number) = number {
                        mark = number_badge(*number, x0, top, badge, tiny, options);
                        x0 += badge / 2.0;
                    }
                    let begin = x0 + if arrow.at_start { trim } else { 0.0 };
                    let end = x1 + if arrow.at_end { trim } else { 0.0 };
                    let _ = write!(
                        inner,
                        "<path d=\"M{},{}H{}A6,6 0 0 1 {},{}V{}A6,6 0 0 1 {},{}H{}\"{dotted}/>",
                        svg::num(begin),
                        svg::num(top),
                        svg::num(outside - 6.0),
                        svg::num(outside),
                        svg::num(top + 6.0),
                        svg::num(bottom - 6.0),
                        svg::num(outside - 6.0),
                        svg::num(bottom),
                        svg::num(end)
                    );
                    if arrow.at_start {
                        inner.push_str(&head(arrow.head, x0, top, -1.0, up, &options.theme.line));
                    }
                    if arrow.at_end {
                        inner.push_str(&head(
                            arrow.head,
                            x1,
                            bottom,
                            -1.0,
                            -up,
                            &options.theme.line,
                        ));
                    }
                    message_labels.push_str(&svg::label_with_options(
                        label,
                        outside + 8.0 + label.width / 2.0,
                        (top + bottom) / 2.0,
                        &format!(" fill=\"{foreground}\""),
                        options,
                    ));
                    bound(&mut min_x, &mut max_x, a.x, outside + 8.0 + label.width);
                    line_y = bottom.max((top + bottom + label.height) / 2.0);
                    y = line_y;
                } else {
                    let direction = if b.x > a.x { 1.0 } else { -1.0 };
                    let a_edges = edges(a, 0);
                    let b_edges = edges(b, usize::from(*activate));
                    let mut x0 = if direction > 0.0 {
                        a_edges.1
                    } else {
                        a_edges.0
                    };
                    let creating = b.created_at == Some(i);
                    let x1 = if creating {
                        b.x - direction
                            * if figure(&model.actors[*to].kind) {
                                18.0
                            } else {
                                b.width / 2.0 + 1.0
                            }
                    } else if direction > 0.0 {
                        b_edges.0
                    } else {
                        b_edges.1
                    };
                    line_y = y
                        + message_gap
                        + if label.height > 0.0 {
                            label.height + 2.0
                        } else {
                            6.0
                        }
                        .max(if creating {
                            b.height / 2.0 - message_gap + 6.0
                        } else {
                            0.0
                        });
                    if creating {
                        created = Some((*to, line_y - b.height / 2.0));
                    }
                    if let Some(number) = number {
                        mark = number_badge(*number, x0, line_y, badge, tiny, options);
                        x0 += direction * badge / 2.0;
                    }
                    let begin = x0
                        + if arrow.at_start {
                            direction * trim
                        } else {
                            0.0
                        };
                    let end = x1 - if arrow.at_end { direction * trim } else { 0.0 };
                    let _ = write!(
                        inner,
                        "<path d=\"M{},{}H{}\"{dotted}/>",
                        svg::num(begin),
                        svg::num(line_y),
                        svg::num(end)
                    );
                    if arrow.at_start {
                        inner.push_str(&head(
                            arrow.head,
                            x0,
                            line_y,
                            -direction,
                            up * direction,
                            &options.theme.line,
                        ));
                    }
                    if arrow.at_end {
                        inner.push_str(&head(
                            arrow.head,
                            x1,
                            line_y,
                            direction,
                            up * direction,
                            &options.theme.line,
                        ));
                    }
                    let cx = (x0 + x1) / 2.0;
                    let cy = line_y - 4.0 - label.height / 2.0;
                    if columns
                        .iter()
                        .any(|c| c.x > cx - label.width / 2.0 && c.x < cx + label.width / 2.0)
                    {
                        message_labels.push_str(&rect(
                            cx - label.width / 2.0 - 3.0,
                            cy - label.height / 2.0 + 2.0,
                            label.width + 6.0,
                            (label.height - 4.0).max(0.0),
                            3.0,
                            &options.theme.background,
                            "none",
                        ));
                    }
                    message_labels.push_str(&svg::label_with_options(
                        label,
                        cx,
                        cy,
                        &format!(" fill=\"{foreground}\""),
                        options,
                    ));
                    bound(
                        &mut min_x,
                        &mut max_x,
                        cx - label.width / 2.0,
                        cx + label.width / 2.0,
                    );
                    y = line_y;
                }
                if let Some((index, top)) = created {
                    let col = &mut columns[index];
                    col.top = top;
                    col.life_start = top + col.height;
                    y = y.max(top + col.height - 6.0);
                    let depth = frames.len();
                    for (k, frame) in frames.iter_mut().enumerate() {
                        let pad = 20.0 + (depth - 1 - k) as f64 * 10.0;
                        frame.left = frame.left.min(col.x - col.width / 2.0 - pad);
                        frame.right = frame.right.max(col.x + col.width / 2.0 + pad);
                        bound(&mut min_x, &mut max_x, frame.left, frame.right);
                    }
                }
                inner.push_str(&mark);
                let _ = write!(
                    messages,
                    "<g class=\"pele-edge pele-message\" data-id=\"i{i}\">{inner}</g>"
                );
                for index in [*from, *to] {
                    if columns[index].destroyed_at == Some(i)
                        && (index == *from || from != to)
                        && columns[index].life_end.is_none()
                    {
                        let col = &mut columns[index];
                        let (cx, cy) = (col.x, line_y + 16.0);
                        col.life_end = Some(cy);
                        let _ = write!(
                            crosses,
                            "<path class=\"pele-destroy\" data-id=\"{}\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
                            svg::esc(&model.actors[index].id),
                            svg::num(cx - 6.0),
                            svg::num(cy - 6.0),
                            svg::num(cx + 6.0),
                            svg::num(cy + 6.0),
                            svg::num(cx + 6.0),
                            svg::num(cy - 6.0),
                            svg::num(cx - 6.0),
                            svg::num(cy + 6.0)
                        );
                        y = y.max(cy + 6.0);
                    }
                }
            }
        }
    }
    y += 20.0;
    for (i, col) in columns.iter_mut().enumerate() {
        while !col.activations.is_empty() {
            let (shape, _) = activation_end(col, &model.actors[i], y, options);
            activations.push(shape);
        }
    }
    let mut actors = String::new();
    let mut lifelines = String::new();
    let mut foot_height = 0.0_f64;
    for (i, col) in columns.iter().enumerate() {
        actors.push_str(&draw_actor(&model.actors[i], col, col.top, false, options));
        if mirror && col.life_end.is_none() {
            actors.push_str(&draw_actor(&model.actors[i], col, y, true, options));
            foot_height = foot_height.max(col.height);
        }
        let end = col.life_end.unwrap_or(y);
        if end > col.life_start {
            let _ = write!(
                lifelines,
                "<path class=\"pele-lifeline\" data-id=\"{}\" d=\"M{},{}V{}\"/>",
                svg::esc(&model.actors[i].id),
                svg::num(col.x),
                svg::num(col.life_start),
                svg::num(end)
            );
        }
    }
    let mut bottom = y + foot_height;
    let mut boxes = String::new();
    for (i, group) in model.groups.iter().enumerate() {
        let members = model
            .actors
            .iter()
            .enumerate()
            .filter(|(_, a)| a.group == Some(i))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let (Some(first), Some(last)) = (members.first(), members.last()) else {
            continue;
        };
        let label = label_for(&group.title, small, wrap, options);
        let left = columns[*first].x - columns[*first].width / 2.0 - 10.0;
        let right = columns[*last].x + columns[*last].width / 2.0 + 10.0;
        bound(&mut min_x, &mut max_x, left, right);
        let fill = if group.fill == "transparent" {
            &options.theme.surface_alt
        } else {
            &group.fill
        };
        boxes.push_str(&rect(
            left,
            0.0,
            right - left,
            bottom + 10.0,
            options.theme.radius,
            fill,
            &options.theme.border,
        ));
        boxes.push_str(&svg::label_with_options(
            &label,
            (left + right) / 2.0,
            7.0 + label.height / 2.0,
            &format!(" fill=\"{}\"", svg::esc(&options.theme.muted)),
            options,
        ));
    }
    if !boxes.is_empty() {
        bottom += 10.0;
    }
    let title = model
        .title
        .as_ref()
        .or(options.title.as_ref())
        .map(|value| {
            label_for(
                value,
                size,
                options
                    .max_width
                    .filter(|_| tight)
                    .map(|w| (w - 2.0 * pad).max(32.0)),
                options,
            )
        });
    let min_y = title
        .as_ref()
        .filter(|l| l.width > 0.0)
        .map_or(0.0, |l| -l.height - 12.0);
    let mut title_out = String::new();
    if let Some(label) = title
        && label.width > 0.0
    {
        let cx = (min_x + max_x) / 2.0;
        bound(
            &mut min_x,
            &mut max_x,
            cx - label.width / 2.0,
            cx + label.width / 2.0,
        );
        title_out = svg::label_with_options(
            &label,
            cx,
            min_y + label.height / 2.0,
            " class=\"pele-title\" font-weight=\"bold\"",
            options,
        );
    }
    let width = (max_x - min_x + 2.0 * pad).ceil();
    let height = (bottom - min_y + 2.0 * pad).ceil();
    if !width.is_finite() || !height.is_finite() {
        return Err(PeleError::limit("Sequence dimensions are not finite"));
    }
    let mut content = String::new();
    if let Some(title) = &model.acc_title {
        let _ = write!(content, "<title>{}</title>", svg::esc(title));
    }
    if let Some(description) = &model.acc_description {
        let _ = write!(content, "<desc>{}</desc>", svg::esc(description));
    }
    let _ = write!(
        content,
        "<g transform=\"translate({},{})\">{title_out}{boxes}{}<g fill=\"none\" stroke=\"{border}\">{lifelines}</g>{}<g class=\"pele-frames\">{frames_out}</g><g class=\"pele-edges\" fill=\"none\" stroke=\"{line}\" stroke-linecap=\"round\">{messages}</g>{}<g class=\"pele-edge-labels\" font-size=\"{}\" fill=\"{foreground}\">{message_labels}</g><g class=\"pele-notes\">{notes}</g><g class=\"pele-nodes\">{actors}</g></g>",
        svg::num(pad - min_x),
        svg::num(pad - min_y),
        rectangles.into_iter().rev().collect::<String>(),
        activations.into_iter().rev().collect::<String>(),
        if crosses.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-destroys\" fill=\"none\" stroke=\"{line}\" stroke-width=\"1.5\" stroke-linecap=\"round\">{crosses}</g>"
            )
        },
        svg::num(small)
    );
    Ok(RenderResult::new(
        "sequence",
        svg::document("sequence", width, height, options, &content),
        width,
        height,
    ))
}

fn number_badge(
    number: f64,
    x: f64,
    y: f64,
    width: f64,
    size: f64,
    options: &RenderOptions,
) -> String {
    let shape = rect(
        x - width / 2.0,
        y - 8.0,
        width,
        16.0,
        8.0,
        &options.theme.line,
        "none",
    );
    let label = svg::text(
        &svg::num(number),
        x,
        y + size * 0.35,
        size,
        "middle",
        &options.theme.background,
    );
    format!("<g class=\"pele-sequence-number\" stroke=\"none\">{shape}{label}</g>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participants_are_ordered_and_aliases_are_preserved() {
        let model=parse("sequenceDiagram\nparticipant Web as Web app\nactor User\nUser->>Web: Hello\nWeb-->>API: Fetch",100).unwrap();
        assert_eq!(
            model
                .actors
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["Web", "User", "API"]
        );
        assert_eq!(model.actors[0].description, "Web app");
        assert_eq!(model.actors[1].kind, "actor");
    }

    #[test]
    fn parses_activation_notes_numbering_and_nested_frames() {
        let source = "sequenceDiagram\nautonumber 10 2\nactor User\nparticipant API\nloop Work\nUser->>+API: Start\nNote over User,API: Working<br/>carefully\nalt Success\nAPI-->>-User: Done\nelse Error\nAPI--xUser: Failed\nend\nend";
        let model = parse(source, 100).unwrap();
        let numbers = model
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Message { number, .. } = e {
                    *number
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(numbers, [10.0, 12.0, 14.0]);
        assert!(model.events.iter().any(|e| matches!(
            e,
            Event::Note {
                first: 0,
                last: 1,
                ..
            }
        )));
        let rendered = render(source, &RenderOptions::default()).unwrap();
        for label in ["Working", "carefully", "Success", "Error", "loop", "alt"] {
            assert!(rendered.svg.contains(label), "missing {label}");
        }
        assert!(!rendered.svg.contains("var("));
        assert!(!rendered.svg.contains("foreignObject"));
    }

    #[test]
    fn a_nested_frame_sits_inside_the_frame_around_it() {
        let source = "sequenceDiagram\nA->>B: one\nloop retry\nB->>C: spawn\nalt ok\nC-->>B: ok\nelse fail\nC-->>B: died\nend\nend\nB->>A: two";
        let svg = render(source, &RenderOptions::default()).unwrap().svg;
        let frame = |kind: &str| {
            let start = svg
                .find(&format!("pele-frame-{kind}\"><rect x=\""))
                .unwrap_or_else(|| panic!("a {kind} frame in {svg}"));
            let numbers = svg[start..]
                .split('"')
                .skip(2)
                .step_by(2)
                .take(4)
                .map(|value| value.parse::<f64>().unwrap())
                .collect::<Vec<_>>();
            (
                numbers[0],
                numbers[1],
                numbers[0] + numbers[2],
                numbers[1] + numbers[3],
            )
        };
        let (outer, inner) = (frame("loop"), frame("alt"));
        assert!(
            outer.0 < inner.0 && inner.2 < outer.2,
            "{outer:?} {inner:?}"
        );
        assert!(
            outer.1 < inner.1 && inner.3 < outer.3,
            "{outer:?} {inner:?}"
        );
    }

    #[test]
    fn sequence_numbers_do_not_inherit_the_message_stroke() {
        let source = "sequenceDiagram\nautonumber\nA->>B: one\nB-->>A: two";
        let rendered = render(source, &RenderOptions::default()).unwrap();
        let doc = roxmltree::Document::parse(&rendered.svg).unwrap();
        let digits = doc
            .descendants()
            .filter(|node| node.has_tag_name("text"))
            .filter(|node| matches!(node.text(), Some("1" | "2")))
            .collect::<Vec<_>>();
        assert_eq!(digits.len(), 2);
        for digit in digits {
            let stroke = digit
                .ancestors()
                .find_map(|node| node.attribute("stroke"))
                .unwrap_or("none");
            assert_eq!(stroke, "none");
        }
    }

    #[test]
    fn malformed_and_unimplemented_constructs_are_reported() {
        for source in [
            "sequenceDiagram\nA->>B",
            "sequenceDiagram\nend",
            "sequenceDiagram\nloop Missing\nA->>B: x",
            "sequenceDiagram\ndeactivate A",
            "sequenceDiagram\nelse unexpected",
        ] {
            assert!(parse(source, 100).is_err(), "{source}");
        }
        assert!(parse("sequenceDiagram\ncreate thing A", 100).is_err());
        assert!(
            parse(
                "sequenceDiagram\nA->>B: x\ncreate participant B\nA->>B: y",
                100
            )
            .is_err()
        );
        assert!(parse("sequenceDiagram\ncreate participant C\nA->>B: x", 100).is_err());
        assert!(parse("sequenceDiagram\nA->>B: x\ndestroy B\nA->>C: y", 100).is_err());
        assert!(parse("sequenceDiagram\nparticipant A@{type: [}", 100).is_err());
        assert!(parse("sequenceDiagram\nA->>B: one\nA->>B: two", 1).is_err());
    }

    #[test]
    fn zz_sequence_fixture_renders_deterministically() {
        let source = "sequenceDiagram\nparticipant User\nparticipant Agent\nparticipant Tool\nUser->>Agent: Fix Mermaid rendering\nAgent->>Tool: Render fixture\nTool-->>Agent: SVG\nAgent-->>User: Verified result";
        let one = render(source, &RenderOptions::default()).unwrap();
        let two = render(source, &RenderOptions::default()).unwrap();
        assert_eq!(one.svg, two.svg);
        assert!(one.width > 200.0 && one.height > 100.0);
        assert_eq!(
            one.svg.matches("class=\"pele-edge pele-message\"").count(),
            4
        );
        assert_eq!(one.svg.matches("class=\"pele-lifeline\"").count(), 3);
    }

    #[test]
    fn xml_text_is_escaped_and_self_messages_have_space() {
        let result=render("sequenceDiagram\nparticipant A as <script> & \"name\"\nA->>A: <script>alert(1)</script> & test",&RenderOptions::default()).unwrap();
        assert!(!result.svg.contains("<script>"));
        assert!(result.svg.contains("&amp;"));
        assert!(result.width > 100.0);
        let result = render("sequenceDiagram\nparticipant A as <script>alert(1)</script>\nA->>B: &quot; onload=&quot;alert(1)",&RenderOptions::default()).unwrap();
        let document = roxmltree::Document::parse(&result.svg).unwrap();
        assert!(
            !document
                .descendants()
                .any(|node| node.attributes().any(|a| a.name().starts_with("on")))
        );
        assert!(result.svg.contains("onload="));
    }

    #[test]
    fn participant_boxes_are_validated() {
        let source = "sequenceDiagram\nbox LightBlue Client\nactor A as Alice\nparticipant B as Browser\nend\nA->>B: Hello";
        let model = parse(source, 100).unwrap();
        assert_eq!(model.groups[0].title, "Client");
        assert_eq!(model.groups[0].fill, "LightBlue");
        assert!(model.actors.iter().all(|a| a.group == Some(0)));
        assert!(
            render(source, &RenderOptions::default())
                .unwrap()
                .svg
                .contains("Client")
        );
        assert!(parse("sequenceDiagram\nbox Client\nA->>B: Invalid\nend", 100).is_err());
    }

    #[test]
    fn participant_metadata_shapes_and_alias_precedence_match_pele() {
        for kind in [
            "boundary",
            "control",
            "entity",
            "database",
            "collections",
            "queue",
        ] {
            let source = format!(
                "sequenceDiagram\nparticipant API@{{type: {kind}, alias: Internal}} as External\nAPI->>Client: Reply"
            );
            let model = parse(&source, 100).unwrap();
            assert_eq!(model.actors[0].kind, kind);
            assert_eq!(model.actors[0].description, "External");
            let result = render(&source, &RenderOptions::default()).unwrap();
            assert!(result.svg.contains(&format!("pele-actor-{kind}")));
            roxmltree::Document::parse(&result.svg).unwrap();
        }
        let model = parse(
            "sequenceDiagram\nactor A@{type: database, alias: Store}\nA->>B: data",
            100,
        )
        .unwrap();
        assert_eq!(model.actors[0].description, "Store");
        assert_eq!(model.actors[0].kind, "database");
    }

    #[test]
    fn sequence_configuration_controls_mirror_and_spacing() {
        let source = "sequenceDiagram\nA->>B: Request\nB-->>A: Response";
        let normal = render(source, &RenderOptions::default()).unwrap();
        let options = RenderOptions {
            config: serde_json::json!({"sequence":{"mirrorActors":false,"actorMargin":120,"messageMargin":70}}),
            ..RenderOptions::default()
        };
        let configured = render(source, &options).unwrap();
        assert!(!configured.svg.contains("pele-actor-footer"));
        assert!(configured.width > normal.width);
        assert!(configured.height > normal.height);
    }
}
