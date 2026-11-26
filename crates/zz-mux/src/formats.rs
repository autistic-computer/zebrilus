use std::{
    borrow::Cow,
    cell::Cell,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    sync::{
        Arc, LazyLock, OnceLock, Weak,
        atomic::{AtomicU8, Ordering as AtomicOrdering},
    },
};

use chrono::{DateTime, Datelike as _, FixedOffset};
use glob::{MatchOptions, Pattern};
use parking_lot::Mutex;
use regex::{Captures, Regex, RegexBuilder};
use unicode_width::UnicodeWidthChar as _;
use zz_protocol::{
    CommandSpec, MAX_STATUS_TEXT_BYTES, PaneBorderStatus, PaneId, RawText, SessionId, WindowId,
};
pub use zz_protocol::{TmuxColour, display_width, indexed_colour_rgb, parse_tmux_colour};

use crate::{
    MuxEngine, PaneKind, WindowSize, command::TmuxOptionTarget, layout::LayoutFormat,
    terminfo::TtyTerm,
};

mod tree;

#[cfg(test)]
mod tree_tests;

use tree::FormatTree;
pub use tree::with_borrowed_formats;

mod compiled;
mod engine;
mod expander;
mod status_values;
mod universe;
mod variables;

pub use status_values::StatusValues;
use status_values::*;
pub use universe::with_eager_universe;
use variables::*;
#[cfg(test)]
pub(crate) use variables::{
    constant_format_variable_names, direct_format_variable_names, format_variable_names,
};
pub use variables::{
    delegated_format_variable_names, format_variable_is_captured, format_variable_is_known,
};

#[must_use]
pub fn format_references(format: &str) -> Arc<[String]> {
    compiled::get(format).references.clone()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatPart {
    pub range: std::ops::Range<usize>,
    pub unconditional_clock: bool,
    pub references: Arc<[String]>,
    pub required_loops: FormatNeeds,
}

#[must_use]
pub fn format_parts(format: &str) -> Arc<[FormatPart]> {
    compiled::get(format).format_parts.clone()
}

#[must_use]
pub fn format_clock_dependent(format: &str) -> bool {
    compiled::get(format).clock_dependent
}

#[must_use]
pub fn cloned_raw_text_bytes(value: &RawText) -> usize {
    let text = value.as_str().len();
    if value.to_utf8().is_some() {
        text
    } else {
        text.saturating_add(value.as_bytes().len())
    }
}

pub fn borrowed_formats_enabled() -> bool {
    tree::borrowed_formats()
}

const FORMAT_LOOP_LIMIT: usize = 100;
const FORMAT_MAX_REPEAT: usize = 10_000;
const FORMAT_MAX_REPEAT_BYTES: usize = MAX_STATUS_TEXT_BYTES * FORMAT_MAX_REPEAT;
const FORMAT_MAX_WIDTH: isize = 10_000;
const LOOP_CONTEXT_FORMATS: &[&str] = &["loop_index", "loop_last_flag"];
const OPTION_LOOP_CONTEXT_FORMATS: &[&str] = &[
    "loop_index",
    "loop_last_flag",
    "option_array_count",
    "option_array_first",
    "option_array_index",
    "option_array_key",
    "option_array_last",
    "option_is_array",
    "option_is_hook",
    "option_is_user",
    "option_name",
    "option_value",
];
const ENVIRONMENT_LOOP_CONTEXT_FORMATS: &[&str] = &[
    "environ_hidden",
    "environ_name",
    "environ_removed",
    "environ_value",
    "loop_index",
    "loop_last_flag",
];
const WINDOW_LOOP_CONTEXT_FORMATS: &[&str] = &["window_after_active", "window_before_active"];
const WINDOW_NEIGHBOUR_INDEX_CONTEXT_FORMATS: &[&str] = &["next_window_index", "prev_window_index"];
const WINDOW_NEIGHBOUR_ACTIVE_CONTEXT_FORMATS: &[&str] =
    &["next_window_active", "prev_window_active"];
const WINDOW_NEIGHBOUR_USER_OPTION_PREFIXES: [&str; 2] = ["next_", "prev_"];
const WINDOW_NEIGHBOUR_USER_OPTION_CONTEXT_PATTERNS: &[&str] = &["next_@*", "prev_@*"];

const LITERAL_FORMAT_CONTEXT_SCOPES: &[(&str, &str, &[&str])] = &[
    (
        "format.c",
        "format_loop_add_array_item",
        OPTION_LOOP_CONTEXT_FORMATS,
    ),
    (
        "format.c",
        "format_loop_add_option",
        OPTION_LOOP_CONTEXT_FORMATS,
    ),
    ("format.c", "format_loop_clients", LOOP_CONTEXT_FORMATS),
    (
        "format.c",
        "format_loop_environ",
        ENVIRONMENT_LOOP_CONTEXT_FORMATS,
    ),
    ("format.c", "format_loop_panes", LOOP_CONTEXT_FORMATS),
    ("format.c", "format_loop_sessions", LOOP_CONTEXT_FORMATS),
    ("format.c", "format_loop_windows", LOOP_CONTEXT_FORMATS),
    (
        "format.c",
        "format_loop_windows",
        WINDOW_LOOP_CONTEXT_FORMATS,
    ),
];

const DERIVED_FORMAT_CONTEXT_FAMILIES: &[(&str, &[&str], &[&str])] = &[
    (
        "window-neighbour-index",
        WINDOW_NEIGHBOUR_INDEX_CONTEXT_FORMATS,
        &[],
    ),
    (
        "window-neighbour-active",
        WINDOW_NEIGHBOUR_ACTIVE_CONTEXT_FORMATS,
        &[],
    ),
    (
        "window-neighbour-user-option",
        &[],
        WINDOW_NEIGHBOUR_USER_OPTION_CONTEXT_PATTERNS,
    ),
];

pub(crate) fn literal_format_context_scopes()
-> impl Iterator<Item = (&'static str, &'static str, &'static [&'static str])> {
    LITERAL_FORMAT_CONTEXT_SCOPES.iter().copied()
}

pub(crate) fn derived_format_context_families() -> impl Iterator<
    Item = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    ),
> {
    DERIVED_FORMAT_CONTEXT_FAMILIES.iter().copied()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FormatType {
    #[default]
    None,
    Session,
    Window,
    Pane,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormatClient {
    #[default]
    NoClient,
    Unattached,
    Attached(SessionId),
}

impl FormatClient {
    fn attached_session(self) -> Option<SessionId> {
        match self {
            Self::Attached(session) => Some(session),
            Self::NoClient | Self::Unattached => None,
        }
    }

    fn session_active(self, session: SessionId) -> Option<bool> {
        match self {
            Self::NoClient => None,
            Self::Unattached => Some(false),
            Self::Attached(client_session) => Some(client_session == session),
        }
    }
}

const FORMAT_CAPTURE_CACHE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FormatCaptureRevision {
    state: u64,
    options: u64,
    data: u64,
    now: u64,
}

impl FormatCaptureRevision {
    fn new(engine: &MuxEngine) -> Self {
        Self {
            state: engine.state.generation(),
            options: engine.format_options_generation(),
            data: engine.format_data_generation,
            now: engine.format_now(),
        }
    }

    fn same_data(&self, other: &Self) -> bool {
        self.state == other.state && self.options == other.options && self.data == other.data
    }
}

#[derive(Debug)]
pub(crate) struct DetachedContextCache {
    target: (Option<SessionId>, Option<WindowId>, Option<PaneId>),
    input_variables: Arc<BTreeMap<String, String>>,
    needs: FormatNeeds,
    references: Option<BTreeSet<String>>,
    context: StatusContext<'static>,
    reference_identity: Option<Weak<BTreeSet<String>>>,
}

impl DetachedContextCache {
    fn matches_references(
        &self,
        references: Option<&BTreeSet<String>>,
        shared: Option<&Arc<BTreeSet<String>>>,
    ) -> bool {
        self.reference_identity
            .as_ref()
            .zip(shared)
            .is_some_and(|(cached, current)| cached.as_ptr() == Arc::as_ptr(current))
            || self.references.as_ref() == references
    }

    fn adopt_reference_identity(&mut self, shared: Option<&Arc<BTreeSet<String>>>) {
        if let Some(shared) = shared
            && self
                .reference_identity
                .as_ref()
                .is_none_or(|cached| cached.as_ptr() != Arc::as_ptr(shared))
        {
            self.reference_identity = Some(Arc::downgrade(shared));
        }
    }
}

#[derive(Debug)]
pub(crate) struct FormatReferenceUnionCache {
    generation: u64,
    sources: Vec<String>,
    references: Arc<BTreeSet<String>>,
}

fn variable_bytes(variables: &BTreeMap<String, String>) -> usize {
    variables.iter().fold(0usize, |bytes, (name, value)| {
        bytes
            .saturating_add(std::mem::size_of::<(String, String)>())
            .saturating_add(name.capacity())
            .saturating_add(value.capacity())
    })
}

fn reference_bytes(references: &BTreeSet<String>) -> usize {
    references.iter().fold(0usize, |bytes, name| {
        bytes
            .saturating_add(std::mem::size_of::<String>())
            .saturating_add(name.capacity())
    })
}

fn format_id_matches(value: &str, expected: Option<u64>, prefix: char) -> bool {
    expected.map_or_else(
        || value.is_empty(),
        |expected| {
            value.strip_prefix(prefix).is_some_and(|digits| {
                (digits == "0" || !digits.starts_with('0'))
                    && digits.bytes().all(|byte| byte.is_ascii_digit())
                    && digits.parse::<u64>() == Ok(expected)
            })
        },
    )
}

#[derive(Clone, Debug, Default)]
pub struct StatusContext<'e> {
    values: OnceLock<Box<StatusValues>>,
    #[doc(hidden)]
    pub format_universe: FormatUniverseRef<'e>,
    tree: Option<FormatTree<'e>>,
    variables: Arc<BTreeMap<String, String>>,
    format_client: FormatClient,
    pub session_id: String,
    pub window_id: String,
    pub pane_id: String,
    pub format_now: Option<i64>,
    capture_revision: Option<Arc<FormatCaptureRevision>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormatNeeds(u8);

impl FormatNeeds {
    pub const NONE: Self = Self(0);
    pub const SESSIONS: Self = Self(1);
    pub const WINDOWS: Self = Self(1 << 1);
    pub const PANES: Self = Self(1 << 2);
    pub const OPTIONS: Self = Self(1 << 3);
    pub const ENVIRONMENT: Self = Self(1 << 4);
    pub const ALL: Self = Self(0b1_1111);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for FormatNeeds {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for FormatNeeds {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.union(other);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OptionRowsKey {
    Server,
    GlobalSession,
    GlobalWindow,
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

type LoopItems = Arc<[FormatLoopItem]>;

#[derive(Debug)]
pub struct FormatUniverse {
    format_client: FormatClient,
    built: AtomicU8,
    sessions: OnceLock<LoopItems>,
    windows: Mutex<BTreeMap<SessionId, Option<LoopItems>>>,
    panes: Mutex<BTreeMap<WindowId, Option<LoopItems>>>,
    options: Mutex<BTreeMap<OptionRowsKey, Option<Arc<[FormatOptionRow]>>>>,
    environments: Mutex<BTreeMap<Option<SessionId>, Option<Arc<[FormatEnvironRow]>>>>,
    window_user_options: Mutex<BTreeMap<WindowId, Option<Arc<[(String, String)]>>>>,
}

#[derive(Clone)]
pub struct FormatUniverseRef<'e> {
    parts: Arc<FormatUniverse>,
    engine: Option<&'e MuxEngine>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatClientRow {
    pub name: String,
    pub activity: u64,
    pub variables: BTreeMap<String, String>,
    pub environment: Vec<FormatEnvironRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatEnvironRow {
    pub name: String,
    pub value: RawText,
    pub hidden: bool,
    pub removed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FormatOptionRow {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) is_array: bool,
    pub(crate) array_key: String,
    pub(crate) array_index: String,
    pub(crate) array_first: bool,
    pub(crate) array_last: bool,
    pub(crate) array_count: usize,
    pub(crate) is_hook: bool,
    pub(crate) is_user: bool,
}

#[derive(Clone, Debug)]
struct FormatLoopItem {
    context: StatusContext<'static>,
    active: bool,
    metadata: LoopMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoopMetadata {
    session_name: String,
    session_sort_activity: u64,
    window_name: String,
    window_index: u32,
    pane_index: u32,
    pane_z: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FormatContext {
    pub(crate) session: Option<SessionId>,
    pub(crate) window: Option<WindowId>,
    pub(crate) pane: Option<PaneId>,
    pub(crate) active_session: Option<SessionId>,
    pub(crate) format_client: FormatClient,
    pub(crate) format_type: FormatType,
}

impl Default for FormatContext {
    fn default() -> Self {
        Self {
            session: None,
            window: None,
            pane: None,
            active_session: None,
            format_client: FormatClient::NoClient,
            format_type: FormatType::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatScope {
    Server,
    Buffer,
    Client,
    Session,
    Window,
    Pane,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatKind {
    String,
    Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatBacking {
    Empty,
    PaneFloatingFlag,
    PaneModalFlag,
    WindowModalPane,
    StatusHook,
    Zero,
    One,
    ActiveWindowIndex,
    ConfigFiles,
    HistoryLimit,
    Host,
    HostShort,
    LastWindowIndex,
    NextSessionId,
    PaneActive,
    PaneAtBottom,
    PaneAtLeft,
    PaneAtRight,
    PaneAtTop,
    PaneBottom,
    PaneCurrentCommand,
    PaneCurrentPath,
    PaneDead,
    PaneDeadSignal,
    PaneDeadStatus,
    PaneDeadTime,
    PaneInputOff,
    PaneMarked,
    PaneMarkedSet,
    PanePath,
    PaneFlags,
    PaneFormat,
    PaneHeight,
    PaneId,
    PaneIndex,
    PaneLast,
    PaneLeft,
    PaneRight,
    PanePid,
    PaneStartCommand,
    PaneStartCommandList,
    PaneStartPath,
    PaneSynchronized,
    PaneTitle,
    PaneTop,
    PaneTty,
    PaneUnzoomedHeight,
    PaneUnzoomedWidth,
    PaneWidth,
    PaneX,
    PaneY,
    PaneZ,
    PaneZoomed,
    Pid,
    ServerSessions,
    SessionAlert,
    SessionAlerts,
    SessionAttached,
    SessionAttachedList,
    SessionBell,
    SessionActivityFlag,
    SessionSilenceFlag,
    SessionActivity,
    SessionActive,
    SessionCreated,
    SessionFormat,
    SessionId,
    SessionManyAttached,
    SessionMarked,
    SessionName,
    SessionPath,
    SessionStack,
    SessionWindows,
    SocketPath,
    StartTime,
    Uid,
    User,
    Version,
    WindowActive,
    WindowActiveClients,
    WindowActiveClientsList,
    WindowActiveSessions,
    WindowActiveSessionsList,
    WindowActivity,
    WindowActivityFlag,
    WindowBell,
    WindowEnd,
    WindowFlags,
    WindowFormat,
    WindowHeight,
    WindowId,
    WindowIndex,
    WindowLast,
    WindowLayout,
    WindowLinked,
    WindowLinkedSessions,
    WindowLinkedSessionsList,
    WindowManualHeight,
    WindowManualWidth,
    WindowMarkedFlag,
    WindowName,
    WindowPanes,
    WindowRawFlags,
    WindowSilenceFlag,
    WindowStackIndex,
    WindowStart,
    WindowVisibleLayout,
    WindowWidth,
    WindowZoomed,
}

#[derive(Clone, Copy, Debug)]
struct FormatVariableSpec {
    name: &'static str,
    scope: FormatScope,
    kind: FormatKind,
    backing: FormatBacking,
    callback: for<'a, 'e> fn(&'a tree::FormatTree<'e>, FormatType) -> Cow<'a, str>,
}

struct ResolvedFormatContext<'e> {
    values: StatusContext<'e>,
    has_session: bool,
    has_window: bool,
    has_pane: bool,
    format_type: FormatType,
}

trait FormatVariables {
    fn variable(&self, name: &str) -> Option<Cow<'_, str>>;
    fn variable_kind(&self, name: &str) -> Option<FormatKind>;
    fn values(&self) -> &StatusContext<'_>;
    fn format_type(&self) -> FormatType;
}

impl FormatVariables for ResolvedFormatContext<'_> {
    fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        let spec = format_variable(name)?;
        let available = match spec.scope {
            FormatScope::Server | FormatScope::Buffer | FormatScope::Client => true,
            FormatScope::Session => self.has_session,
            FormatScope::Window => self.has_window,
            FormatScope::Pane | FormatScope::Terminal => self.has_pane,
        };
        if available {
            Some(self.values.resolve(spec, self.format_type))
        } else {
            Some(Cow::Borrowed(""))
        }
    }

    fn variable_kind(&self, name: &str) -> Option<FormatKind> {
        format_variable(name).map(|spec| spec.kind)
    }

    fn values(&self) -> &StatusContext<'_> {
        &self.values
    }

    fn format_type(&self) -> FormatType {
        self.format_type
    }
}

pub struct FormatContextSnapshot<'a> {
    engine: &'a MuxEngine,
    format_client: FormatClient,
    universe: FormatUniverseRef<'a>,
}

impl<'a> FormatContextSnapshot<'a> {
    #[must_use]
    pub fn status_context(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    ) -> StatusContext<'a> {
        FormatContext {
            session,
            window,
            pane,
            active_session: self.format_client.attached_session(),
            format_client: self.format_client,
            format_type: FormatType::None,
        }
        .resolve_with_universe(self.engine, self.universe.clone())
        .values
    }
}

impl FormatContext {
    fn resolve(self, engine: &MuxEngine) -> ResolvedFormatContext<'_> {
        self.resolve_with_universe(
            engine,
            FormatUniverseRef::deferred(engine, self.format_client),
        )
    }

    fn resolve_with_universe<'e>(
        self,
        engine: &'e MuxEngine,
        universe: FormatUniverseRef<'e>,
    ) -> ResolvedFormatContext<'e> {
        self.resolve_batched(engine, universe, &mut LayoutDumps::default())
    }

    fn resolve_batched<'e>(
        self,
        engine: &'e MuxEngine,
        universe: FormatUniverseRef<'e>,
        dumps: &mut LayoutDumps,
    ) -> ResolvedFormatContext<'e> {
        let (session, window, pane) = engine.format_target(self.session, self.window, self.pane);
        let values = if tree::borrowed_formats() {
            StatusContext::borrowed(engine, session, window, pane, self.format_client, universe)
        } else {
            let mut values: StatusContext<'e> = engine
                .build_status_values(session, window, pane, self.format_client, dumps)
                .into();
            values.format_universe = universe;
            values
        };
        ResolvedFormatContext {
            values,
            has_session: session.is_some(),
            has_window: window.is_some(),
            has_pane: pane.is_some(),
            format_type: self.format_type,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaneFormatGeometry {
    pub window_width: Option<u16>,
    pub window_height: Option<u16>,
    pub pane_left: Option<u16>,
    pub pane_top: Option<u16>,
    pub pane_right: Option<u16>,
    pub pane_bottom: Option<u16>,
    pub pane_width: Option<u16>,
    pub pane_height: Option<u16>,
}

#[derive(Default)]
pub(crate) struct LayoutDumps {
    window: Option<WindowId>,
    layout: String,
    visible: String,
}

fn parse_session(value: &str) -> Option<SessionId> {
    value.strip_prefix('$')?.parse().ok().map(SessionId)
}

fn parse_window(value: &str) -> Option<WindowId> {
    value.strip_prefix('@')?.parse().ok().map(WindowId)
}

fn parse_pane(value: &str) -> Option<PaneId> {
    value.strip_prefix('%')?.parse().ok().map(PaneId)
}

struct NeedsScan<'a> {
    engine: Option<&'a MuxEngine>,
    needs: FormatNeeds,
    followed: BTreeSet<String>,
}

impl NeedsScan<'_> {
    fn done(&self) -> bool {
        self.needs.contains(FormatNeeds::ALL)
    }

    fn template(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() && !self.done() {
            if bytes[index] != b'#' {
                index += 1;
                continue;
            }
            let hashes_end = bytes[index..]
                .iter()
                .position(|byte| *byte != b'#')
                .map_or(bytes.len(), |offset| index + offset);
            if bytes.get(hashes_end) == Some(&b'[') {
                let Some(end) = find_style_end(text, hashes_end + 1) else {
                    break;
                };
                self.template(&text[hashes_end + 1..end]);
                index = end + 1;
                continue;
            }
            match bytes.get(index + 1) {
                None => break,
                Some(b'(') => {
                    let Some(end) = find_plain_group_end(text, index + 2, '(', ')') else {
                        break;
                    };
                    self.template(&text[index + 2..end]);
                    index = end + 1;
                }
                Some(b'{') => {
                    let Some(end) = find_format_end(text, index) else {
                        break;
                    };
                    self.replacement(&text[index + 2..end]);
                    index = end + 1;
                }
                Some(_) => index += 2,
            }
        }
    }

    fn replacement(&mut self, body: &str) {
        let (modifiers, copy) = parse_modifiers(body, |argument| {
            self.template(argument);
            argument.to_owned()
        })
        .map_or_else(
            || (Vec::new(), body),
            |(modifiers, offset)| (modifiers, &body[offset..]),
        );
        let flags = ModifierFlags::from_modifiers(&modifiers);
        let mut named = false;
        let loop_needs = if flags.character || flags.colour.is_some() {
            Some(FormatNeeds::NONE)
        } else if flags.loop_sessions {
            Some(FormatNeeds::SESSIONS)
        } else if flags.loop_windows {
            Some(FormatNeeds::WINDOWS)
        } else if flags.loop_panes {
            Some(FormatNeeds::PANES)
        } else if flags.loop_clients {
            Some(FormatNeeds::NONE)
        } else if flags.loop_options.is_some() {
            Some(FormatNeeds::OPTIONS)
        } else if flags.loop_environment.is_some() {
            Some(FormatNeeds::ENVIRONMENT)
        } else if flags.name_window {
            Some(FormatNeeds::WINDOWS)
        } else if flags.name_session {
            Some(FormatNeeds::SESSIONS)
        } else {
            None
        };
        if flags.interrogate.is_some() || flags.literal {
        } else if let Some(needs) = loop_needs {
            self.needs |= needs;
            self.template(copy);
        } else if flags.content_search.is_some()
            || flags.repeat
            || flags.not
            || flags.not_not
            || flags.bool_op.is_some()
            || flags.comparison.is_some()
        {
            self.template(copy);
        } else if let Some(conditional) = copy.strip_prefix('?') {
            let parts = split_top(conditional, ',');
            let paired = parts.len() / 2 * 2;
            for (index, part) in parts.iter().enumerate() {
                if index < paired && index % 2 == 0 {
                    self.name(part);
                }
                self.template(part);
            }
        } else if flags.expression.is_some() || copy.contains("#{") {
            self.template(copy);
        } else {
            self.name(copy);
            named = true;
        }
        if flags.expand || flags.expand_time {
            if named {
                self.follow(copy);
            } else {
                self.needs = FormatNeeds::ALL;
            }
        }
    }

    fn name(&mut self, name: &str) {
        if format_variable(name).is_none()
            && !self
                .engine
                .is_some_and(|engine| engine.format_option_always_answers(name))
        {
            self.needs |= FormatNeeds::ENVIRONMENT;
        }
    }

    fn follow(&mut self, name: &str) {
        if self.followed.contains(name) {
            return;
        }
        self.followed.insert(name.to_owned());
        let Some(engine) = self.engine else {
            self.needs = FormatNeeds::ALL;
            return;
        };
        if !engine.format_option_texts(name, &mut |text| self.template(text)) {
            self.needs = FormatNeeds::ALL;
        }
    }
}

#[must_use]
pub fn format_needs_without_engine<'t>(
    templates: impl IntoIterator<Item = &'t str>,
) -> FormatNeeds {
    scan_format_needs(None, templates)
}

fn scan_format_needs<'t>(
    engine: Option<&MuxEngine>,
    templates: impl IntoIterator<Item = &'t str>,
) -> FormatNeeds {
    let mut scan = NeedsScan {
        engine,
        needs: FormatNeeds::NONE,
        followed: BTreeSet::new(),
    };
    for template in templates {
        if scan.done() {
            break;
        }
        scan.template(template);
    }
    scan.needs
}

fn pane_number(value: &str) -> u64 {
    value
        .strip_prefix('%')
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum FormatJobTag {
    #[default]
    None,
    Window(String),
    Pane(String),
}

pub const FORMAT_CYCLE_PERIOD_MS: u64 = 100;

pub trait StatusHooks {
    fn strftime(&mut self, literal: &str) -> String;
    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String;

    fn cycle_clock(&mut self) -> Option<u64> {
        None
    }

    fn cycle_animates(&mut self) {}

    fn stable_option_lookups(&self) -> bool {
        false
    }

    fn only_tmux_options(&self) -> bool {
        false
    }

    fn variable(&mut self, _name: &str, _context: &StatusContext) -> Option<String> {
        None
    }

    fn option_variable(&mut self, _name: &str, _context: &StatusContext) -> Option<RawText> {
        None
    }

    fn copy_line_numbers(&mut self, context: &StatusContext, _option: &str) -> Option<String> {
        self.variable("copy_line_numbers", context)
    }

    fn tree_variable(&mut self, _name: &str, _context: &StatusContext) -> Option<Cow<'_, str>> {
        None
    }

    fn window_activity(&mut self, _window: WindowId) -> u64 {
        0
    }

    fn pane_activity(&mut self, _pane: PaneId) -> u64 {
        0
    }

    fn pane_search(
        &mut self,
        _pane: Option<PaneId>,
        _pattern: &str,
        _regex: bool,
        _ignore_case: bool,
    ) -> usize {
        0
    }

    fn client_loop_rows(&mut self, _context: &StatusContext) -> Vec<FormatClientRow> {
        Vec::new()
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        Vec::new()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        None
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        Vec::new()
    }

    fn tree_entries(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
}

fn loop_job_tag(
    inherited: &FormatJobTag,
    target: LoopTarget,
    values: &StatusContext,
) -> FormatJobTag {
    match target {
        LoopTarget::Sessions => inherited.clone(),
        LoopTarget::Windows => FormatJobTag::Window(values.window_id.clone()),
        LoopTarget::Panes => FormatJobTag::Pane(values.pane_id.clone()),
    }
}

pub fn expand_status(
    format: &str,
    context: &StatusContext,
    hooks: &mut impl StatusHooks,
) -> String {
    let mut expander = Expander {
        context,
        universe: &context.format_universe,
        hooks,
        time: true,
        job_tag: FormatJobTag::None,
        client_row: None,
        trace: None,
    };
    expander.expand(format, 0).into()
}

pub fn expand_format_bytes(
    format: &str,
    context: &StatusContext,
    hooks: &mut impl StatusHooks,
) -> RawText {
    let mut expander = Expander {
        context,
        universe: &context.format_universe,
        hooks,
        time: false,
        job_tag: FormatJobTag::None,
        client_row: None,
        trace: None,
    };
    expander.expand(format, 0)
}

pub fn expand_format_values(
    format: &str,
    context: &StatusContext,
    hooks: &mut impl StatusHooks,
) -> String {
    let mut expander = Expander {
        context,
        universe: &context.format_universe,
        hooks,
        time: false,
        job_tag: FormatJobTag::None,
        client_row: None,
        trace: None,
    };
    expander.expand(format, 0).into()
}

pub(crate) struct CommandHooks {
    now: Option<i64>,
}

impl CommandHooks {
    pub(crate) fn new(now: u64) -> Self {
        Self {
            now: i64::try_from(now).ok().filter(|now| *now != 0),
        }
    }
}

impl StatusHooks for CommandHooks {
    fn stable_option_lookups(&self) -> bool {
        true
    }

    fn only_tmux_options(&self) -> bool {
        true
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.now
            .and_then(crate::localtime::local_time)
            .map_or_else(String::new, |now| format_datetime(&now, literal))
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }
}

#[cfg(test)]
pub(crate) fn expand_format(format: &str, engine: &MuxEngine, context: FormatContext) -> String {
    let mut hooks = CommandHooks::new(engine.format_now());
    expand_format_with_hooks(format, engine, context, &mut hooks).into()
}

const LISTING_NEVER_ANSWERED: [&str; 14] = [
    "mouse_hyperlink",
    "mouse_line",
    "mouse_pane",
    "mouse_status_line",
    "mouse_status_range",
    "mouse_word",
    "mouse_x",
    "mouse_y",
    "session_group",
    "session_group_attached",
    "session_group_attached_list",
    "session_group_list",
    "session_group_many_attached",
    "session_group_size",
];

const LISTING_EMPTY_LIST_DECLINES: [&str; 3] = [
    "session_attached_list",
    "window_active_clients_list",
    "window_modal_pane",
];

const LISTING_ZERO_TIMES: [&str; 7] = [
    "client_activity",
    "client_created",
    "session_activity",
    "session_created",
    "session_last_attached",
    "start_time",
    "window_activity",
];

const LISTING_CLIENT_WITNESS: &str = "client_name";

const LISTING_CLIENTLESS_CLIENT_NAME: &str = "client_mode_format";

pub(crate) fn format_listing(
    engine: &MuxEngine,
    context: FormatContext,
    hooks: &mut impl StatusHooks,
) -> Vec<(String, String)> {
    let resolved = context.resolve(engine);
    let has_client = hooks
        .variable(LISTING_CLIENT_WITNESS, &resolved.values)
        .is_some();
    let mut rows = Vec::with_capacity(FORMAT_VARIABLES.len());
    for spec in &FORMAT_VARIABLES {
        if let Some(value) = listing_value(spec, &resolved, has_client, hooks) {
            rows.push((spec.name.to_owned(), value));
        }
    }
    let mut tree = hooks.tree_entries();
    tree.retain(|(name, _)| format_variable(name).is_none());
    tree.sort();
    rows.extend(tree);
    rows
}

fn listing_value(
    spec: &FormatVariableSpec,
    resolved: &ResolvedFormatContext,
    has_client: bool,
    hooks: &mut impl StatusHooks,
) -> Option<String> {
    let available = match spec.scope {
        FormatScope::Server | FormatScope::Buffer => true,
        FormatScope::Client => has_client || spec.name == LISTING_CLIENTLESS_CLIENT_NAME,
        FormatScope::Session => resolved.has_session,
        FormatScope::Window => resolved.has_window,
        FormatScope::Pane | FormatScope::Terminal => resolved.has_pane,
    };
    if !available || LISTING_NEVER_ANSWERED.contains(&spec.name) {
        return None;
    }
    let hooked = hooks.variable(spec.name, &resolved.values);
    let value = match (hooked, spec.backing) {
        (Some(value), _) => value,
        (None, FormatBacking::StatusHook) => {
            return LISTING_ZERO_TIMES
                .contains(&spec.name)
                .then(|| "0".to_owned());
        }
        (None, _) => resolved
            .values
            .resolve(spec, resolved.format_type)
            .into_owned(),
    };
    if value.is_empty() {
        if LISTING_ZERO_TIMES.contains(&spec.name) {
            return Some("0".to_owned());
        }
        if LISTING_EMPTY_LIST_DECLINES.contains(&spec.name) {
            return None;
        }
    }
    if matches!(
        spec.name,
        "pane_dead_signal" | "pane_dead_status" | "pane_dead_time"
    ) && resolved.values.variable("pane_dead").as_deref() != Some("1")
    {
        return None;
    }
    if spec.name == "session_active"
        && resolved
            .values
            .variable("session_active")
            .is_none_or(|value| value.is_empty())
    {
        return None;
    }
    Some(value)
}

pub(crate) fn expand_format_with_hooks(
    format: &str,
    engine: &MuxEngine,
    context: FormatContext,
    hooks: &mut impl StatusHooks,
) -> RawText {
    expand_format_inner(format, engine, context, hooks, false, false).0
}

pub(crate) fn expand_format_time_with_hooks(
    format: &str,
    engine: &MuxEngine,
    context: FormatContext,
    hooks: &mut impl StatusHooks,
) -> RawText {
    expand_format_inner(format, engine, context, hooks, true, false).0
}

pub(crate) fn expand_format_time_traced(
    format: &str,
    engine: &MuxEngine,
    context: FormatContext,
    hooks: &mut impl StatusHooks,
) -> (RawText, Vec<String>) {
    expand_format_inner(format, engine, context, hooks, true, true)
}

fn expand_format_inner(
    format: &str,
    engine: &MuxEngine,
    context: FormatContext,
    hooks: &mut impl StatusHooks,
    time: bool,
    trace: bool,
) -> (RawText, Vec<String>) {
    if !trace && plain_format(format, time) {
        return (RawText::from(format), Vec::new());
    }
    PreparedFormat::new(engine, context).expand_inner(format, hooks, time, trace)
}

pub(crate) fn plain_format(format: &str, time: bool) -> bool {
    !format.contains('#') && (!time || !format.contains('%'))
}

pub(crate) struct PreparedFormat<'e> {
    engine: &'e MuxEngine,
    context: ResolvedFormatContext<'e>,
    fallback: Option<StatusContext<'static>>,
}

impl<'e> PreparedFormat<'e> {
    pub(crate) fn new(engine: &'e MuxEngine, context: FormatContext) -> Self {
        Self::with_universe(
            engine,
            context,
            engine.format_universe(context.format_client),
            &mut LayoutDumps::default(),
        )
    }

    pub(crate) fn with_universe(
        engine: &'e MuxEngine,
        context: FormatContext,
        universe: FormatUniverseRef<'e>,
        dumps: &mut LayoutDumps,
    ) -> Self {
        let fallback =
            (context.session.is_none() && context.window.is_none() && context.pane.is_none())
                .then(|| context.format_client.attached_session())
                .flatten()
                .and_then(|session| {
                    let window = engine.state.sessions.get(&session)?.active_window;
                    let pane = engine.state.windows.get(&window)?.active_pane;
                    Some(StatusContext::from(StatusValues {
                        session_id: session.to_string(),
                        window_id: window.to_string(),
                        pane_id: pane.to_string(),
                        ..StatusValues::default()
                    }))
                });
        Self {
            engine,
            context: context.resolve_batched(engine, universe, dumps),
            fallback,
        }
    }

    pub(crate) fn values(&self) -> &StatusContext<'e> {
        &self.context.values
    }

    pub(crate) fn expand(&self, format: &str, hooks: &mut impl StatusHooks) -> RawText {
        if plain_format(format, false) {
            return RawText::from(format);
        }
        self.expand_inner(format, hooks, false, false).0
    }

    fn expand_inner(
        &self,
        format: &str,
        hooks: &mut impl StatusHooks,
        time: bool,
        trace: bool,
    ) -> (RawText, Vec<String>) {
        let mut sink = Vec::new();
        let mut hooks = OptionFormatHooks {
            engine: self.engine,
            inner: hooks,
            fallback: self.fallback.as_ref(),
        };
        let mut expander = Expander {
            context: &self.context,
            universe: &self.context.values.format_universe,
            hooks: &mut hooks,
            time,
            job_tag: FormatJobTag::None,
            client_row: None,
            trace: trace.then_some(&mut sink),
        };
        let value = expander.expand(format, 0);
        let _ = expander;
        (value, sink)
    }
}

struct OptionFormatHooks<'a, H> {
    engine: &'a MuxEngine,
    inner: &'a mut H,
    fallback: Option<&'a StatusContext<'a>>,
}

impl<H: StatusHooks> StatusHooks for OptionFormatHooks<'_, H> {
    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn cycle_clock(&mut self) -> Option<u64> {
        self.inner.cycle_clock()
    }

    fn cycle_animates(&mut self) {
        self.inner.cycle_animates();
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        let option_context = if context.session_id.is_empty()
            && context.window_id.is_empty()
            && context.pane_id.is_empty()
        {
            self.fallback.unwrap_or(context)
        } else {
            context
        };
        self.engine
            .format_option_value(option_context, name)
            .map(RawText::from)
            .or_else(|| self.inner.option_variable(name, context))
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if name == "copy_line_numbers" {
            let option = self
                .option_variable("copy-mode-line-numbers", context)
                .unwrap_or_default();
            return self.inner.copy_line_numbers(context, option.as_str());
        }
        self.inner.variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        self.inner.tree_variable(name, context)
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }

    fn tree_entries(&mut self) -> Vec<(String, String)> {
        self.inner.tree_entries()
    }
}

struct Expander<'a, V: FormatVariables + ?Sized, H: StatusHooks> {
    context: &'a V,
    universe: &'a FormatUniverseRef<'a>,
    hooks: &'a mut H,
    time: bool,
    job_tag: FormatJobTag,
    client_row: Option<&'a FormatClientRow>,
    trace: Option<&'a mut Vec<String>>,
}

fn parse_modifiers(
    body: &str,
    mut argument: impl FnMut(&str) -> String,
) -> Option<(Vec<Modifier>, usize)> {
    let mut modifiers = Vec::new();
    let mut position = 0usize;
    while position < body.len() && body.as_bytes()[position] != b':' {
        if body.as_bytes()[position] == b';' {
            position += 1;
        }
        if position >= body.len() {
            return None;
        }
        let remaining = &body[position..];
        let mut matched = None;
        for spec in FORMAT_MODIFIER_SPECS
            .iter()
            .filter(|spec| spec.text.len() > 1)
        {
            if remaining.starts_with(spec.text)
                && remaining
                    .as_bytes()
                    .get(spec.text.len())
                    .is_some_and(|next| is_modifier_end(*next))
            {
                matched = Some((spec.kind, spec.text));
                break;
            }
        }
        if let Some((kind, text)) = matched {
            modifiers.push(Modifier {
                kind,
                text,
                args: Vec::new(),
            });
            position += text.len();
            continue;
        }
        let character = remaining.as_bytes()[0];
        let next = remaining.as_bytes().get(1).copied();
        let spec = FORMAT_MODIFIER_SPECS
            .iter()
            .find(|spec| spec.text.as_bytes() == [character])?;
        if next.is_some_and(is_modifier_end) {
            modifiers.push(Modifier {
                kind: spec.kind,
                text: spec.text,
                args: Vec::new(),
            });
            position += 1;
            continue;
        }
        if !spec.arguments {
            return None;
        }
        let kind = spec.kind;
        let text = spec.text;
        position += 1;
        if position >= body.len() {
            return None;
        }
        if is_modifier_end(body.as_bytes()[position]) {
            modifiers.push(Modifier {
                kind,
                text,
                args: Vec::new(),
            });
            continue;
        }
        let wrapper = body.as_bytes()[position];
        let mut args = Vec::new();
        if wrapper.is_ascii_punctuation() && wrapper != b'-' {
            loop {
                if body.as_bytes().get(position) == Some(&wrapper)
                    && body
                        .as_bytes()
                        .get(position + 1)
                        .is_some_and(|next| is_modifier_end(*next))
                {
                    position += 1;
                    break;
                }
                let start = position + 1;
                let end = find_modifier_argument(body, start, wrapper)?;
                let value = unescape(&body[start..end]);
                args.push(argument(&value));
                position = end;
                if is_modifier_end(body.as_bytes()[position]) {
                    break;
                }
            }
        } else {
            let end = find_modifier_argument(body, position, 0)?;
            let value = unescape(&body[position..end]);
            args.push(argument(&value));
            position = end;
        }
        modifiers.push(Modifier { kind, text, args });
    }
    if body.as_bytes().get(position) == Some(&b':') {
        Some((modifiers, position + 1))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModifierKind {
    Literal,
    Basename,
    Dirname,
    Length,
    Width,
    Expand,
    ExpandTime,
    Character,
    Not,
    NotNot,
    And,
    Or,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Match,
    Time,
    Quote,
    Substitute,
    Limit,
    Padding,
    Expression,
    Colour,
    NameExists,
    Sessions,
    Windows,
    Panes,
    Clients,
    Options,
    Environments,
    ContentSearch,
    Repeat,
    Interrogate,
    Animate,
}

#[derive(Clone, Copy)]
struct FormatModifierSpec {
    text: &'static str,
    kind: ModifierKind,
    arguments: bool,
}

impl FormatModifierSpec {
    const fn new(text: &'static str, kind: ModifierKind, arguments: bool) -> Self {
        Self {
            text,
            kind,
            arguments,
        }
    }
}

const FORMAT_MODIFIER_SPECS: [FormatModifierSpec; 37] = [
    FormatModifierSpec::new("||", ModifierKind::Or, false),
    FormatModifierSpec::new("&&", ModifierKind::And, false),
    FormatModifierSpec::new("!!", ModifierKind::NotNot, false),
    FormatModifierSpec::new("!=", ModifierKind::NotEqual, false),
    FormatModifierSpec::new("==", ModifierKind::Equal, false),
    FormatModifierSpec::new("<=", ModifierKind::LessEqual, false),
    FormatModifierSpec::new(">=", ModifierKind::GreaterEqual, false),
    FormatModifierSpec::new("l", ModifierKind::Literal, false),
    FormatModifierSpec::new("b", ModifierKind::Basename, false),
    FormatModifierSpec::new("d", ModifierKind::Dirname, false),
    FormatModifierSpec::new("n", ModifierKind::Length, false),
    FormatModifierSpec::new("w", ModifierKind::Width, false),
    FormatModifierSpec::new("E", ModifierKind::Expand, false),
    FormatModifierSpec::new("T", ModifierKind::ExpandTime, false),
    FormatModifierSpec::new("a", ModifierKind::Character, true),
    FormatModifierSpec::new("!", ModifierKind::Not, false),
    FormatModifierSpec::new("<", ModifierKind::Less, false),
    FormatModifierSpec::new(">", ModifierKind::Greater, false),
    FormatModifierSpec::new("m", ModifierKind::Match, true),
    FormatModifierSpec::new("s", ModifierKind::Substitute, true),
    FormatModifierSpec::new("t", ModifierKind::Time, true),
    FormatModifierSpec::new("=", ModifierKind::Limit, true),
    FormatModifierSpec::new("q", ModifierKind::Quote, true),
    FormatModifierSpec::new("p", ModifierKind::Padding, true),
    FormatModifierSpec::new("e", ModifierKind::Expression, true),
    FormatModifierSpec::new("c", ModifierKind::Colour, true),
    FormatModifierSpec::new("N", ModifierKind::NameExists, true),
    FormatModifierSpec::new("S", ModifierKind::Sessions, true),
    FormatModifierSpec::new("W", ModifierKind::Windows, true),
    FormatModifierSpec::new("P", ModifierKind::Panes, true),
    FormatModifierSpec::new("L", ModifierKind::Clients, true),
    FormatModifierSpec::new("O", ModifierKind::Options, true),
    FormatModifierSpec::new("V", ModifierKind::Environments, true),
    FormatModifierSpec::new("C", ModifierKind::ContentSearch, true),
    FormatModifierSpec::new("R", ModifierKind::Repeat, true),
    FormatModifierSpec::new("I", ModifierKind::Interrogate, true),
    FormatModifierSpec::new("A", ModifierKind::Animate, true),
];

#[cfg(test)]
pub(crate) fn format_modifier_names() -> impl Iterator<Item = &'static str> {
    FORMAT_MODIFIER_SPECS.iter().map(|spec| spec.text)
}

struct Modifier {
    kind: ModifierKind,
    text: &'static str,
    args: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopTarget {
    Sessions,
    Windows,
    Panes,
}

impl LoopTarget {
    const fn format_type(self) -> FormatType {
        match self {
            Self::Sessions => FormatType::Session,
            Self::Windows => FormatType::Window,
            Self::Panes => FormatType::Pane,
        }
    }
}

struct LoopVariables<'a> {
    context: &'a StatusContext<'a>,
    format_type: FormatType,
    dynamic: BTreeMap<String, String>,
}

impl FormatVariables for LoopVariables<'_> {
    fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        if let Some(value) = self.dynamic.get(name) {
            return Some(Cow::Borrowed(value));
        }
        let spec = format_variable(name)?;
        Some(self.context.resolve(spec, self.format_type))
    }

    fn variable_kind(&self, name: &str) -> Option<FormatKind> {
        if self.dynamic.contains_key(name) {
            Some(FormatKind::String)
        } else {
            format_variable(name).map(|spec| spec.kind)
        }
    }

    fn values(&self) -> &StatusContext<'_> {
        self.context
    }

    fn format_type(&self) -> FormatType {
        self.format_type
    }
}

#[derive(Clone, Copy)]
enum Comparison<'a> {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Match(&'a str),
}

const fn comparison_text(comparison: Comparison<'_>) -> &'static str {
    match comparison {
        Comparison::Equal => "==",
        Comparison::NotEqual => "!=",
        Comparison::Less => "<",
        Comparison::Greater => ">",
        Comparison::LessEqual => "<=",
        Comparison::GreaterEqual => ">=",
        Comparison::Match(_) => "m",
    }
}

#[derive(Clone, Copy)]
enum LoopSort {
    Index,
    Name,
    Activity,
    Order,
    Creation,
    Z,
}

#[derive(Default)]
struct TimeFlags<'a> {
    enabled: bool,
    pretty: bool,
    relative: bool,
    difference: bool,
    format: Option<&'a str>,
}

#[derive(Default)]
struct ModifierFlags<'a> {
    literal: bool,
    basename: bool,
    dirname: bool,
    length: bool,
    width: bool,
    expand: bool,
    expand_time: bool,
    character: bool,
    not: bool,
    not_not: bool,
    bool_op: Option<bool>,
    comparison: Option<Comparison<'a>>,
    substitutions: Vec<&'a Modifier>,
    limit: Option<(isize, Option<&'a str>)>,
    padding: isize,
    expression: Option<&'a Modifier>,
    colour: Option<&'a str>,
    name_window: bool,
    name_session: bool,
    loop_sessions: bool,
    loop_windows: bool,
    loop_panes: bool,
    loop_clients: bool,
    loop_options: Option<&'a str>,
    loop_environment: Option<&'a str>,
    loop_sort: Option<LoopSort>,
    loop_reversed: bool,
    content_search: Option<&'a str>,
    repeat: bool,
    interrogate: Option<&'a str>,
    cycle: Option<u64>,
    time: TimeFlags<'a>,
    quote_shell: bool,
    quote_single: bool,
    quote_style: bool,
    quote_arguments: bool,
}

impl<'a> ModifierFlags<'a> {
    fn from_modifiers(modifiers: &'a [Modifier]) -> Self {
        let mut flags = Self::default();
        for modifier in modifiers {
            match modifier.kind {
                ModifierKind::Literal => flags.literal = true,
                ModifierKind::Basename => flags.basename = true,
                ModifierKind::Dirname => flags.dirname = true,
                ModifierKind::Length => flags.length = true,
                ModifierKind::Width => flags.width = true,
                ModifierKind::Expand => flags.expand = true,
                ModifierKind::ExpandTime => flags.expand_time = true,
                ModifierKind::Character => flags.character = true,
                ModifierKind::Not => flags.not = true,
                ModifierKind::NotNot => flags.not_not = true,
                ModifierKind::And => flags.bool_op = Some(true),
                ModifierKind::Or => flags.bool_op = Some(false),
                ModifierKind::Equal => flags.comparison = Some(Comparison::Equal),
                ModifierKind::NotEqual => flags.comparison = Some(Comparison::NotEqual),
                ModifierKind::Less => flags.comparison = Some(Comparison::Less),
                ModifierKind::Greater => flags.comparison = Some(Comparison::Greater),
                ModifierKind::LessEqual => flags.comparison = Some(Comparison::LessEqual),
                ModifierKind::GreaterEqual => flags.comparison = Some(Comparison::GreaterEqual),
                ModifierKind::Match => {
                    flags.comparison = Some(Comparison::Match(
                        modifier.args.first().map_or("", String::as_str),
                    ));
                }
                ModifierKind::Time => {
                    flags.time.enabled = true;
                    if let Some(time_flags) = modifier.args.first() {
                        if time_flags.contains('p') {
                            flags.time.pretty = true;
                        } else if time_flags.contains('r') {
                            flags.time.relative = true;
                        } else if time_flags.contains('d') {
                            flags.time.difference = true;
                        } else if time_flags.contains('f')
                            && let Some(format) = modifier.args.get(1)
                        {
                            flags.time.format = Some(format.as_str());
                        }
                    }
                }
                ModifierKind::Quote => {
                    let Some(quote_flags) = modifier.args.first() else {
                        flags.quote_shell = true;
                        continue;
                    };
                    if quote_flags.contains('s') {
                        flags.quote_single = true;
                    } else if quote_flags.contains('e') || quote_flags.contains('h') {
                        flags.quote_style = true;
                    } else if quote_flags.contains('a') {
                        flags.quote_arguments = true;
                    }
                }
                ModifierKind::Substitute if modifier.args.len() >= 2 => {
                    flags.substitutions.push(modifier);
                }
                ModifierKind::Limit => {
                    if let Some(limit) = modifier.args.first() {
                        let limit = limit
                            .parse::<isize>()
                            .ok()
                            .filter(|limit| (-FORMAT_MAX_WIDTH..=FORMAT_MAX_WIDTH).contains(limit))
                            .unwrap_or_default();
                        let marker = modifier
                            .args
                            .get(1)
                            .map(String::as_str)
                            .or_else(|| flags.limit.as_ref().and_then(|(_, marker)| *marker));
                        flags.limit = Some((limit, marker));
                    }
                }
                ModifierKind::Padding => {
                    flags.padding = modifier
                        .args
                        .first()
                        .and_then(|width| width.parse::<isize>().ok())
                        .filter(|width| (-FORMAT_MAX_WIDTH..=FORMAT_MAX_WIDTH).contains(width))
                        .unwrap_or_default();
                }
                ModifierKind::Expression if (1..=3).contains(&modifier.args.len()) => {
                    flags.expression = Some(modifier);
                }
                ModifierKind::Substitute | ModifierKind::Expression => {}
                ModifierKind::Colour => {
                    flags.colour = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Interrogate => {
                    if let Some(word) = modifier.args.first()
                        && word.contains(['c', 'f', 'e'])
                    {
                        flags.interrogate = Some(word.as_str());
                    }
                }
                ModifierKind::NameExists => {
                    if modifier
                        .args
                        .first()
                        .is_none_or(|value| value.contains('w'))
                    {
                        flags.name_window = true;
                    } else if modifier
                        .args
                        .first()
                        .is_some_and(|value| value.contains('s'))
                    {
                        flags.name_session = true;
                    }
                }
                ModifierKind::Sessions => {
                    flags.loop_sessions = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Index
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Index
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Windows => {
                    flags.loop_windows = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Order
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Order
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Panes => {
                    flags.loop_panes = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Index
                    } else if value.contains('z') {
                        LoopSort::Z
                    } else {
                        LoopSort::Creation
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Clients => {
                    flags.loop_clients = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Order
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Order
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Options => {
                    flags.loop_options = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Environments => {
                    flags.loop_environment = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::ContentSearch => {
                    flags.content_search = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Repeat => flags.repeat = true,
                ModifierKind::Animate => {
                    flags.cycle = Some(
                        modifier
                            .args
                            .first()
                            .and_then(|count| count.parse::<u64>().ok())
                            .filter(|count| (1..=100).contains(count))
                            .unwrap_or(1),
                    );
                }
            }
        }
        flags
    }
}

fn insert_window_neighbour_user_options(
    dynamic: &mut BTreeMap<String, String>,
    universe: &FormatUniverseRef<'_>,
    neighbour: &FormatLoopItem,
    prefix: &str,
) {
    let Some(options) = universe.window_user_options(&neighbour.context.window_id) else {
        return;
    };
    for (name, value) in options.iter() {
        dynamic.insert(format!("{prefix}{name}"), value.clone());
    }
}

fn client_loop_scoped(name: &str) -> bool {
    format_variable(name).is_some_and(|spec| {
        spec.scope == FormatScope::Client && spec.backing == FormatBacking::StatusHook
    })
}

fn sort_client_loop_rows(rows: &mut [FormatClientRow], sort: Option<LoopSort>, reversed: bool) {
    if !matches!(sort, Some(LoopSort::Name | LoopSort::Activity)) {
        if reversed {
            rows.reverse();
        }
        return;
    }
    rows.sort_by(|left, right| {
        let ordering = match sort {
            Some(LoopSort::Activity) => right.activity.cmp(&left.activity),
            _ => Ordering::Equal,
        }
        .then_with(|| left.name.cmp(&right.name));
        if reversed {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

fn sort_loop_items(items: &mut [&FormatLoopItem], target: LoopTarget, sort: Option<LoopSort>) {
    match (target, sort) {
        (LoopTarget::Sessions, Some(LoopSort::Name)) => items.sort_by(|left, right| {
            left.metadata
                .session_name
                .cmp(&right.metadata.session_name)
                .then_with(|| {
                    session_number(&left.context.session_id)
                        .cmp(&session_number(&right.context.session_id))
                })
        }),
        (LoopTarget::Sessions, Some(LoopSort::Activity)) => items.sort_by(|left, right| {
            right
                .metadata
                .session_sort_activity
                .cmp(&left.metadata.session_sort_activity)
                .then_with(|| left.metadata.session_name.cmp(&right.metadata.session_name))
        }),
        (LoopTarget::Sessions, _) => {
            items.sort_by_key(|item| session_number(&item.context.session_id));
        }
        (LoopTarget::Windows, Some(LoopSort::Name)) => items.sort_by(|left, right| {
            left.metadata
                .window_name
                .cmp(&right.metadata.window_name)
                .then_with(|| left.metadata.window_index.cmp(&right.metadata.window_index))
        }),
        (LoopTarget::Windows, _) => items.sort_by_key(|item| item.metadata.window_index),
        (LoopTarget::Panes, Some(LoopSort::Index)) => {
            items.sort_by_key(|item| item.metadata.pane_index);
        }
        (LoopTarget::Panes, Some(LoopSort::Z)) => items.sort_by_key(|item| {
            (
                item.metadata.pane_z.unwrap_or_default(),
                pane_number(&item.context.pane_id),
            )
        }),
        (LoopTarget::Panes, _) => items.sort_by_key(|item| pane_number(&item.context.pane_id)),
    }
}

fn session_number(value: &str) -> u64 {
    value
        .strip_prefix('$')
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
}

fn parse_expression_number(value: &str) -> Option<f64> {
    if value.is_empty() {
        return Some(0.0);
    }
    let value = value.trim_start();
    if value.trim_end() != value {
        return None;
    }
    value.parse::<f64>().ok()
}

fn pad_value(value: &RawText, width: isize) -> RawText {
    let target = width.unsigned_abs();
    let current = format_byte_width(value.as_bytes());
    if current >= target {
        return value.clone();
    }
    let padding = " ".repeat(target - current);
    let mut output = Vec::with_capacity(value.byte_len() + padding.len());
    if width < 0 {
        output.extend_from_slice(padding.as_bytes());
        output.extend_from_slice(value.as_bytes());
    } else {
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(padding.as_bytes());
    }
    RawText::from_bytes(output)
}

fn format_colour(value: &str, flags: &str) -> String {
    if flags.contains('f') || flags.contains('b') {
        if value.eq_ignore_ascii_case("none") {
            return "\u{1b}[0m".to_owned();
        }
        return parse_tmux_colour(value)
            .and_then(|colour| colour_escape(colour, flags.contains('b')))
            .unwrap_or_default();
    }
    parse_tmux_colour(value)
        .and_then(colour_rgb)
        .map_or_else(String::new, |colour| format!("{colour:06x}"))
}

fn colour_rgb(colour: TmuxColour) -> Option<u32> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => Some(indexed_colour_rgb(index)),
        TmuxColour::Rgb(colour) => Some(colour),
        TmuxColour::Default | TmuxColour::Terminal | TmuxColour::Theme(_) => None,
    }
}

fn colour_escape(colour: TmuxColour, background: bool) -> Option<String> {
    let colour = match colour {
        TmuxColour::Theme(index) => {
            let basic = [0, 7, 7, 0, 2, 3, 1, 4, 6, 5].get(usize::from(index))?;
            TmuxColour::Basic(*basic)
        }
        colour => colour,
    };
    let base = if background { 40 } else { 30 };
    Some(match colour {
        TmuxColour::Basic(index) if index < 8 => format!("\u{1b}[{}m", base + index),
        TmuxColour::Basic(index) => format!("\u{1b}[{}m", base + 60 + index - 8),
        TmuxColour::Indexed(index) => format!("\u{1b}[{};5;{index}m", base + 8),
        TmuxColour::Rgb(colour) => format!(
            "\u{1b}[{};2;{};{};{}m",
            base + 8,
            (colour >> 16) & 0xff,
            (colour >> 8) & 0xff,
            colour & 0xff
        ),
        TmuxColour::Default | TmuxColour::Terminal => format!("\u{1b}[{}m", base + 9),
        TmuxColour::Theme(_) => return None,
    })
}

fn shorthand(character: char) -> Option<&'static str> {
    Some(match character {
        'D' => "pane_id",
        'F' => "window_flags",
        'H' => "host",
        'I' => "window_index",
        'P' => "pane_index",
        'S' => "session_name",
        'T' => "pane_title",
        'W' => "window_name",
        'h' => "host_short",
        _ => return None,
    })
}

const fn is_modifier_end(character: u8) -> bool {
    character == b';' || character == b':'
}

fn find_plain_group_end(text: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, character) in text[start..].char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth -= 1;
            if depth == 0 {
                return Some(start + offset);
            }
        }
    }
    None
}

fn find_style_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            position = find_format_end(text, position)? + 1;
            continue;
        }
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'(') {
            position = find_plain_group_end(text, position + 2, '(', ')')? + 1;
            continue;
        }
        if bytes[position] == b']' {
            return Some(position);
        }
        position += 1;
    }
    None
}

fn find_format_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(position);
            }
        }
        position += 1;
    }
    None
}

fn option_loop_key(loop_flags: &str, values: &StatusContext) -> Option<OptionRowsKey> {
    let loop_flags = if loop_flags.is_empty() {
        "s"
    } else {
        loop_flags
    };
    if loop_flags.contains('v') {
        return Some(OptionRowsKey::Server);
    }
    let global = loop_flags.contains('g');
    if loop_flags.contains('w') {
        return if global {
            Some(OptionRowsKey::GlobalWindow)
        } else {
            parse_window(&values.window_id).map(OptionRowsKey::Window)
        };
    }
    if loop_flags.contains('s') {
        return if global {
            Some(OptionRowsKey::GlobalSession)
        } else {
            parse_session(&values.session_id).map(OptionRowsKey::Session)
        };
    }
    if loop_flags.contains('p') {
        return if global {
            None
        } else {
            parse_pane(&values.pane_id).map(OptionRowsKey::Pane)
        };
    }
    global.then_some(OptionRowsKey::GlobalSession)
}

fn environment_loop_variables(
    row: &FormatEnvironRow,
    index: usize,
    last: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[0].to_owned(),
            bool_string(row.hidden).to_owned(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[1].to_owned(),
            row.name.clone(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[2].to_owned(),
            bool_string(row.removed).to_owned(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[3].to_owned(),
            row.value.to_string(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[4].to_owned(),
            index.to_string(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[5].to_owned(),
            bool_string(last).to_owned(),
        ),
    ])
}

fn option_loop_variables(
    row: &FormatOptionRow,
    index: usize,
    last: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (OPTION_LOOP_CONTEXT_FORMATS[0].to_owned(), index.to_string()),
        (
            OPTION_LOOP_CONTEXT_FORMATS[1].to_owned(),
            bool_string(last).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[2].to_owned(),
            row.array_count.to_string(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[3].to_owned(),
            bool_string(row.array_first).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[4].to_owned(),
            row.array_index.clone(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[5].to_owned(),
            row.array_key.clone(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[6].to_owned(),
            bool_string(row.array_last).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[7].to_owned(),
            bool_string(row.is_array).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[8].to_owned(),
            bool_string(row.is_hook).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[9].to_owned(),
            bool_string(row.is_user).to_owned(),
        ),
        (OPTION_LOOP_CONTEXT_FORMATS[10].to_owned(), row.name.clone()),
        (
            OPTION_LOOP_CONTEXT_FORMATS[11].to_owned(),
            row.value.clone(),
        ),
    ])
}

fn styled_display_width(bytes: &[u8]) -> usize {
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            width += leading;
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    return 0;
                };
                position = end + 1;
            }
            continue;
        }
        let (length, unit, _) = next_format_unit(bytes, position);
        width += unit;
        position += length;
    }
    width
}

fn leading_hashes(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut count = 0usize;
    while bytes.get(start + count) == Some(&b'#') {
        count += 1;
    }
    if count == 0 {
        return (start, 0);
    }
    if bytes.get(start + count) != Some(&b'[') {
        return (start + count, count.div_ceil(2));
    }
    if count.is_multiple_of(2) {
        (start + count, count / 2)
    } else {
        (start + count - 1, count / 2)
    }
}

fn find_measured_style_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut brackets = 0isize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            brackets += 1;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            brackets -= 1;
        }
        if bytes[position] == b']' && brackets == 0 {
            return Some(position);
        }
        position += 1;
    }
    None
}

fn find_modifier_argument(text: &str, start: usize, wrapper: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0isize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth -= 1;
            position += 1;
            continue;
        }
        if depth == 0
            && (is_modifier_end(bytes[position]) || (wrapper != 0 && bytes[position] == wrapper))
        {
            return Some(position);
        }
        position += 1;
    }
    None
}

fn split_once_top(text: &str, separator: char) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    let separator = separator as u8;
    let mut depth = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' && depth > 0 {
            depth -= 1;
        } else if bytes[position] == separator && depth == 0 {
            return (&text[..position], Some(&text[position + 1..]));
        }
        position += 1;
    }
    (text, None)
}

fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut values = Vec::new();
    let mut rest = text;
    loop {
        let (value, next) = split_once_top(rest, separator);
        values.push(value);
        let Some(next) = next else {
            break;
        };
        rest = next;
    }
    values
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
        }
        if depth == 0
            && bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            output.push(bytes[position + 1] as char);
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth = depth.saturating_sub(1);
        }
        let character = text[position..].chars().next().unwrap_or_default();
        output.push(character);
        position += character.len_utf8();
    }
    output
}

pub fn format_true(value: &str) -> bool {
    !value.is_empty() && value != "0"
}

fn trim_end_slashes(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .rposition(|byte| *byte != b'/')
        .map_or(0, |index| index + 1);
    &value[..end]
}

fn basename(value: &[u8]) -> RawText {
    if value.is_empty() {
        return RawText::from(".");
    }
    let trimmed = trim_end_slashes(value);
    if trimmed.is_empty() {
        return RawText::from("/");
    }
    let start = trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index + 1);
    RawText::from_bytes(&trimmed[start..])
}

fn dirname(value: &[u8]) -> RawText {
    if value.is_empty() {
        return RawText::from(".");
    }
    let trimmed = trim_end_slashes(value);
    if trimmed.is_empty() {
        return RawText::from("/");
    }
    let Some(index) = trimmed.iter().rposition(|byte| *byte == b'/') else {
        return RawText::from(".");
    };
    let parent = trim_end_slashes(&trimmed[..index]);
    if parent.is_empty() {
        RawText::from("/")
    } else {
        RawText::from_bytes(parent)
    }
}

fn quote_shell(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len() * 2);
    for byte in value {
        if b"|&;<>(){}$`\\\"'*?[# =%\n\t".contains(byte) {
            output.push(b'\\');
        }
        output.push(*byte);
    }
    RawText::from_bytes(output)
}

fn quote_single(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len() + 2);
    output.push(b'\'');
    for byte in value {
        if *byte == b'\'' {
            output.extend_from_slice(b"'\\''");
        } else {
            output.push(*byte);
        }
    }
    output.push(b'\'');
    RawText::from_bytes(output)
}

fn quote_style(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len());
    for byte in value {
        if *byte == b'#' {
            output.push(b'#');
        }
        output.push(*byte);
    }
    RawText::from_bytes(output)
}

pub(crate) fn quote_argument(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    let double = value
        .chars()
        .any(|character| " #';${}%".contains(character));
    let single = !double && value.chars().any(|character| " \"".contains(character));
    if value != " " && value.len() == 1 && (double || single || value == "~") {
        return format!("\\{value}");
    }
    let mut escaped = String::with_capacity(value.len() * 2);
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\u{8}' => escaped.push_str("\\b"),
            '\u{7}' => escaped.push_str("\\a"),
            '\u{b}' => escaped.push_str("\\v"),
            '\t' => escaped.push_str("\\t"),
            '\u{c}' => escaped.push_str("\\f"),
            '\0' => {
                escaped.push_str("\\0");
                if characters
                    .peek()
                    .is_some_and(|next| matches!(next, '0'..='7'))
                {
                    escaped.push_str("00");
                }
            }
            '\\' => escaped.push_str("\\\\"),
            '"' if double => escaped.push_str("\\\""),
            '$' if double
                && characters.peek().is_some_and(|next| {
                    next.is_ascii_alphabetic() || matches!(next, '_' | '{')
                }) =>
            {
                escaped.push_str("\\$");
            }
            character if character.is_ascii_control() => {
                let _ = write!(&mut escaped, "\\{:03o}", u32::from(character));
            }
            character => escaped.push(character),
        }
    }
    if double {
        if escaped.starts_with('~') {
            format!("\"\\{escaped}\"")
        } else {
            format!("\"{escaped}\"")
        }
    } else if single {
        format!("'{escaped}'")
    } else if escaped.starts_with('~') {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

const REGEX_CACHE_ENTRIES: usize = 64;

type RegexCache = VecDeque<((String, bool), Option<Regex>)>;

static REGEX_CACHE: LazyLock<Mutex<RegexCache>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(REGEX_CACHE_ENTRIES)));

fn cached_regex(pattern: &str, insensitive: bool) -> Option<Regex> {
    if let Some((_, regex)) = REGEX_CACHE
        .lock()
        .iter()
        .find(|((cached, flag), _)| *flag == insensitive && cached == pattern)
    {
        return regex.clone();
    }
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(insensitive)
        .dot_matches_new_line(true)
        .build()
        .ok();
    let mut cache = REGEX_CACHE.lock();
    if cache.len() == REGEX_CACHE_ENTRIES {
        cache.pop_front();
    }
    cache.push_back(((pattern.to_owned(), insensitive), regex.clone()));
    regex
}

fn match_value(pattern: &str, text: &str, flags: &str) -> String {
    if flags.contains('p') {
        return fuzzy_positions(pattern, text).unwrap_or_default();
    }
    if flags.contains('z') {
        return bool_string(fuzzy_positions(pattern, text).is_some()).to_owned();
    }
    let matched = if flags.contains('r') {
        cached_regex(pattern, flags.contains('i')).is_some_and(|regex| regex.is_match(text))
    } else {
        Pattern::new(pattern).is_ok_and(|pattern| {
            pattern.matches_with(
                text,
                MatchOptions {
                    case_sensitive: !flags.contains('i'),
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                },
            )
        })
    };
    bool_string(matched).to_owned()
}

#[derive(Clone, Copy)]
struct FuzzyCharacter {
    value: char,
    column: usize,
    width: usize,
    align: zz_protocol::TmuxAlign,
}

#[derive(Clone, Copy)]
struct FuzzyTerm<'a> {
    inverse: bool,
    exact: bool,
    prefix: bool,
    suffix: bool,
    text: &'a str,
}

fn fuzzy_positions(pattern: &str, text: &str) -> Option<String> {
    let characters = fuzzy_characters(text);
    if pattern
        .chars()
        .all(|character| matches!(character, ' ' | '|'))
    {
        return Some(String::new());
    }
    let (_, selected) = fuzzy_best(pattern, &characters)?;
    Some(
        characters
            .iter()
            .zip(selected)
            .filter(|(_, selected)| *selected)
            .flat_map(|(character, _)| character.column..character.column + character.width)
            .map(|column| column.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn fuzzy_best(pattern: &str, characters: &[FuzzyCharacter]) -> Option<(i32, Vec<bool>)> {
    let fold = !pattern.bytes().any(|byte| byte.is_ascii_uppercase());
    let mut best: Option<(i32, Vec<bool>)> = None;
    for group in pattern.split('|') {
        let mut any = false;
        let mut score = 0i32;
        let mut selected = vec![false; characters.len()];
        let mut matched_group = true;
        for raw in group.split(' ').filter(|term| !term.is_empty()) {
            let Some(term) = fuzzy_term(raw) else {
                matched_group = false;
                break;
            };
            any = true;
            let found = if term.exact {
                fuzzy_exact(&term, characters, fold)
            } else {
                fuzzy_subsequence(term.text, characters, fold)
            };
            if term.inverse {
                if found.is_some() {
                    matched_group = false;
                    break;
                }
                continue;
            }
            let Some((term_score, positions)) = found else {
                matched_group = false;
                break;
            };
            score = score.saturating_add(term_score);
            for position in positions {
                selected[position] = true;
            }
        }
        if !any || !matched_group {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, selected));
        }
    }
    best
}

pub fn fuzzy_match_columns(pattern: &str, text: &str, width: usize) -> Option<(u32, Vec<usize>)> {
    use zz_protocol::TmuxAlign;
    if width == 0 {
        return None;
    }
    if pattern
        .chars()
        .all(|character| matches!(character, ' ' | '|'))
    {
        return Some((0, Vec::new()));
    }
    let characters = fuzzy_markup_characters(text);
    let (score, selected) = fuzzy_best(pattern, &characters)?;
    let total = |align: TmuxAlign| {
        characters
            .iter()
            .filter(|character| character.align == align)
            .map(|character| character.width)
            .sum::<usize>()
    };
    let (left, centre, right, absolute) = (
        total(TmuxAlign::Left),
        total(TmuxAlign::Centre),
        total(TmuxAlign::Right),
        total(TmuxAlign::AbsoluteCentre),
    );
    let (mut wl, mut wc, mut wr) = (left, centre, right);
    while wl + wc + wr > width {
        if wc > 0 {
            wc -= 1;
        } else if wr > 0 {
            wr -= 1;
        } else {
            wl -= 1;
        }
    }
    let wa = absolute.min(width);
    let area = |align: TmuxAlign| match align {
        TmuxAlign::Right => (width - wr, right - wr, wr),
        TmuxAlign::Centre => (
            (wl + (width - wr - wl) / 2).saturating_sub(wc / 2),
            (centre / 2).saturating_sub(wc / 2),
            wc,
        ),
        TmuxAlign::AbsoluteCentre => ((width - wa) / 2, 0, wa),
        TmuxAlign::Left | TmuxAlign::Default => (0, 0, wl),
    };
    let mut columns = Vec::new();
    for (character, _) in characters.iter().zip(selected).filter(|(_, hit)| *hit) {
        let (start, source, visible) = area(character.align);
        if character.column < source || character.column >= source + visible {
            continue;
        }
        let column = start + character.column - source;
        columns.extend((column..column + character.width).filter(|column| *column < width));
    }
    columns.sort_unstable();
    columns.dedup();
    Some((u32::try_from(score.max(0)).unwrap_or(0), columns))
}

fn fuzzy_characters(text: &str) -> Vec<FuzzyCharacter> {
    let mut column = 0usize;
    text.chars()
        .filter_map(|value| {
            if value.is_ascii_control() {
                return None;
            }
            let width = value.width().unwrap_or_default();
            let character = FuzzyCharacter {
                value,
                column,
                width,
                align: zz_protocol::TmuxAlign::Left,
            };
            column = column.saturating_add(width);
            Some(character)
        })
        .collect()
}

fn fuzzy_markup_characters(text: &str) -> Vec<FuzzyCharacter> {
    use zz_protocol::TmuxAlign;
    let mut characters = Vec::new();
    let mut offsets = [0usize; 5];
    let mut align = TmuxAlign::Left;
    let slot = |align: TmuxAlign| match align {
        TmuxAlign::Default | TmuxAlign::Left => 0,
        TmuxAlign::Centre => 1,
        TmuxAlign::Right => 2,
        TmuxAlign::AbsoluteCentre => 3,
    };
    let mut push = |value: char, align: TmuxAlign, characters: &mut Vec<FuzzyCharacter>| {
        let width = value.width().unwrap_or_default();
        let offset = &mut offsets[slot(align)];
        characters.push(FuzzyCharacter {
            value,
            column: *offset,
            width,
            align,
        });
        *offset += width;
    };
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let hashes = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            if bytes.get(index + hashes) != Some(&b'[') {
                for _ in 0..hashes.div_ceil(2) {
                    push('#', align, &mut characters);
                }
                index += hashes;
                continue;
            }
            for _ in 0..hashes / 2 {
                push('#', align, &mut characters);
            }
            if hashes % 2 == 0 {
                push('[', align, &mut characters);
                index += hashes + 1;
                continue;
            }
            let start = index + hashes + 1;
            let Some(end) = text[start..].find(']') else {
                break;
            };
            if let Some(style) = zz_protocol::parse_style(&text[start..start + end])
                && let Some(next) = style.align
            {
                align = if next == TmuxAlign::Default {
                    TmuxAlign::Left
                } else {
                    next
                };
            }
            index = start + end + 1;
            continue;
        }
        let value = text[index..].chars().next().expect("character boundary");
        index += value.len_utf8();
        if !value.is_ascii_control() {
            push(value, align, &mut characters);
        }
    }
    characters
}

fn fuzzy_term(raw: &str) -> Option<FuzzyTerm<'_>> {
    let mut text = raw;
    let inverse = text.starts_with('!');
    if inverse {
        text = &text[1..];
    }
    if text.is_empty() {
        return None;
    }
    let mut exact = false;
    let mut prefix = false;
    if text.starts_with('\'') {
        exact = true;
        text = &text[1..];
    } else if text.starts_with('^') {
        exact = true;
        prefix = true;
        text = &text[1..];
    }
    if text.is_empty() {
        return None;
    }
    let suffix = text.ends_with('$');
    if suffix {
        exact = true;
        text = &text[..text.len() - 1];
    }
    if text.is_empty() {
        return None;
    }
    Some(FuzzyTerm {
        inverse,
        exact: exact || inverse,
        prefix,
        suffix,
        text,
    })
}

fn fuzzy_subsequence(
    pattern: &str,
    text: &[FuzzyCharacter],
    fold: bool,
) -> Option<(i32, Vec<usize>)> {
    let pattern = pattern.chars().collect::<Vec<_>>();
    if pattern.is_empty() || text.is_empty() {
        return None;
    }
    let mut positions = Vec::with_capacity(pattern.len());
    let mut text_index = 0usize;
    for wanted in &pattern {
        while text_index < text.len() && !fuzzy_equal(*wanted, text[text_index].value, fold) {
            text_index += 1;
        }
        if text_index == text.len() {
            return None;
        }
        positions.push(text_index);
        text_index += 1;
    }
    let mut candidate = *positions.last()?;
    for pattern_index in (0..pattern.len()).rev() {
        loop {
            if fuzzy_equal(pattern[pattern_index], text[candidate].value, fold) {
                positions[pattern_index] = candidate;
                break;
            }
            candidate = candidate.checked_sub(1)?;
        }
        if pattern_index != 0 {
            candidate = candidate.checked_sub(1)?;
        }
    }
    let score = fuzzy_subsequence_score(&positions, text);
    Some((score, positions))
}

fn fuzzy_exact(
    term: &FuzzyTerm<'_>,
    text: &[FuzzyCharacter],
    fold: bool,
) -> Option<(i32, Vec<usize>)> {
    let pattern = term.text.chars().collect::<Vec<_>>();
    if pattern.is_empty() || pattern.len() > text.len() {
        return None;
    }
    let starts = if term.prefix && term.suffix {
        if pattern.len() != text.len() {
            return None;
        }
        0..1
    } else if term.prefix {
        0..1
    } else if term.suffix {
        let start = text.len() - pattern.len();
        start..start + 1
    } else {
        0..text.len() - pattern.len() + 1
    };
    let mut best: Option<(i32, Vec<usize>)> = None;
    for start in starts {
        if !pattern
            .iter()
            .enumerate()
            .all(|(offset, wanted)| fuzzy_equal(*wanted, text[start + offset].value, fold))
        {
            continue;
        }
        let score = fuzzy_exact_score(start, pattern.len(), text, term.prefix, term.suffix);
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, (start..start + pattern.len()).collect()));
        }
    }
    best
}

fn fuzzy_subsequence_score(positions: &[usize], text: &[FuzzyCharacter]) -> i32 {
    let Some(first) = positions.first().copied() else {
        return 0;
    };
    let mut score = 0i32;
    if first == 0 {
        score += 12;
    } else {
        if fuzzy_boundary(text[first - 1].value) {
            score += 8;
        }
        score -= fuzzy_score_number(first.min(10));
    }
    for pair in positions.windows(2) {
        if pair[1] == pair[0] + 1 {
            score += 6;
        } else if fuzzy_boundary(text[pair[1] - 1].value) {
            score += 8;
        }
    }
    let span = positions[positions.len() - 1] - first + 1;
    score.saturating_sub(fuzzy_score_number(span - positions.len()))
}

fn fuzzy_exact_score(
    start: usize,
    length: usize,
    text: &[FuzzyCharacter],
    prefix: bool,
    suffix: bool,
) -> i32 {
    let mut score = 1_000i32.saturating_add(fuzzy_score_number(length).saturating_mul(6));
    if prefix {
        score = score.saturating_add(200);
    }
    if suffix {
        score = score.saturating_add(100);
    }
    if start == 0 {
        score = score.saturating_add(12);
    } else if fuzzy_boundary(text[start - 1].value) {
        score = score.saturating_add(8);
    }
    score = score.saturating_sub(fuzzy_score_number(start.min(10)));
    if !prefix && !suffix {
        score = score.saturating_sub(fuzzy_score_number(text.len() - (start + length)));
    }
    score
}

fn fuzzy_equal(left: char, right: char, fold: bool) -> bool {
    if fold && left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn fuzzy_boundary(character: char) -> bool {
    character.is_ascii() && " -_/.:".contains(character)
}

fn fuzzy_score_number(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn substitute(text: &str, pattern: &str, replacement: &str, insensitive: bool) -> String {
    if text.is_empty() || pattern.is_empty() {
        return text.to_owned();
    }
    let Some(regex) = cached_regex(pattern, insensitive) else {
        return text.to_owned();
    };
    let mut output = String::with_capacity(text.len());
    let mut start = 0usize;
    let mut last = 0usize;
    let mut empty = false;
    while start <= text.len() {
        let Some(captures) = regex.captures(&text[start..]) else {
            output.push_str(&text[start..]);
            break;
        };
        let matched = captures.get(0).expect("a regex capture has a whole match");
        let match_start = start + matched.start();
        let match_end = start + matched.end();
        output.push_str(&text[last..match_start]);
        if pattern.starts_with('^') {
            output.push_str(&expand_substitution(replacement, &captures));
            output.push_str(&text[match_end..]);
            break;
        }
        if empty || match_start != last || !matched.is_empty() {
            output.push_str(&expand_substitution(replacement, &captures));
            last = match_end;
            start = match_end;
            empty = false;
        } else {
            last = match_end;
            let Some(character) = text[match_end..].chars().next() else {
                break;
            };
            start = match_end + character.len_utf8();
            empty = true;
        }
    }
    output
}

fn expand_substitution(replacement: &str, captures: &Captures<'_>) -> String {
    let mut output = String::with_capacity(replacement.len());
    let mut characters = replacement.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let Some(next) = characters.next() else {
            output.push('\\');
            break;
        };
        if let Some(index) = next.to_digit(10).map(|index| index as usize)
            && let Some(value) = captures.get(index)
            && !value.as_str().is_empty()
        {
            output.push_str(value.as_str());
        } else {
            output.push(next);
        }
    }
    output
}

fn next_format_unit(value: &[u8], index: usize) -> (usize, usize, bool) {
    let byte = value[index];
    let size = match byte {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    };
    if size != 0
        && let Some(slice) = value.get(index..index + size)
        && let Ok(text) = std::str::from_utf8(slice)
        && let Some(character) = text.chars().next()
        && character.len_utf8() == size
    {
        return (size, character.width().unwrap_or_default(), true);
    }
    if (0x20..0x7f).contains(&byte) {
        (1, 1, true)
    } else {
        (1, 0, false)
    }
}

#[must_use]
pub fn utf8_sanitize(value: &[u8]) -> String {
    let mut output = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        let (length, width, printable) = next_format_unit(value, index);
        if length > 1 {
            for _ in 0..width {
                output.push('_');
            }
        } else if printable {
            output.push(char::from(value[index]));
        } else {
            output.push('_');
        }
        index += length;
    }
    output
}

#[must_use]
pub fn sanitize_client_output(output: &RawText) -> RawText {
    let mut sanitized = Vec::with_capacity(output.byte_len());
    for (index, line) in output.as_bytes().split(|byte| *byte == b'\n').enumerate() {
        if index != 0 {
            sanitized.push(b'\n');
        }
        sanitized.extend_from_slice(utf8_sanitize(line).as_bytes());
    }
    RawText::from_bytes(sanitized)
}

fn format_byte_width(value: &[u8]) -> usize {
    let mut width = 0;
    let mut index = 0;
    while index < value.len() {
        let (length, columns, _) = next_format_unit(value, index);
        index += length;
        width += columns;
    }
    width
}

fn append_padded(output: &mut Vec<u8>, value: &[u8], width: isize) {
    if width == 0 {
        output.extend_from_slice(value);
        return;
    }
    let padding = width
        .unsigned_abs()
        .saturating_sub(format_byte_width(value));
    if width < 0 {
        output.extend(std::iter::repeat_n(b' ', padding));
    }
    output.extend_from_slice(value);
    if width > 0 {
        output.extend(std::iter::repeat_n(b' ', padding));
    }
}

fn format_trim_left(bytes: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() && width < limit {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            let leading = leading.min(limit - width);
            if leading != 0 {
                if end == position + 1 {
                    out.push(b'#');
                } else {
                    out.extend(std::iter::repeat_n(b'#', 2 * leading));
                }
                width += leading;
            }
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    break;
                };
                out.extend_from_slice(&bytes[position..=end]);
                position = end + 1;
            }
            continue;
        }
        let (length, unit, kept) = next_format_unit(bytes, position);
        if kept && width + unit <= limit {
            out.extend_from_slice(&bytes[position..position + length]);
        }
        width += unit;
        position += length;
    }
    out
}

fn format_trim_right(bytes: &[u8], skip: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            let mut copy_width = leading;
            if width <= skip {
                copy_width = copy_width.saturating_sub(skip - width);
            }
            if copy_width != 0 {
                if end == position + 1 {
                    out.push(b'#');
                } else {
                    out.extend(std::iter::repeat_n(b'#', 2 * copy_width));
                }
            }
            width += leading;
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    break;
                };
                out.extend_from_slice(&bytes[position..=end]);
                position = end + 1;
            }
            continue;
        }
        let (length, unit, kept) = next_format_unit(bytes, position);
        if kept && width >= skip {
            out.extend_from_slice(&bytes[position..position + length]);
        }
        width += unit;
        position += length;
    }
    out
}

fn truncate_value(value: &RawText, limit: isize, marker: Option<&str>) -> RawText {
    if limit == 0 {
        return value.clone();
    }
    let keep = limit.unsigned_abs();
    let bytes = value.as_bytes();
    let trimmed = if limit > 0 {
        format_trim_left(bytes, keep)
    } else {
        let total = styled_display_width(bytes);
        if total <= keep {
            return value.clone();
        }
        format_trim_right(bytes, total - keep)
    };
    if trimmed == bytes {
        return value.clone();
    }
    let mut output = Vec::with_capacity(trimmed.len() + marker.map_or(0, str::len));
    match (limit > 0, marker) {
        (true, Some(marker)) => {
            output.extend_from_slice(&trimmed);
            output.extend_from_slice(marker.as_bytes());
        }
        (false, Some(marker)) => {
            output.extend_from_slice(marker.as_bytes());
            output.extend_from_slice(&trimmed);
        }
        _ => output = trimmed,
    }
    RawText::from_bytes(output)
}

fn format_time_value(value: &str, flags: &TimeFlags<'_>, now: Option<i64>) -> String {
    let Ok(timestamp) = value.parse::<i64>() else {
        return String::new();
    };
    if timestamp <= 0 {
        return String::new();
    }
    if flags.relative {
        return now.map_or_else(String::new, |now| relative_time(timestamp, now));
    }
    if flags.difference {
        return now.map_or_else(String::new, |now| now.saturating_sub(timestamp).to_string());
    }
    if flags.pretty {
        return now.map_or_else(String::new, |now| pretty_time(timestamp, now));
    }
    let Some(time) = crate::localtime::local_time(timestamp) else {
        return String::new();
    };
    format_datetime(&time, flags.format.unwrap_or("%a %b %e %H:%M:%S %Y"))
}

pub(crate) fn pretty_time(timestamp: i64, now: i64) -> String {
    let effective_now = now.max(timestamp);
    let age = effective_now.saturating_sub(timestamp);
    let Some(time) = crate::localtime::local_time(timestamp) else {
        return String::new();
    };
    let Some(now) = crate::localtime::local_time(effective_now) else {
        return String::new();
    };
    let format = if age < 24 * 3600 {
        "%H:%M"
    } else if (time.year() == now.year() && time.month() == now.month()) || age < 28 * 24 * 3600 {
        "%a%d"
    } else if (time.year() == now.year() && time.month() < now.month())
        || (time.year() == now.year() - 1 && time.month() > now.month())
    {
        "%d%b"
    } else {
        "%b%y"
    };
    format_datetime(&time, format)
}

fn relative_time(timestamp: i64, now: i64) -> String {
    if timestamp > now {
        return String::new();
    }
    let age = now - timestamp;
    if age == 0 {
        return "0s".to_owned();
    }
    let days = age / 86_400;
    let hours = age % 86_400 / 3_600;
    let minutes = age % 3_600 / 60;
    let seconds = age % 60;
    if days != 0 {
        if hours != 0 {
            format!("{days}d{hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours != 0 {
        if minutes != 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes != 0 {
        if seconds != 0 {
            format!("{minutes}m{seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

fn format_datetime(time: &DateTime<FixedOffset>, format: &str) -> String {
    let Ok(items) = chrono::format::StrftimeItems::new(format).parse() else {
        return String::new();
    };
    let mut output = String::with_capacity(format.len());
    if write!(&mut output, "{}", time.format_with_items(items.iter())).is_err() {
        String::new()
    } else {
        output
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod engine_tests;

#[cfg(test)]
mod expander_tests;

#[cfg(test)]
mod modifiers_tests;

#[cfg(test)]
mod status_values_tests;

#[cfg(test)]
mod text_tests;

#[cfg(test)]
mod variables_tests;
