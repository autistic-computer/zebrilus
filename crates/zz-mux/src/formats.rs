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
mod matching;
mod modifiers;
mod status_values;
mod text;
mod time;
mod universe;
mod variables;

pub use matching::fuzzy_match_columns;
use matching::*;
#[cfg(test)]
pub(crate) use modifiers::format_modifier_names;
use modifiers::*;
pub use status_values::StatusValues;
use status_values::*;
pub(crate) use text::quote_argument;
use text::*;
pub use text::{format_true, sanitize_client_output, utf8_sanitize};
pub(crate) use time::pretty_time;
use time::*;
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

#[derive(Clone, Copy)]
struct FormatModifierSpec {
    text: &'static str,
    kind: ModifierKind,
    arguments: bool,
}

struct Modifier {
    kind: ModifierKind,
    text: &'static str,
    args: Vec<String>,
}

struct LoopVariables<'a> {
    context: &'a StatusContext<'a>,
    format_type: FormatType,
    dynamic: BTreeMap<String, String>,
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
