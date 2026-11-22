use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde_json::Value;
use zz_protocol::AgentProvider;
use zz_protocol::agent_stream::AgentStreamPayload;
use zz_protocol::chat::{
    ChatEvent, ChatLog, ChatView, ItemId, ItemKind, ItemStatus, ItemView, RequestView, TurnView,
};

use super::journal::AgentJournal;

pub(crate) struct Tape {
    pub(crate) name: &'static str,
    pub(crate) provider: AgentProvider,
    pub(crate) session: String,
    pub(crate) lines: Vec<Line>,
    journal: Vec<Value>,
    reload: fn(&str, &[Value]) -> Vec<ChatEvent>,
}

pub(crate) struct Line {
    pub(crate) label: String,
    pub(crate) payloads: Vec<AgentStreamPayload>,
    held: usize,
    tail: Option<Value>,
}

impl Tape {
    pub(crate) fn events(&self) -> Vec<ChatEvent> {
        self.lines
            .iter()
            .flat_map(|line| line.payloads.iter())
            .filter_map(chat_event)
            .cloned()
            .collect()
    }

    pub(crate) fn journal_at(&self, line: usize) -> Vec<Value> {
        let line = &self.lines[line];
        let mut held = self.journal[..line.held.saturating_sub(1)].to_vec();
        held.extend(line.tail.clone());
        held
    }

    fn reload(&self, line: usize) -> Vec<ChatEvent> {
        (self.reload)(&self.session, &self.journal_at(line))
    }
}

pub(crate) struct Taping {
    name: &'static str,
    provider: AgentProvider,
    session: String,
    journal: Arc<AgentJournal>,
    lines: Vec<Line>,
    held: Vec<Value>,
    _directory: tempfile::TempDir,
}

impl Taping {
    pub(crate) fn new(name: &'static str, provider: AgentProvider, session: &str) -> Self {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        Self {
            name,
            provider,
            session: session.to_owned(),
            journal,
            lines: Vec::new(),
            held: Vec::new(),
            _directory: directory,
        }
    }

    pub(crate) fn journal(&self) -> Arc<AgentJournal> {
        Arc::clone(&self.journal)
    }

    pub(crate) fn line(&mut self, entry: Option<&Value>, payloads: Vec<AgentStreamPayload>) {
        let held = self
            .journal
            .held_for(self.provider, &self.session)
            .expect("read the journal");
        let settled = self.held.len().saturating_sub(1);
        assert!(
            held.len() >= settled && held[..settled] == self.held[..settled],
            "{}: the journal rewrote a settled record",
            self.name
        );
        self.lines.push(Line {
            label: entry.map_or_else(|| "setup".to_owned(), label),
            payloads,
            held: held.len(),
            tail: held.last().cloned(),
        });
        self.held = held;
    }

    pub(crate) fn finish(
        self,
        emitted: &[ChatEvent],
        reload: fn(&str, &[Value]) -> Vec<ChatEvent>,
    ) -> Tape {
        let streamed = self
            .lines
            .iter()
            .flat_map(|line| line.payloads.iter())
            .filter_map(chat_event)
            .collect::<Vec<_>>();
        assert!(
            streamed.iter().copied().eq(emitted),
            "{}: the payload stream lost chat events the shadow emitted",
            self.name
        );
        Tape {
            name: self.name,
            provider: self.provider,
            session: self.session,
            lines: self.lines,
            journal: self.held,
            reload,
        }
    }
}

fn label(entry: &Value) -> String {
    let message = &entry["msg"];
    [
        &entry["step"],
        &entry["dir"],
        &message["type"],
        &message["subtype"],
        &message["request"]["subtype"],
        &message["method"],
    ]
    .into_iter()
    .filter_map(Value::as_str)
    .collect::<Vec<_>>()
    .join(" ")
}

pub(crate) fn tapes() -> &'static [Tape] {
    static TAPES: OnceLock<Vec<Tape>> = OnceLock::new();
    TAPES.get_or_init(|| {
        let mut tapes = super::claude::tapes();
        tapes.extend(super::codex::tapes());
        tapes
    })
}

pub(crate) fn chat_event(payload: &AgentStreamPayload) -> Option<&ChatEvent> {
    match payload {
        AgentStreamPayload::Chat { event } => Some(event),
        _ => None,
    }
}

pub(crate) fn fold<'a>(events: impl IntoIterator<Item = &'a ChatEvent>) -> ChatView {
    let mut view = ChatView::default();
    for event in events {
        let _ = view.apply(event);
    }
    view
}

pub(crate) fn kind_of<T: Debug>(value: &T) -> String {
    format!("{value:?}")
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect()
}

pub(crate) fn payload_kind(payload: &AgentStreamPayload) -> String {
    chat_event(payload).map_or_else(|| kind_of(payload), kind_of)
}

pub(crate) fn short<T: Debug>(value: &T) -> String {
    let text = format!("{value:?}");
    if text.chars().count() <= 240 {
        return text;
    }
    let mut clipped = text.chars().take(240).collect::<String>();
    clipped.push('…');
    clipped
}

pub(crate) struct Difference {
    pub(crate) field: String,
    pub(crate) at: String,
    pub(crate) expected: String,
    pub(crate) got: String,
}

impl Difference {
    pub(crate) fn describe(&self) -> String {
        format!("{}: expected {} got {}", self.at, self.expected, self.got)
    }
}

fn same<T: PartialEq + Debug>(field: &str, at: &str, expected: &T, got: &T) -> Option<Difference> {
    (expected != got).then(|| Difference {
        field: field.to_owned(),
        at: at.to_owned(),
        expected: short(expected),
        got: short(got),
    })
}

fn presence(field: &str, at: &str, expected: &str, got: &str) -> Difference {
    Difference {
        field: field.to_owned(),
        at: at.to_owned(),
        expected: expected.to_owned(),
        got: got.to_owned(),
    }
}

macro_rules! first_field {
    ($group:expr, $at:expr, $expected:expr, $got:expr, [$($head:ident $(. $tail:ident)*),+]) => {
        None$(.or_else(|| {
            let name = concat!(stringify!($head) $(, ".", stringify!($tail))*);
            same(
                &format!("{}.{name}", $group),
                &format!("{}.{name}", $at),
                &$expected.$head$(.$tail)*,
                &$got.$head$(.$tail)*,
            )
        }))+
    };
}

fn within(all: &[ItemId], kept: &[ItemId]) -> bool {
    let mut all = all.iter();
    kept.iter().all(|id| all.any(|other| other == id))
}

fn turn_difference(
    at: &str,
    expected: &TurnView,
    got: &TurnView,
    subsequence: bool,
) -> Option<Difference> {
    first_field!(
        "turns[]",
        at,
        expected,
        got,
        [id, by, started_at, ended_at, outcome, stopping]
    )
    .or_else(|| {
        if subsequence && within(&expected.items, &got.items) {
            return None;
        }
        first_field!("turns[]", at, expected, got, [items])
    })
    .or_else(|| first_field!("turns[]", at, expected, got, [activity]))
}

fn item_difference(
    group: &str,
    expected: &ItemView,
    got: &ItemView,
    batches: bool,
) -> Option<Difference> {
    let (field, here) = (
        format!("{group}[]"),
        format!("{group}[{}]", expected.id.as_str()),
    );
    first_field!(
        field,
        here,
        expected,
        got,
        [
            kind,
            body.title,
            body.target,
            body.text,
            body.steps,
            body.exit_code,
            body.images,
            body.activity,
            status,
            turn,
            parent,
            children,
            background,
            started_at,
            ended_at
        ]
    )
    .or_else(|| {
        let summary = |item: &ItemView| {
            item.summary
                .as_ref()
                .map(|summary| (batches.then_some(summary.batch), summary.text.clone()))
        };
        same(
            &format!("{field}.summary"),
            &format!("{here}.summary"),
            &summary(expected),
            &summary(got),
        )
    })
}

fn request_difference(
    group: &str,
    expected: &RequestView,
    got: &RequestView,
) -> Option<Difference> {
    first_field!(
        format!("{group}[]"),
        format!("{group}[{}]", expected.id.0),
        expected,
        got,
        [id, turn, item, ask, blocking, opened_at, close, closed_at]
    )
}

fn in_order(view: &ChatView) -> Vec<&ItemView> {
    let mut items = view.items.values().collect::<Vec<_>>();
    items.sort_by(|a, b| (a.started_at, &a.id).cmp(&(b.started_at, &b.id)));
    items
}

fn items_difference(expected: &ChatView, got: &ChatView) -> Option<Difference> {
    in_order(expected)
        .into_iter()
        .find_map(|item| match got.item(&item.id) {
            None => Some(presence(
                "items[] missing",
                &format!("items[{}]", item.id.as_str()),
                &short(&(item.kind, &item.status, &item.body.title)),
                "absent",
            )),
            Some(other) => item_difference("items", item, other, true),
        })
        .or_else(|| {
            in_order(got).into_iter().find_map(|item| {
                expected.item(&item.id).is_none().then(|| {
                    presence(
                        "items[] extra",
                        &format!("items[{}]", item.id.as_str()),
                        "absent",
                        &short(&(item.kind, &item.status, &item.body.title)),
                    )
                })
            })
        })
}

pub(crate) fn difference(expected: &ChatView, got: &ChatView) -> Option<Difference> {
    if expected == got {
        return None;
    }
    same("session", "session", &expected.session, &got.session)
        .or_else(|| {
            same(
                "last_session",
                "last_session",
                &expected.last_session,
                &got.last_session,
            )
        })
        .or_else(|| {
            expected
                .turns
                .iter()
                .zip(&got.turns)
                .enumerate()
                .find_map(|(index, (turn, other))| {
                    turn_difference(&format!("turns[{index}]"), turn, other, false)
                })
        })
        .or_else(|| {
            same(
                "turns.len",
                "turns",
                &expected
                    .turns
                    .iter()
                    .map(|turn| turn.id)
                    .collect::<Vec<_>>(),
                &got.turns.iter().map(|turn| turn.id).collect::<Vec<_>>(),
            )
        })
        .or_else(|| items_difference(expected, got))
        .or_else(|| {
            same(
                "background",
                "background",
                &expected.background,
                &got.background,
            )
        })
        .or_else(|| {
            expected
                .requests
                .iter()
                .zip(&got.requests)
                .find_map(|(request, other)| request_difference("requests", request, other))
        })
        .or_else(|| {
            same(
                "requests.len",
                "requests",
                &expected
                    .requests
                    .iter()
                    .map(|request| request.id)
                    .collect::<Vec<_>>(),
                &got.requests
                    .iter()
                    .map(|request| request.id)
                    .collect::<Vec<_>>(),
            )
        })
        .or_else(|| same("queue", "queue", &expected.queue, &got.queue))
        .or_else(|| same("usage", "usage", &expected.usage, &got.usage))
        .or_else(|| same("skipped", "skipped", &expected.skipped, &got.skipped))
        .or_else(|| {
            Some(presence(
                "private fields",
                "pictured or batches",
                "equal public fields",
                "a different private field",
            ))
        })
}

pub(crate) fn sub_view_difference(straight: &ChatView, rebuilt: &ChatView) -> Option<Difference> {
    (straight.phase() != rebuilt.phase())
        .then(|| Difference {
            field: format!("phase {:?} read as {:?}", straight.phase(), rebuilt.phase()),
            at: "phase".to_owned(),
            expected: short(&straight.phase()),
            got: short(&rebuilt.phase()),
        })
        .or_else(|| same("session", "session", &straight.session, &rebuilt.session))
        .or_else(|| {
            same(
                "last_session",
                "last_session",
                &straight.last_session,
                &rebuilt.last_session,
            )
        })
        .or_else(|| match (straight.running(), rebuilt.running()) {
            (Some(turn), Some(other)) => turn_difference("running", turn, other, true),
            (turn, other) => same(
                "running",
                "running",
                &turn.map(|turn| turn.id),
                &other.map(|turn| turn.id),
            ),
        })
        .or_else(|| {
            straight
                .open_requests()
                .find_map(|request| match rebuilt.request(request.id) {
                    None => Some(presence(
                        "open requests[] missing",
                        &format!("requests[{}]", request.id.0),
                        &short(&(&request.item, &request.ask)),
                        "absent",
                    )),
                    Some(other) => request_difference("open requests", request, other),
                })
        })
        .or_else(|| {
            in_order(straight)
                .into_iter()
                .filter(|item| item.status.is_open())
                .find_map(|item| match rebuilt.item(&item.id) {
                    None => Some(presence(
                        "open items[] missing",
                        &format!("items[{}]", item.id.as_str()),
                        &short(&(item.kind, &item.body.title, item.turn, item.background)),
                        "absent",
                    )),
                    Some(other) => item_difference("open items", item, other, false),
                })
        })
        .or_else(|| same("queue", "queue", &straight.queue, &rebuilt.queue))
        .or_else(|| same("usage", "usage", &straight.usage, &rebuilt.usage))
        .or_else(|| {
            rebuilt.turns.iter().find_map(|turn| {
                match straight.turns.iter().find(|other| other.id == turn.id) {
                    None => Some(presence(
                        "turns[] extra",
                        &format!("turns[{}]", turn.id.0),
                        "absent",
                        &short(&turn.outcome),
                    )),
                    Some(other) => {
                        turn_difference(&format!("turns[{}]", turn.id.0), other, turn, true)
                    }
                }
            })
        })
        .or_else(|| {
            in_order(rebuilt)
                .into_iter()
                .find_map(|item| match straight.item(&item.id) {
                    None => Some(presence(
                        "items[] extra",
                        &format!("items[{}]", item.id.as_str()),
                        "absent",
                        &short(&(item.kind, &item.body.title)),
                    )),
                    Some(other) => item_difference("items", other, item, false),
                })
        })
        .or_else(|| {
            rebuilt
                .requests
                .iter()
                .find_map(|request| match straight.request(request.id) {
                    None => Some(presence(
                        "requests[] extra",
                        &format!("requests[{}]", request.id.0),
                        "absent",
                        &short(&request.ask),
                    )),
                    Some(other) => request_difference("requests", other, request),
                })
        })
        .or_else(|| {
            (!within(&straight.background, &rebuilt.background)).then(|| Difference {
                field: "background".to_owned(),
                at: "background".to_owned(),
                expected: format!("an ordered subset of {}", short(&straight.background)),
                got: short(&rebuilt.background),
            })
        })
}

struct Group {
    count: usize,
    kinds: BTreeMap<String, usize>,
    rank: (usize, usize),
    example: String,
}

pub(crate) struct Findings {
    sweep: &'static str,
    started: Instant,
    checked: usize,
    groups: BTreeMap<String, Group>,
}

impl Findings {
    pub(crate) fn new(sweep: &'static str) -> Self {
        Self {
            sweep,
            started: Instant::now(),
            checked: 0,
            groups: BTreeMap::new(),
        }
    }

    pub(crate) fn checked(&mut self) {
        self.checked += 1;
    }

    pub(crate) fn add(
        &mut self,
        signature: String,
        kind: String,
        rank: (usize, usize),
        example: impl FnOnce() -> String,
    ) {
        let group = self.groups.entry(signature).or_insert(Group {
            count: 0,
            kinds: BTreeMap::new(),
            rank: (usize::MAX, usize::MAX),
            example: String::new(),
        });
        group.count += 1;
        *group.kinds.entry(kind).or_default() += 1;
        if rank < group.rank {
            group.rank = rank;
            group.example = example();
        }
    }

    pub(crate) fn finish(self, expected: &[&str]) {
        let summary = format!(
            "{}: {} checks in {:.2?}, {} signatures",
            self.sweep,
            self.checked,
            self.started.elapsed(),
            self.groups.len()
        );
        eprintln!("{summary}");
        let gone = expected
            .iter()
            .filter(|signature| !self.groups.contains_key(**signature))
            .map(|signature| format!("gone: {signature}"))
            .collect::<Vec<_>>();
        let report = self
            .groups
            .iter()
            .filter(|(signature, _)| !expected.contains(&signature.as_str()))
            .map(|(signature, group)| {
                let kinds = group
                    .kinds
                    .iter()
                    .map(|(kind, count)| format!("{kind} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{} x {signature} | cut at: {kinds}\n    smallest: {}",
                    group.count, group.example
                )
            })
            .chain(gone)
            .collect::<Vec<_>>();
        assert!(report.is_empty(), "{summary}\n{}", report.join("\n"));
    }
}

pub(crate) struct Cut<'a> {
    pub(crate) name: &'a str,
    pub(crate) events: &'a [ChatEvent],
    pub(crate) straight: &'a ChatView,
    pub(crate) log: ChatLog,
    pub(crate) live: ChatView,
    pub(crate) k: usize,
}

impl<'a> Cut<'a> {
    pub(crate) fn new(name: &'a str, events: &'a [ChatEvent], straight: &'a ChatView) -> Self {
        Self {
            name,
            events,
            straight,
            log: ChatLog::default(),
            live: ChatView::default(),
            k: 0,
        }
    }

    pub(crate) fn advance(&mut self, findings: &mut Findings) {
        let (k, event) = (self.k, &self.events[self.k]);
        self.k += 1;
        self.log.push(event);
        if let Err(skip) = self.live.apply(event) {
            let name = self.name;
            findings.add(
                format!("A | straight fold | skipped {skip:?}"),
                kind_of(event),
                (self.events.len(), k),
                || format!("{name} k={k}: {}", short(event)),
            );
        }
    }

    pub(crate) fn check(&self, findings: &mut Findings, continued: bool) {
        let (name, k, events) = (self.name, self.k, self.events);
        let cut = k.checked_sub(1).map(|at| &events[at]);
        let at = cut.map_or_else(|| "start".to_owned(), kind_of);
        let rank = (events.len(), k);
        let example = |diff: &Difference| {
            format!(
                "{name} k={k}/{} after {}: {}",
                events.len(),
                cut.map_or_else(|| "start".to_owned(), short),
                diff.describe()
            )
        };
        let mut compacted = fold(std::iter::once(&ChatEvent::Restart).chain(self.log.events()));
        findings.checked();
        if let Some(diff) = difference(&self.live, &compacted) {
            findings.add(
                format!("A | at the cut | {}", diff.field),
                at.clone(),
                rank,
                || example(&diff),
            );
        }
        let logged = self.log.events().collect::<Vec<_>>();
        let mut spine = ChatView::default();
        findings.checked();
        for kept in self.log.in_effect() {
            if let Err(skip) = spine.apply(logged[kept]) {
                findings.add(
                    format!("A | in effect | skipped {skip:?}"),
                    kind_of(logged[kept]),
                    rank,
                    || format!("{name} k={k}: {}", short(logged[kept])),
                );
            }
        }
        if let Some(diff) = sub_view_difference(&self.live, &spine) {
            findings.add(
                format!("A | in effect | {}", diff.field),
                at.clone(),
                rank,
                || example(&diff),
            );
        }
        if !continued {
            return;
        }
        for event in &events[k..] {
            let _ = compacted.apply(event);
        }
        findings.checked();
        if let Some(diff) = difference(self.straight, &compacted) {
            findings.add(format!("A | continued | {}", diff.field), at, rank, || {
                example(&diff)
            });
        }
    }
}

#[test]
fn every_cut_of_a_recording_compacts_to_the_view_a_live_client_holds() {
    let mut findings = Findings::new("A late attach");
    for tape in tapes() {
        let events = tape.events();
        let straight = fold(&events);
        let mut cut = Cut::new(tape.name, &events, &straight);
        cut.check(&mut findings, true);
        while cut.k < events.len() {
            cut.advance(&mut findings);
            cut.check(&mut findings, true);
        }
    }
    findings.finish(&[]);
}

#[derive(Debug, PartialEq)]
struct Row {
    kind: ItemKind,
    title: String,
    target: Option<String>,
    text: String,
    status: ItemStatus,
    children: Vec<Row>,
}

fn rows(view: &ChatView, ids: &[ItemId], settle: bool) -> Vec<Row> {
    ids.iter()
        .filter_map(|id| view.item(id))
        .map(|item| Row {
            kind: item.kind,
            title: item.body.title.clone(),
            target: item.body.target.clone(),
            text: item.body.text.clone(),
            status: match &item.status {
                ItemStatus::Running if settle && item.kind.is_text() => ItemStatus::Completed,
                ItemStatus::Running if settle => ItemStatus::Interrupted,
                status => status.clone(),
            },
            children: rows(view, &item.children, settle),
        })
        .collect()
}

fn top_level(view: &ChatView, events: &[ChatEvent]) -> Vec<ItemId> {
    let mut ids = Vec::<ItemId>::new();
    for event in events {
        if let ChatEvent::ItemStarted {
            item, parent: None, ..
        } = event
            && view.item(item).is_some_and(|view| view.parent.is_none())
            && !ids.contains(item)
        {
            ids.push(item.clone());
        }
    }
    ids
}

fn rows_difference(at: &str, expected: &[Row], got: &[Row]) -> Option<Difference> {
    expected
        .iter()
        .zip(got)
        .enumerate()
        .find_map(|(index, (row, other))| {
            let here = format!("{at}[{index}]");
            first_field!(
                "rows[]",
                here,
                row,
                other,
                [kind, title, target, text, status]
            )
            .or_else(|| {
                rows_difference(&format!("{here}.children"), &row.children, &other.children)
            })
        })
        .or_else(|| {
            same(
                "rows.len",
                at,
                &expected
                    .iter()
                    .map(|row| (row.kind, &row.title))
                    .collect::<Vec<_>>(),
                &got.iter()
                    .map(|row| (row.kind, &row.title))
                    .collect::<Vec<_>>(),
            )
        })
}

fn answered_in_terminal(view: &ChatView) -> Vec<String> {
    view.requests
        .iter()
        .filter(|request| request.id.0 >= 1 << 48)
        .map(|request| {
            short(&(
                &request.item,
                &request.ask,
                request.blocking,
                &request.close,
            ))
        })
        .collect()
}

fn strict_reload_difference(live: &ChatView, reloaded: &ChatView) -> Option<Difference> {
    same(
        "turns.len",
        "turns",
        &live.turns.len(),
        &reloaded.turns.len(),
    )
    .or_else(|| {
        live.turns
            .iter()
            .zip(&reloaded.turns)
            .enumerate()
            .find_map(|(index, (turn, other))| {
                same(
                    "turns[].outcome",
                    &format!("turns[{index}].outcome"),
                    &turn.outcome,
                    &other.outcome,
                )
                .or_else(|| {
                    rows_difference(
                        &format!("turns[{index}].rows"),
                        &rows(live, &turn.items, true),
                        &rows(reloaded, &other.items, false),
                    )
                })
            })
    })
    .or_else(|| {
        rows_difference(
            "background",
            &rows(live, &live.background, true),
            &rows(reloaded, &reloaded.background, false),
        )
    })
    .or_else(|| {
        same(
            "requests",
            "requests",
            &answered_in_terminal(live),
            &answered_in_terminal(reloaded),
        )
    })
}

fn flat_reload_difference(
    live: (&ChatView, &[ChatEvent]),
    reloaded: (&ChatView, &[ChatEvent]),
) -> Option<Difference> {
    rows_difference(
        "rows",
        &rows(live.0, &top_level(live.0, live.1), true),
        &rows(reloaded.0, &top_level(reloaded.0, reloaded.1), false),
    )
    .or_else(|| {
        same(
            "requests",
            "requests",
            &answered_in_terminal(live.0),
            &answered_in_terminal(reloaded.0),
        )
    })
}

#[test]
fn a_reload_from_the_journal_at_every_line_shows_the_live_chat() {
    let mut findings = Findings::new("C journal reload");
    for tape in tapes() {
        let mut events = Vec::new();
        let mut live = ChatView::default();
        for (index, line) in tape.lines.iter().enumerate() {
            for event in line.payloads.iter().filter_map(chat_event) {
                let _ = live.apply(event);
                events.push(event.clone());
            }
            let replayed = tape.reload(index);
            let reloaded = fold(&replayed);
            let at = events.last().map_or_else(|| "start".to_owned(), kind_of);
            let rank = (tape.lines.len(), index);
            let example = |diff: &Difference| {
                format!(
                    "{} line {index}/{} ({}): {}",
                    tape.name,
                    tape.lines.len(),
                    line.label,
                    diff.describe()
                )
            };
            findings.checked();
            if reloaded.skipped > 0 {
                findings.add(
                    "C | reload fold skipped events".to_owned(),
                    at.clone(),
                    rank,
                    || format!("{} line {index}: {} skipped", tape.name, reloaded.skipped),
                );
            }
            if let Some(diff) = strict_reload_difference(&live, &reloaded) {
                findings.add(
                    format!("C | strict | {}", diff.field),
                    at.clone(),
                    rank,
                    || example(&diff),
                );
            }
            if let Some(diff) = flat_reload_difference((&live, &events), (&reloaded, &replayed)) {
                findings.add(
                    format!("C | rows without turns | {}", diff.field),
                    at,
                    rank,
                    || example(&diff),
                );
            }
        }
    }
    findings.finish(&[
        "C | rows without turns | requests",
        "C | strict | turns.len",
    ]);
}
