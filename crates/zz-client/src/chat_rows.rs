use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use zz_protocol::chat::{
    Answer, Ask, ChatView, ItemId, ItemKind, ItemStatus, ItemView, RequestClose, RequestId,
    RequestView, TurnId, TurnOutcome, TurnView,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unit {
    User(ItemId),
    Text(ItemId),
    Run(Run),
    Subagent(ItemId),
    Agents(Vec<ItemId>),
    Plan(ItemId),
    Notice(ItemId),
    Ask(RequestId),
    Record(RequestId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub items: Vec<ItemId>,
    pub label: RunLabel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunLabel {
    Live {
        action: &'static str,
        detail: String,
    },
    Thinking {
        heading: Option<String>,
    },
    Done {
        summary: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    Working { ms: u64 },
    Worked { ms: u64 },
    Stopped { ms: u64 },
    Failed { ms: u64 },
}

impl Fold {
    pub fn label(self) -> String {
        match self {
            Self::Working { ms } => format!("Working for {}", duration(ms)),
            Self::Worked { ms } => format!("Worked for {}", duration(ms)),
            Self::Stopped { ms } => format!("You stopped after {}", duration(ms)),
            Self::Failed { ms } => format!("Failed after {}", duration(ms)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRows {
    pub folded: Vec<Unit>,
    pub shown: Vec<Unit>,
    pub fold: Option<Fold>,
    pub activity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentState {
    Working { label: RunLabel },
    Waiting,
    Done { result: Option<String> },
    Failed { message: Option<String> },
    Stopped,
}

#[must_use]
pub fn now_ms() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

pub fn turn_rows(view: &ChatView, turn: &TurnView, now: u64) -> TurnRows {
    let live = turn.is_open();
    let standalone = unattached(view, Some(turn.id));
    let units = units(view, &turn.items, &standalone, live);
    let activity = live.then(|| live_line(view, turn)).flatten();
    let elapsed = turn.ended_at.unwrap_or(now).saturating_sub(turn.started_at);
    let fold = match &turn.outcome {
        None => Fold::Working { ms: elapsed },
        Some(TurnOutcome::Completed { .. } | TurnOutcome::Dropped) => Fold::Worked { ms: elapsed },
        Some(TurnOutcome::Cancelled) => Fold::Stopped { ms: elapsed },
        Some(TurnOutcome::Failed { .. }) => Fold::Failed { ms: elapsed },
    };
    let answer = (!live)
        .then(|| {
            let last = units
                .iter()
                .rposition(|unit| matches!(unit, Unit::Text(_)))?;
            Some(
                units[..last]
                    .iter()
                    .rposition(|unit| !matches!(unit, Unit::Text(_)))
                    .map_or(0, |index| index + 1),
            )
        })
        .flatten()
        .filter(|index| {
            units[..*index]
                .iter()
                .any(|unit| !matches!(unit, Unit::User(_)))
        });
    let Some(answer) = answer else {
        return TurnRows {
            folded: Vec::new(),
            shown: units,
            fold: live.then_some(fold),
            activity,
        };
    };
    let (mut folded, mut shown) = (Vec::new(), Vec::new());
    for (index, unit) in units.into_iter().enumerate() {
        let keep = index >= answer || still_running(view, &unit);
        if keep {
            shown.push(unit);
        } else {
            folded.push(unit);
        }
    }
    TurnRows {
        folded: joined(view, folded),
        shown,
        fold: Some(fold),
        activity,
    }
}

pub fn live_line(view: &ChatView, turn: &TurnView) -> Option<String> {
    let mut line = turn
        .activity
        .as_ref()
        .map(|activity| (activity.at, Some(activity.text.clone())));
    let mut busy = false;
    for item in turn.items.iter().filter_map(|id| view.item(id)) {
        let text = match item.kind {
            ItemKind::User | ItemKind::Agent => None,
            ItemKind::Reasoning => match last_heading(&item.body.text) {
                Some(heading) => Some(heading),
                None => continue,
            },
            kind if (kind.is_tool() || kind == ItemKind::Subagent)
                && item.status.is_open()
                && !item.background =>
            {
                busy = true;
                Some(doing(item))
            }
            _ => continue,
        };
        if line.as_ref().is_none_or(|(at, _)| item.started_at >= *at) {
            line = Some((item.started_at, text));
        }
    }
    if !busy && view.asking().is_some() {
        return Some("Waiting for your answer".to_owned());
    }
    line.and_then(|(_, text)| text)
}

fn doing(item: &ItemView) -> String {
    if let Some(activity) = item.body.activity.as_deref().map(str::trim)
        && !activity.is_empty()
    {
        return activity.to_owned();
    }
    if item.kind == ItemKind::Subagent && !item.body.title.trim().is_empty() {
        return item.body.title.clone();
    }
    let (action, detail) = live_parts(item);
    format!("{action} {detail}")
}

fn still_running(view: &ChatView, unit: &Unit) -> bool {
    let alive = |id: &ItemId| {
        let subtree = subtree(view, id);
        subtree
            .iter()
            .any(|id| view.item(id).is_some_and(|item| item.status.is_open()))
            || view
                .open_requests()
                .any(|request| request.item.as_ref().is_some_and(|id| subtree.contains(id)))
    };
    match unit {
        Unit::User(_) | Unit::Record(_) => true,
        Unit::Ask(request) => view.request(*request).is_some_and(RequestView::is_open),
        Unit::Subagent(id) => alive(id),
        Unit::Agents(ids) => ids.iter().any(alive),
        Unit::Run(run) => run.items.iter().any(alive),
        _ => false,
    }
}

pub fn background_rows(view: &ChatView) -> Vec<Unit> {
    units(
        view,
        &view.background,
        &unattached(view, None),
        background_live(view),
    )
}

pub fn background_live(view: &ChatView) -> bool {
    view.background.iter().any(|root| {
        subtree(view, root)
            .iter()
            .any(|id| view.item(id).is_some_and(|item| item.status.is_open()))
    })
}

pub fn unit_at(view: &ChatView, unit: &Unit) -> Option<u64> {
    let item = match unit {
        Unit::User(id)
        | Unit::Text(id)
        | Unit::Subagent(id)
        | Unit::Plan(id)
        | Unit::Notice(id) => id,
        Unit::Run(run) => run.items.first()?,
        Unit::Agents(ids) => ids.first()?,
        Unit::Ask(request) | Unit::Record(request) => {
            return view.request(*request).map(|request| request.opened_at);
        }
    };
    view.item(item).map(|item| item.started_at)
}

fn unattached(view: &ChatView, turn: Option<TurnId>) -> Vec<&RequestView> {
    view.requests
        .iter()
        .filter(|request| request.turn == turn)
        .filter(|request| {
            request
                .item
                .as_ref()
                .is_none_or(|item| view.item(item).is_none())
        })
        .collect()
}

pub fn agent_rows(view: &ChatView, agent: &ItemId) -> Vec<Unit> {
    let Some(item) = view.item(agent) else {
        return Vec::new();
    };
    units(view, &item.children, &[], item.status.is_open())
}

pub fn agent_state(view: &ChatView, agent: &ItemId) -> Option<AgentState> {
    let item = view.item(agent)?;
    let subtree = subtree(view, agent);
    if view
        .open_requests()
        .any(|request| request.item.as_ref().is_some_and(|id| subtree.contains(id)))
    {
        return Some(AgentState::Waiting);
    }
    Some(match &item.status {
        ItemStatus::Running => AgentState::Working {
            label: match agent_rows(view, agent).pop() {
                Some(Unit::Run(run)) => run.label,
                _ => RunLabel::Thinking { heading: None },
            },
        },
        ItemStatus::Completed => AgentState::Done {
            result: last_text(view, &item.children)
                .or_else(|| (!item.body.text.is_empty()).then(|| item.body.text.clone())),
        },
        ItemStatus::Failed { message } => AgentState::Failed {
            message: message.clone(),
        },
        ItemStatus::Declined | ItemStatus::Interrupted => AgentState::Stopped,
    })
}

pub fn agent_ms(view: &ChatView, agent: &ItemId, now: u64) -> Option<u64> {
    let item = view.item(agent)?;
    Some(item.ended_at.unwrap_or(now).saturating_sub(item.started_at))
}

pub fn agent_prompt<'a>(view: &'a ChatView, agent: &ItemId) -> Option<&'a str> {
    view.item(agent)?
        .body
        .target
        .as_deref()
        .filter(|target| !target.is_empty())
}

pub fn answered(request: &RequestView) -> Vec<(String, String)> {
    let (Ask::Questions { questions }, Some(close)) = (&request.ask, &request.close) else {
        return Vec::new();
    };
    let answers = match close {
        RequestClose::Answered {
            answer: Answer::Questions { answers },
        } => answers.as_slice(),
        _ => &[],
    };
    questions
        .iter()
        .map(|question| {
            let label = Some(question.question.clone())
                .filter(|text| !text.is_empty())
                .or_else(|| question.header.clone())
                .unwrap_or_default();
            let value = answers
                .iter()
                .find(|answer| answer.question == question.id)
                .or_else(|| {
                    answers
                        .iter()
                        .find(|answer| answer.question == question.question)
                })
                .map(|answer| answer.answers.join(", "))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "no answer".to_owned());
            (label, value)
        })
        .collect()
}

pub fn run_summary(view: &ChatView, items: &[ItemId]) -> String {
    let tools = items
        .iter()
        .filter_map(|id| view.item(id))
        .filter(|item| item.kind.is_tool())
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return "Thought".to_owned();
    }
    let mut parts = Vec::new();
    for (kinds, one, many) in CATEGORIES {
        let of_kind = tools
            .iter()
            .filter(|item| kinds.contains(&item.kind))
            .collect::<Vec<_>>();
        let Some(first) = of_kind.first() else {
            continue;
        };
        let count = if kinds.iter().any(|kind| FILE_KINDS.contains(kind)) {
            let named = of_kind
                .iter()
                .filter_map(|item| item.body.target.as_deref())
                .collect::<BTreeSet<_>>();
            named.len()
                + of_kind
                    .iter()
                    .filter(|item| item.body.target.is_none())
                    .count()
        } else {
            of_kind.len()
        };
        let part = match (first.body.target.as_deref(), *one) {
            (Some(target), Some(verb)) if count == 1 => format!("{verb} {}", short(target)),
            _ => many(count),
        };
        parts.push(part);
    }
    let mut summary = parts.join(", ");
    if let Some(first) = summary.get(..1) {
        summary = format!("{}{}", first.to_uppercase(), &summary[1..]);
    }
    summary.push_str(&outcomes(&tools));
    summary
}

fn outcomes(tools: &[&ItemView]) -> String {
    let mut suffix = String::new();
    let failed = tools
        .iter()
        .filter(|item| matches!(item.status, ItemStatus::Failed { .. }))
        .count();
    let denied = tools
        .iter()
        .filter(|item| item.status == ItemStatus::Declined)
        .count();
    if failed > 0 {
        let _ = write!(suffix, " · {failed} failed");
    }
    if denied > 0 {
        let _ = write!(suffix, " · {denied} denied");
    }
    suffix
}

pub fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

type Category = (
    &'static [ItemKind],
    Option<&'static str>,
    fn(usize) -> String,
);

const FILE_KINDS: [ItemKind; 2] = [ItemKind::Edit, ItemKind::Read];

const CATEGORIES: &[Category] = &[
    (&[ItemKind::Edit], Some("edited"), |n| {
        plural(n, "edited a file", "edited", "files")
    }),
    (&[ItemKind::Read], Some("read"), |n| {
        plural(n, "read a file", "read", "files")
    }),
    (&[ItemKind::Search], Some("searched for"), |n| {
        plural(n, "searched once", "searched", "times")
    }),
    (&[ItemKind::Execute], Some("ran"), |n| {
        plural(n, "ran a command", "ran", "commands")
    }),
    (&[ItemKind::Fetch, ItemKind::Web], None, |n| {
        plural(n, "looked something up online", "looked up", "pages")
    }),
    (&[ItemKind::Mcp, ItemKind::Tool], None, |n| {
        plural(n, "used a tool", "used", "tools")
    }),
];

fn plural(n: usize, one: &str, verb: &str, noun: &str) -> String {
    if n == 1 {
        one.to_owned()
    } else {
        format!("{verb} {n} {noun}")
    }
}

fn short(target: &str) -> String {
    if target.contains(' ') {
        return target.to_owned();
    }
    Path::new(target)
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| target.to_owned(), str::to_owned)
}

fn units(view: &ChatView, ids: &[ItemId], standalone: &[&RequestView], live: bool) -> Vec<Unit> {
    let mut entries = ids
        .iter()
        .filter_map(|id| view.item(id))
        .map(|item| (item.started_at, Entry::Item(item)))
        .collect::<Vec<_>>();
    entries.extend(
        standalone
            .iter()
            .map(|request| (request.opened_at, Entry::Request(request))),
    );
    entries.sort_by_key(|(at, _)| *at);
    let mut out = Vec::new();
    let mut run = Vec::new();
    for (_, entry) in entries {
        match entry {
            Entry::Request(request) => {
                if let Some(unit) = request_unit(request) {
                    flush(view, &mut run, &mut out, false);
                    out.push(unit);
                }
            }
            Entry::Item(item) => {
                match item.kind {
                    kind if kind.is_tool() || kind == ItemKind::Reasoning => {
                        if !questioned(view, &item.id) {
                            run.push(item.id.clone());
                        }
                    }
                    ItemKind::User => {
                        flush(view, &mut run, &mut out, false);
                        out.push(Unit::User(item.id.clone()));
                    }
                    ItemKind::Agent => {
                        flush(view, &mut run, &mut out, false);
                        out.push(Unit::Text(item.id.clone()));
                    }
                    ItemKind::Subagent => {
                        flush(view, &mut run, &mut out, false);
                        out.push(Unit::Subagent(item.id.clone()));
                    }
                    ItemKind::Plan => {
                        flush(view, &mut run, &mut out, false);
                        out.push(Unit::Plan(item.id.clone()));
                    }
                    _ => {
                        flush(view, &mut run, &mut out, false);
                        out.push(Unit::Notice(item.id.clone()));
                    }
                }
                let attached = view
                    .requests
                    .iter()
                    .filter(|request| request.item.as_ref() == Some(&item.id))
                    .filter_map(request_unit)
                    .collect::<Vec<_>>();
                if !attached.is_empty() {
                    flush(view, &mut run, &mut out, false);
                    out.extend(attached);
                }
            }
        }
    }
    flush(view, &mut run, &mut out, live);
    group(out)
}

fn group(units: Vec<Unit>) -> Vec<Unit> {
    let mut out = Vec::with_capacity(units.len());
    for unit in units {
        let Unit::Subagent(id) = unit else {
            out.push(unit);
            continue;
        };
        match out.pop() {
            Some(Unit::Subagent(first)) => out.push(Unit::Agents(vec![first, id])),
            Some(Unit::Agents(mut ids)) => {
                ids.push(id);
                out.push(Unit::Agents(ids));
            }
            last => {
                out.extend(last);
                out.push(Unit::Subagent(id));
            }
        }
    }
    out
}

enum Entry<'a> {
    Item(&'a ItemView),
    Request(&'a RequestView),
}

fn questioned(view: &ChatView, id: &ItemId) -> bool {
    view.requests.iter().any(|request| {
        request.item.as_ref() == Some(id) && matches!(request.ask, Ask::Questions { .. })
    })
}

fn request_unit(request: &RequestView) -> Option<Unit> {
    if request.is_open() {
        Some(Unit::Ask(request.id))
    } else if matches!(request.ask, Ask::Questions { .. })
        && !matches!(request.close, Some(RequestClose::Withdrawn))
    {
        Some(Unit::Record(request.id))
    } else {
        None
    }
}

fn flush(view: &ChatView, run: &mut Vec<ItemId>, out: &mut Vec<Unit>, tail: bool) {
    if !run.is_empty() {
        push_run(view, std::mem::take(run), out, tail);
    }
}

fn joined(view: &ChatView, units: Vec<Unit>) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::with_capacity(units.len());
    for unit in units {
        match (out.last_mut(), unit) {
            (Some(Unit::Run(held)), Unit::Run(next)) => {
                held.items.extend(next.items);
                held.label = done_label(view, &held.items);
            }
            (_, unit) => out.push(unit),
        }
    }
    out
}

fn done_label(view: &ChatView, items: &[ItemId]) -> RunLabel {
    RunLabel::Done {
        summary: match summarized(view, items) {
            Some((summary, tools)) => format!("{summary}{}", outcomes(&tools)),
            None => run_summary(view, items),
        },
    }
}

fn summarized<'a>(view: &'a ChatView, items: &[ItemId]) -> Option<(&'a str, Vec<&'a ItemView>)> {
    let tools = items
        .iter()
        .filter_map(|id| view.item(id))
        .filter(|item| item.kind.is_tool())
        .collect::<Vec<_>>();
    let summary = tools.first()?.summary.as_ref()?;
    tools
        .iter()
        .all(|item| item.summary.as_ref() == Some(summary))
        .then_some((summary.text.as_str(), tools))
}

fn push_run(view: &ChatView, items: Vec<ItemId>, out: &mut Vec<Unit>, tail: bool) {
    if !tail && !run_shows(view, &items) {
        return;
    }
    let running = items
        .iter()
        .rev()
        .filter_map(|id| view.item(id))
        .find(|item| item.kind.is_tool() && item.status.is_open());
    let label = match running {
        Some(item) => live_label(item),
        None if tail && summarized(view, &items).is_none() => RunLabel::Thinking {
            heading: items
                .iter()
                .rev()
                .filter_map(|id| view.item(id))
                .find(|item| item.kind == ItemKind::Reasoning)
                .and_then(|item| heading(&item.body.text)),
        },
        None => done_label(view, &items),
    };
    out.push(Unit::Run(Run { items, label }));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepLabel {
    pub label: String,
    pub detail: Option<String>,
}

pub fn step_label(view: &ChatView, item: &ItemView) -> StepLabel {
    let named = item
        .body
        .target
        .clone()
        .filter(|target| !target.is_empty())
        .unwrap_or_else(|| item.body.title.clone());
    let own = item.summary.as_ref().filter(|summary| {
        view.items
            .values()
            .filter(|other| {
                other
                    .summary
                    .as_ref()
                    .is_some_and(|held| held.batch == summary.batch)
            })
            .count()
            == 1
    });
    let (label, detail) = match own {
        Some(summary) => (
            summary.text.clone(),
            Some(named).filter(|named| !named.is_empty() && *named != summary.text),
        ),
        None => (named, None),
    };
    StepLabel {
        label,
        detail: failure(item).or(detail),
    }
}

fn failure(item: &ItemView) -> Option<String> {
    let ItemStatus::Failed { message } = &item.status else {
        return None;
    };
    let exit = item
        .body
        .exit_code
        .filter(|_| item.kind == ItemKind::Execute)
        .map(|code| format!("exit {code}"));
    let reason = message.as_deref().and_then(|message| {
        message
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !restates_exit(line))
            .map(str::to_owned)
    });
    match (exit, reason) {
        (Some(exit), Some(reason)) => Some(format!("{exit} · {reason}")),
        (exit, reason) => exit.or(reason),
    }
}

fn restates_exit(line: &str) -> bool {
    line.get(..9)
        .filter(|head| head.eq_ignore_ascii_case("exit code"))
        .is_some_and(|_| line[9..].trim().parse::<i64>().is_ok())
}

fn live_label(item: &ItemView) -> RunLabel {
    let (action, detail) = live_parts(item);
    RunLabel::Live { action, detail }
}

fn live_parts(item: &ItemView) -> (&'static str, String) {
    let target = item
        .body
        .target
        .clone()
        .unwrap_or_else(|| item.body.title.clone());
    let action = match item.kind {
        ItemKind::Read => "Reading",
        ItemKind::Edit => "Editing",
        ItemKind::Search => "Searching for",
        ItemKind::Execute => "Running",
        ItemKind::Fetch => "Fetching",
        ItemKind::Web => "Searching the web for",
        _ => "Using",
    };
    (action, target)
}

pub fn run_shows(view: &ChatView, items: &[ItemId]) -> bool {
    items
        .iter()
        .filter_map(|id| view.item(id))
        .any(|item| item.kind.is_tool() || reasoning_body(&item.body.text).is_some())
}

pub fn reasoning_body(text: &str) -> Option<String> {
    let mut rest = text.trim_start();
    while let Some(line) = rest.lines().next()
        && heading_line(line).is_some()
    {
        rest = rest[line.len()..].trim_start();
    }
    (!rest.trim().is_empty()).then(|| rest.trim_end().to_owned())
}

fn heading_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if let Some(inner) = line
        .strip_prefix("**")
        .and_then(|rest| rest.strip_suffix("**"))
    {
        return (!inner.trim().is_empty() && !inner.contains("**")).then_some(inner);
    }
    let marks = line.len() - line.trim_start_matches('#').len();
    let rest = &line[marks..];
    ((1..=6).contains(&marks) && (rest.is_empty() || rest.starts_with([' ', '\t']))).then_some(rest)
}

fn heading(text: &str) -> Option<String> {
    text.lines().find_map(heading_of)
}

fn last_heading(text: &str) -> Option<String> {
    text.lines().rev().find_map(heading_of)
}

fn heading_of(line: &str) -> Option<String> {
    let line = line.trim();
    let inner = line
        .strip_prefix("**")
        .and_then(|rest| rest.split("**").next())
        .or_else(|| {
            line.strip_prefix('#')
                .map(|rest| rest.trim_start_matches('#'))
        })?;
    let inner = inner.trim().trim_end_matches('.');
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn last_text(view: &ChatView, ids: &[ItemId]) -> Option<String> {
    ids.iter()
        .rev()
        .filter_map(|id| view.item(id))
        .find(|item| item.kind == ItemKind::Agent && !item.body.text.is_empty())
        .map(|item| item.body.text.clone())
}

fn subtree(view: &ChatView, root: &ItemId) -> Vec<ItemId> {
    let mut found = vec![root.clone()];
    let mut index = 0;
    while index < found.len() {
        if let Some(item) = view.item(&found[index]) {
            found.extend(item.children.iter().cloned());
        }
        index += 1;
    }
    found
}
