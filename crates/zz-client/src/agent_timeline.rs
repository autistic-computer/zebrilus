use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use web_time::{Duration, Instant};

use crate::agent_transcript::{AgentToolKindModel, AgentToolStatusModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentToolKind {
    Read,
    Search,
    Edit,
    Execute,
    Fetch,
    Think,
    Other,
}

impl From<AgentToolKindModel> for AgentToolKind {
    fn from(kind: AgentToolKindModel) -> Self {
        match kind {
            AgentToolKindModel::Read => Self::Read,
            AgentToolKindModel::Search => Self::Search,
            AgentToolKindModel::Edit | AgentToolKindModel::Delete | AgentToolKindModel::Move => {
                Self::Edit
            }
            AgentToolKindModel::Execute => Self::Execute,
            AgentToolKindModel::Fetch => Self::Fetch,
            AgentToolKindModel::Think => Self::Think,
            AgentToolKindModel::SwitchMode | AgentToolKindModel::Other => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentToolStatus {
    Pending,
    Running,
    NeedsApproval,
    Completed,
    Failed,
    Canceled,
}

impl AgentToolStatus {
    #[must_use]
    pub const fn running(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

impl From<AgentToolStatusModel> for AgentToolStatus {
    fn from(status: AgentToolStatusModel) -> Self {
        match status {
            AgentToolStatusModel::Pending => Self::Pending,
            AgentToolStatusModel::Running => Self::Running,
            AgentToolStatusModel::NeedsApproval => Self::NeedsApproval,
            AgentToolStatusModel::Completed => Self::Completed,
            AgentToolStatusModel::Failed => Self::Failed,
            AgentToolStatusModel::Canceled => Self::Canceled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineShape {
    User,
    Assistant,
    Notice,
    Reasoning,
    Plan,
    Tool(AgentToolKind, AgentToolStatus),
}

pub trait TimelineItem: Clone {
    fn id(&self) -> u64;
    fn parent(&self) -> Option<u64>;
    fn shape(&self) -> TimelineShape;
    fn label(&self) -> &str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimelineGroupKind {
    Turn,
    Reply,
}

#[must_use]
pub const fn group_kind(shape: TimelineShape) -> Option<TimelineGroupKind> {
    match shape {
        TimelineShape::Tool(..)
        | TimelineShape::Reasoning
        | TimelineShape::Plan
        | TimelineShape::Assistant => Some(TimelineGroupKind::Turn),
        TimelineShape::User | TimelineShape::Notice => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineRow<E> {
    Single(E),
    Group {
        kind: TimelineGroupKind,
        id: u64,
        entries: Arc<Vec<E>>,
    },
}

impl<E: TimelineItem> TimelineRow<E> {
    #[must_use]
    pub fn id(&self) -> u64 {
        match self {
            Self::Single(entry) => entry.id(),
            Self::Group { id, .. } => *id,
        }
    }

    #[must_use]
    pub fn entry(&self, id: u64) -> Option<&E> {
        match self {
            Self::Single(entry) => (entry.id() == id).then_some(entry),
            Self::Group { entries, .. } => entries.iter().find(|entry| entry.id() == id),
        }
    }

    pub fn replace_entry(&mut self, id: u64, entry: E) -> bool {
        if entry.id() != id {
            return false;
        }
        match self {
            Self::Single(current) if current.id() == id => {
                *current = entry;
                true
            }
            Self::Group { entries, .. } => {
                let Some(index) = entries.iter().position(|current| current.id() == id) else {
                    return false;
                };
                Arc::make_mut(entries)[index] = entry;
                true
            }
            Self::Single(_) => false,
        }
    }

    #[must_use]
    pub fn members(&self) -> &[E] {
        match self {
            Self::Single(entry) => std::slice::from_ref(entry),
            Self::Group { entries, .. } => entries,
        }
    }
}

pub struct FoldedTimelineRows<E> {
    pub rows: Arc<Vec<TimelineRow<E>>>,
    pub entry_to_row: Vec<usize>,
}

#[must_use]
pub fn fold_timeline_rows<E: TimelineItem>(entries: &[E]) -> FoldedTimelineRows<E> {
    let positions = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id(), index))
        .collect::<HashMap<_, _>>();
    let roots = (0..entries.len())
        .map(|index| step_root(entries, &positions, index))
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut entry_to_row = vec![0; entries.len()];
    for (index, entry) in entries.iter().enumerate() {
        if roots[index].is_none() {
            entry_to_row[index] = append_timeline_row(&mut rows, entry.clone()).0;
        }
    }
    for (index, entry) in entries.iter().enumerate() {
        if let Some(root) = roots[index] {
            let row_index = entry_to_row[root];
            nest_in_row(&mut rows[row_index], entry.clone());
            entry_to_row[index] = row_index;
        }
    }
    FoldedTimelineRows {
        rows: Arc::new(rows),
        entry_to_row,
    }
}

fn step_root<E: TimelineItem>(
    entries: &[E],
    positions: &HashMap<u64, usize>,
    index: usize,
) -> Option<usize> {
    let mut at = index;
    for _ in 0..entries.len() {
        match entries[at]
            .parent()
            .and_then(|parent| positions.get(&parent).copied())
        {
            Some(parent) => at = parent,
            None => return (at != index).then_some(at),
        }
    }
    None
}

fn nest_in_row<E: TimelineItem>(row: &mut TimelineRow<E>, entry: E) {
    match row {
        TimelineRow::Group { entries, .. } => Arc::make_mut(entries).push(entry),
        TimelineRow::Single(previous) => {
            let id = previous.id();
            let kind = if previous.shape() == TimelineShape::User {
                TimelineGroupKind::Reply
            } else {
                TimelineGroupKind::Turn
            };
            let previous = previous.clone();
            *row = TimelineRow::Group {
                kind,
                id,
                entries: Arc::new(vec![previous, entry]),
            };
        }
    }
}

pub fn append_timeline_row<E: TimelineItem>(
    rows: &mut Vec<TimelineRow<E>>,
    entry: E,
) -> (usize, bool) {
    if let Some(parent) = entry.parent()
        && let Some(row_index) = rows.iter().rposition(|row| row.entry(parent).is_some())
    {
        nest_in_row(&mut rows[row_index], entry);
        return (row_index, false);
    }
    if let Some(kind) = group_kind(entry.shape())
        && let (Some(row_index), Some(last)) = (rows.len().checked_sub(1), rows.last_mut())
    {
        match last {
            TimelineRow::Single(previous) if group_kind(previous.shape()) == Some(kind) => {
                let id = previous.id();
                let previous = previous.clone();
                *last = TimelineRow::Group {
                    kind,
                    id,
                    entries: Arc::new(vec![previous, entry]),
                };
                return (row_index, false);
            }
            TimelineRow::Group {
                kind: open,
                entries,
                ..
            } if *open == kind => {
                Arc::make_mut(entries).push(entry);
                return (row_index, false);
            }
            TimelineRow::Single(_) | TimelineRow::Group { .. } => {}
        }
    }
    let row_index = rows.len();
    rows.push(TimelineRow::Single(entry));
    (row_index, true)
}

#[must_use]
pub fn is_turn_row<E: TimelineItem>(row: &TimelineRow<E>) -> bool {
    match row {
        TimelineRow::Group { kind, .. } => *kind == TimelineGroupKind::Turn,
        TimelineRow::Single(entry) => group_kind(entry.shape()) == Some(TimelineGroupKind::Turn),
    }
}

#[must_use]
pub fn final_assistant_entry_id<E: TimelineItem>(rows: &[TimelineRow<E>]) -> Option<u64> {
    rows.iter().rev().find_map(|row| match row {
        TimelineRow::Single(entry) => {
            (entry.shape() == TimelineShape::Assistant).then(|| entry.id())
        }
        TimelineRow::Group { entries, .. } => entries
            .iter()
            .rev()
            .find(|entry| entry.shape() == TimelineShape::Assistant)
            .map(TimelineItem::id),
    })
}

pub struct StepNesting {
    pub top: Vec<usize>,
    pub steps: HashMap<u64, Vec<usize>>,
}

impl StepNesting {
    pub fn new<E: TimelineItem>(members: &[E]) -> Self {
        let positions = members
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id(), index))
            .collect::<HashMap<_, _>>();
        let parent_of = |index: usize| {
            members[index]
                .parent()
                .and_then(|parent| positions.get(&parent).copied())
        };
        let reaches_top = |index: usize| {
            let mut visited = HashSet::new();
            let mut at = index;
            loop {
                if !visited.insert(at) {
                    return false;
                }
                match parent_of(at) {
                    Some(parent) => at = parent,
                    None => return true,
                }
            }
        };
        let mut top = Vec::new();
        let mut steps = HashMap::<u64, Vec<usize>>::new();
        for index in 0..members.len() {
            match parent_of(index).filter(|_| reaches_top(index)) {
                Some(parent) => steps.entry(members[parent].id()).or_default().push(index),
                None => top.push(index),
            }
        }
        Self { top, steps }
    }

    #[must_use]
    pub fn substeps(&self, id: u64) -> usize {
        self.steps.get(&id).map_or(0, Vec::len)
    }
}

pub fn split_turn<E: TimelineItem>(
    members: &[E],
    nesting: &StepNesting,
) -> (Vec<usize>, Vec<usize>) {
    let mut shown = nesting
        .top
        .iter()
        .copied()
        .filter(|index| members[*index].shape() != TimelineShape::Plan)
        .collect::<Vec<_>>();
    let answer_start = shown
        .iter()
        .rposition(|index| members[*index].shape() != TimelineShape::Assistant)
        .map_or(0, |last| last + 1);
    let answer = shown.split_off(answer_start);
    (shown, answer)
}

pub const TRACE_COUNT_NOUNS: [(&str, &str); 8] = [
    ("read", "reads"),
    ("search", "searches"),
    ("edit", "edits"),
    ("command", "commands"),
    ("fetch", "fetches"),
    ("tool call", "tool calls"),
    ("agent", "agents"),
    ("failed", "failed"),
];

pub const TRACE_COUNT_FAILED: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TraceCount {
    pub slot: usize,
    pub count: usize,
    pub failed: bool,
}

impl TraceCount {
    #[must_use]
    pub fn describe(&self) -> String {
        let (one, many) = TRACE_COUNT_NOUNS[self.slot];
        format!(
            "{} {}",
            self.count,
            if self.count == 1 { one } else { many }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceHeader {
    Waiting {
        detail: String,
    },
    Live {
        label: String,
        tool: bool,
    },
    Done {
        label: String,
        counts: Vec<TraceCount>,
    },
}

impl TraceHeader {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Waiting { .. } => "Waiting for you",
            Self::Live { label, .. } | Self::Done { label, .. } => label,
        }
    }
}

pub fn trace_header<E: TimelineItem>(
    members: &[E],
    nesting: &StepNesting,
    steps: &[usize],
    answering: bool,
    live: bool,
    elapsed: Option<Duration>,
) -> TraceHeader {
    let entries = || steps.iter().map(|index| &members[*index]);
    let tools = || {
        entries().filter_map(|entry| match entry.shape() {
            TimelineShape::Tool(kind, status) => Some((entry, kind, status)),
            _ => None,
        })
    };
    if live && !answering {
        if let Some((waiting, ..)) =
            tools().find(|(_, _, status)| *status == AgentToolStatus::NeedsApproval)
        {
            return TraceHeader::Waiting {
                detail: single_line(waiting.label()).into_owned(),
            };
        }
        let last = entries().next_back();
        return match last.map(TimelineItem::shape) {
            Some(TimelineShape::Tool(_, status)) if status.running() => TraceHeader::Live {
                label: single_line(last.map_or("", TimelineItem::label)).into_owned(),
                tool: true,
            },
            Some(TimelineShape::Reasoning) => TraceHeader::Live {
                label: "Thinking".to_owned(),
                tool: false,
            },
            _ => TraceHeader::Live {
                label: "Working".to_owned(),
                tool: false,
            },
        };
    }
    let counts = trace_counts(members, nesting, steps);
    let thought_only = tools().next().is_none();
    let label = match (thought_only, elapsed) {
        (true, Some(elapsed)) => format!("Thought for {}", duration_label(elapsed)),
        (true, None) => "Thought".to_owned(),
        (false, Some(elapsed)) => format!("Worked {}", duration_label(elapsed)),
        (false, None) => "Worked".to_owned(),
    };
    TraceHeader::Done { label, counts }
}

#[must_use]
pub fn trace_counts<E: TimelineItem>(
    members: &[E],
    nesting: &StepNesting,
    steps: &[usize],
) -> Vec<TraceCount> {
    let mut counts = [0_usize; 7];
    let mut failed = 0;
    for entry in steps.iter().map(|index| &members[*index]) {
        let TimelineShape::Tool(kind, status) = entry.shape() else {
            continue;
        };
        if status == AgentToolStatus::Failed {
            failed += 1;
        }
        let slot = if nesting.substeps(entry.id()) > 0 {
            6
        } else {
            match kind {
                AgentToolKind::Read => 0,
                AgentToolKind::Search => 1,
                AgentToolKind::Edit => 2,
                AgentToolKind::Execute => 3,
                AgentToolKind::Fetch => 4,
                AgentToolKind::Other => 5,
                AgentToolKind::Think => continue,
            }
        };
        counts[slot] += 1;
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(slot, count)| TraceCount {
            slot,
            count,
            failed: false,
        })
        .chain((failed > 0).then_some(TraceCount {
            slot: TRACE_COUNT_FAILED,
            count: failed,
            failed: true,
        }))
        .filter(|count| count.count > 0)
        .collect()
}

pub fn ending_failure<E: TimelineItem>(members: &[E], steps: &[usize]) -> Option<usize> {
    steps
        .iter()
        .rev()
        .copied()
        .find(|index| matches!(members[*index].shape(), TimelineShape::Tool(..)))
        .filter(|index| {
            matches!(
                members[*index].shape(),
                TimelineShape::Tool(_, AgentToolStatus::Failed)
            )
        })
}

#[must_use]
pub fn tool_tag(status: AgentToolStatus, exit_code: Option<i64>) -> Option<(String, bool)> {
    match status {
        AgentToolStatus::Failed => Some((
            exit_code
                .filter(|code| *code != 0)
                .map_or_else(|| "failed".to_owned(), |code| format!("exit {code}")),
            true,
        )),
        AgentToolStatus::Canceled => Some(("canceled".to_owned(), false)),
        _ => None,
    }
}

#[must_use]
pub fn duration_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs().max(1);
    match (seconds / 3600, seconds / 60 % 60, seconds % 60) {
        (0, 0, seconds) => format!("{seconds}s"),
        (0, minutes, seconds) => format!("{minutes}m {seconds}s"),
        (hours, minutes, _) => format!("{hours}h {minutes}m"),
    }
}

#[must_use]
pub fn thought_tail(text: &str) -> Option<String> {
    const KEEP: usize = 220;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.len() <= KEEP {
        return Some(flat);
    }
    let mut start = flat.len() - KEEP;
    while !flat.is_char_boundary(start) {
        start += 1;
    }
    let tail = &flat[start..];
    let tail = tail.split_once(' ').map_or(tail, |(_, rest)| rest);
    Some(format!("…{tail}"))
}

#[must_use]
pub fn single_line(text: &str) -> Cow<'_, str> {
    if !text.contains(['\n', '\r']) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.split(['\n', '\r'])
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
    )
}

struct TurnClock {
    started: Instant,
    finished: Option<Duration>,
}

#[derive(Default)]
pub struct TurnClocks(HashMap<u64, TurnClock>);

impl TurnClocks {
    pub fn tick(&mut self, live: Option<u64>) {
        for (id, clock) in &mut self.0 {
            if Some(*id) != live && clock.finished.is_none() {
                clock.finished = Some(clock.started.elapsed());
            }
        }
        if let Some(live) = live {
            self.0.entry(live).or_insert_with(|| TurnClock {
                started: Instant::now(),
                finished: None,
            });
        }
    }

    #[must_use]
    pub fn elapsed(&self, id: u64) -> Option<Duration> {
        let clock = self.0.get(&id)?;
        Some(clock.finished.unwrap_or_else(|| clock.started.elapsed()))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanItemState {
    Pending,
    InProgress,
    Done,
}

#[must_use]
pub fn plan_items(source: &str) -> Vec<(PlanItemState, &str)> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("- [")?;
            let mut characters = rest.chars();
            let state = match characters.next()? {
                'x' | 'X' => PlanItemState::Done,
                '~' => PlanItemState::InProgress,
                _ => PlanItemState::Pending,
            };
            let text = characters.as_str().strip_prefix(']')?.trim();
            Some((state, text))
        })
        .collect()
}

#[must_use]
pub fn plan_progress(source: &str) -> (usize, usize, Option<&str>) {
    let items = plan_items(source);
    let done = items
        .iter()
        .filter(|(state, _)| *state == PlanItemState::Done)
        .count();
    let current = items
        .iter()
        .find(|(state, _)| *state == PlanItemState::InProgress)
        .or_else(|| {
            items
                .iter()
                .find(|(state, _)| *state == PlanItemState::Pending)
        })
        .map(|(_, text)| *text);
    (done, items.len(), current)
}

#[must_use]
pub fn plan_chip_label(source: &str) -> Option<(String, Option<String>)> {
    let (done, total, current) = plan_progress(source);
    (total > 0).then(|| (format!("Plan {done}/{total}"), current.map(str::to_owned)))
}

#[must_use]
pub fn task_tray_label(count: usize) -> String {
    format!("{count} running")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComposerAction {
    Send,
    Queue,
    Stop,
}

#[must_use]
pub const fn composer_action(active_turn: bool, has_content: bool) -> ComposerAction {
    match (active_turn, has_content) {
        (false, _) => ComposerAction::Send,
        (true, true) => ComposerAction::Queue,
        (true, false) => ComposerAction::Stop,
    }
}

#[must_use]
pub fn git_file_count_label(count: u32) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

#[must_use]
pub fn git_summary_tooltip(
    branch: Option<&str>,
    changed_files: u32,
    additions: u32,
    deletions: u32,
) -> String {
    format!(
        "{}: {}, +{additions} additions, -{deletions} deletions",
        branch.unwrap_or("Detached HEAD"),
        git_file_count_label(changed_files),
    )
}

#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn context_usage_fraction(used: u64, size: u64) -> f64 {
    if size == 0 {
        0.0
    } else {
        (used.min(size) as f64 / size as f64).clamp(0.0, 1.0)
    }
}

#[must_use]
pub fn context_usage_tooltip(used: u64, size: u64) -> String {
    if size == 0 {
        "Context window usage unavailable".to_owned()
    } else {
        format!(
            "{used} of {size} context tokens used ({:.0}%)",
            context_usage_fraction(used, size) * 100.0
        )
    }
}
