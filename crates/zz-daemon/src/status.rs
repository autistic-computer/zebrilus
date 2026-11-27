use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::thread;

use glob::{MatchOptions, Pattern};
use regex::RegexBuilder;
use zz_daemon_client::{home_directory, unmasked::SpawnUnmasked as _};
use zz_mux::{
    FormatClientRow, FormatEnvironRow, FormatJobTag, FormatNeeds, MuxEngine, StatusContext,
    StatusFormats, StatusHooks, StatusRowVariables, TtyTerm, display_width, expand_status,
};
use zz_protocol::{
    ClientEnvironmentBlob, ClientId, MAX_STATUS_ROWS, MAX_STATUS_TEXT_BYTES, MuxSnapshot, PaneId,
    RawText, SessionId, StatusLine, TmuxColour, WindowId,
};
use zz_terminal::{
    CellWidth, CopyModeFacts, PRIVATE_MODE_NUMBERS, ProgramBlockKind, ProgramStatusRecord,
    ProgressBar, TerminalColorScheme, TerminalFacts, TerminalSession, TerminalViewport,
};

use crate::{configure_shell_job_environment, shell_process};

mod facts;
mod formats;
mod hooks;
mod parts;
mod renderer;
mod shell_jobs;
mod terminfo;

use hooks::cycle_clock_ms;
#[cfg(test)]
use hooks::{PANE_OUTPUT_FORMATS, buffer_full, buffer_sample, pane_output_variable};

pub(crate) use hooks::vis_escape;

use renderer::clamp_status_text;
#[cfg(test)]
use renderer::{render, trim_status_left};

pub(crate) use renderer::{host_names, live_status_context, status_context};

#[cfg(test)]
use terminfo::{parse_infocmp_entries, terminfo_entries};

pub(crate) use terminfo::{
    client_terminal_facts, terminfo_is_warm, warm_terminfo_entries, warm_terminfo_entries_using,
};

use facts::{window_cell_pixels, window_scoped};

pub(crate) use facts::{
    BufferFormatFacts, ClientFormatFacts, ClientViewportFacts, FormatFactSource, FormatHookFacts,
    MessageFormatFacts, client_environment_rows,
};

use formats::{
    mode_line_numbers, mode_position_variables, mode_presentation, resolve_theme_colours,
    theme_formats,
};

pub(crate) use formats::{
    MODE_FORMATS, ModeRequest, expand_modes, expand_style, status_line_needs, status_line_templates,
};

pub(crate) use parts::{completed_status_bytes, status_cache_callbacks};
use parts::{
    completed_status_bytes_with_capture_bytes, expand_status_parts, status_callback_values,
    status_parts_enabled,
};

use shell_jobs::{run_shell, shell_second, status_working_directory};

type ShellCacheKey = (ClientId, FormatJobTag, String);

#[derive(Default)]
struct ShellCacheEntry {
    expanded: Option<String>,
    output: Option<String>,
    output_needs: FormatNeeds,
    last: u64,
    job: Option<ShellJob>,
}

struct ShellJob {
    updates: crossbeam_channel::Receiver<crate::daemon::status_jobs::StatusUpdate>,
    client: crate::daemon::status_jobs::StatusClient,
    fd: i32,
    pid: u32,
    serial: u64,
}

fn format_second() -> i64 {
    i64::try_from(shell_second()).unwrap_or(i64::MAX)
}

pub(crate) const LIST_CLIENTS_CONTEXT_FORMATS: [&str; 1] = ["line"];
pub(crate) const COPY_MODE_CONTEXT_FORMATS: [&str; 19] = [
    "copy_cursor_line",
    "copy_cursor_word",
    "copy_cursor_x",
    "copy_cursor_y",
    "copy_line_numbers",
    "rectangle_toggle",
    "refresh_active",
    "scroll_position",
    "search_count",
    "search_count_partial",
    "search_match",
    "search_present",
    "search_timed_out",
    "selection_active",
    "selection_end_x",
    "selection_end_y",
    "selection_present",
    "selection_start_x",
    "selection_start_y",
];
pub(crate) const SHOW_MESSAGES_CONTEXT_FORMATS: [&str; 3] =
    ["message_number", "message_text", "message_time"];

pub(crate) type StatusJobNeeds = Arc<parking_lot::Mutex<BTreeMap<ClientId, FormatNeeds>>>;

#[derive(Default)]
pub(crate) struct StatusRenderer {
    shell_cache: BTreeMap<ShellCacheKey, ShellCacheEntry>,
    published: BTreeMap<ClientId, Arc<StatusLine>>,
    tmux_shim: Option<PathBuf>,
    zz_executable: Option<PathBuf>,
    job_waker: Option<std::task::Waker>,
    job_client: crate::daemon::status_jobs::StatusClient,
    pending_modes: BTreeSet<ClientId>,
    job_needs: StatusJobNeeds,
    uncovered_jobs: BTreeSet<ClientId>,
    completed: Option<CompletedStatus>,
    animating: BTreeSet<ClientId>,
    animation_started: bool,
    #[cfg(test)]
    expansions: usize,
    owned_clients: BTreeSet<ClientId>,
    owned_client_high_watermark: Option<ClientId>,
}

#[derive(Clone)]
pub(crate) struct StatusRequest {
    pub(crate) client: ClientId,
    pub(crate) formats: Arc<StatusFormats>,
    pub(crate) row_formats: Arc<BTreeMap<u32, String>>,
    pub(crate) option_snapshot: Arc<StatusRowVariables>,
    pub(crate) message_line: u8,
    pub(crate) customized: bool,
    pub(crate) title_format: Arc<Option<String>>,
    pub(crate) environment: Arc<Vec<(RawText, Option<RawText>)>>,
    pub(crate) default_terminal: Arc<String>,
    pub(crate) startup: bool,
    pub(crate) context: Arc<StatusContext<'static>>,
    pub(crate) facts: Arc<FormatHookFacts>,
    pub(crate) client_scheme: Option<TerminalColorScheme>,
    pub(crate) message_styles: Arc<(String, String)>,
    pub(crate) modes: Vec<ModeRequest>,
    pub(crate) pane_borders: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    pub(crate) references: Arc<BTreeSet<String>>,
}

struct CompletedStatus {
    client: ClientId,
    formats: Arc<StatusFormats>,
    row_formats: Arc<BTreeMap<u32, String>>,
    option_snapshot: Arc<StatusRowVariables>,
    references: Arc<BTreeSet<String>>,
    message_line: u8,
    customized: bool,
    title_format: Arc<Option<String>>,
    context: Arc<StatusContext<'static>>,
    client_scheme: Option<TerminalColorScheme>,
    message_styles: Arc<(String, String)>,
    pane_borders: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    callback_names: Vec<String>,
    callbacks: Vec<Option<String>>,
    now: i64,
    status: Arc<StatusLine>,
    fact_identity: Weak<FormatHookFacts>,
    request_identity: Weak<StatusRequest>,
    option_bytes: usize,
    parts: Option<Arc<StatusParts>>,
    startup: bool,
    environment_identity: Weak<Vec<(RawText, Option<RawText>)>>,
    default_terminal_identity: Weak<String>,
    context_bytes: usize,
    cycle_tick: Option<u64>,
}

const COMPLETED_STATUS_MAX_BYTES: usize = 1024 * 1024;

pub(crate) const STATUS_CYCLE_PERIOD: std::time::Duration = std::time::Duration::from_millis(100);

thread_local! {
    static CYCLE_ANIMATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn cycle_tick() -> u64 {
    cycle_clock_ms() / 100
}

pub(crate) fn expand_cycle_frames<T>(expand: impl FnOnce() -> T) -> (T, bool) {
    let before = CYCLE_ANIMATED.replace(false);
    let value = expand();
    let animated = CYCLE_ANIMATED.replace(before);
    (value, animated)
}

struct StatusParts {
    left: Vec<StatusPart>,
    right: Vec<StatusPart>,
    rows: BTreeMap<u32, Vec<StatusPart>>,
    theme: Option<OnceLock<zz_protocol::ThemeColours>>,
    message_styles: Option<OnceLock<(String, String)>>,
}

struct StatusPart {
    range: std::ops::Range<usize>,
    value: Option<OnceLock<String>>,
}

#[cfg(test)]
#[path = "daemon/format_status_tests.rs"]
mod format_status_tests;

pub(crate) struct DaemonFormatHooks<'a> {
    status_client: Option<ClientId>,
    facts: &'a dyn FormatFactSource,
    option_engine: Option<&'a MuxEngine>,
    status_context: Option<&'a StatusContext<'a>>,
    variables: Option<&'a BTreeMap<String, String>>,
    command_item: Option<&'a str>,
    option_snapshot: Option<&'a StatusRowVariables>,
    cache: Option<&'a mut BTreeMap<ShellCacheKey, ShellCacheEntry>>,
    touched: Option<&'a mut BTreeSet<ShellCacheKey>>,
    refresh: bool,
    now: i64,
    environment: Option<&'a [(RawText, Option<RawText>)]>,
    default_terminal: Option<&'a str>,
    startup: bool,
    tmux_shim: Option<&'a std::path::Path>,
    zz_executable: Option<&'a std::path::Path>,
    job_waker: Option<&'a std::task::Waker>,
    job_client: Option<&'a crate::daemon::status_jobs::StatusClient>,
    facts_withheld: bool,
    buffer_override: Option<BufferFormatFacts>,
    pane_in_mode_override: Option<(PaneId, usize)>,
    live_process_facts: bool,
    cycle: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::status_jobs::tests::renderer;
    use std::time::{Duration, Instant};
    use zz_mux::{PaneKind, SplitSize, StatusValues, expand_format_values};
    use zz_protocol::Axis;
    use zz_terminal::{SessionStatus, TerminalViewId};

    pub(super) fn settled(renderer: &mut StatusRenderer, request: &StatusRequest) -> StatusLine {
        renderer.render_initial(request);
        let deadline = Instant::now() + Duration::from_secs(5);
        while renderer
            .shell_cache
            .values()
            .any(|entry| entry.job.is_some())
        {
            renderer.poll_jobs();
            assert!(Instant::now() < deadline, "status jobs did not complete");
            thread::sleep(Duration::from_millis(5));
        }
        renderer.render_initial(request)
    }

    pub(super) fn counted_value_job(count: &Path) -> String {
        #[cfg(unix)]
        let command = format!("echo run >> '{}'; echo value", count.display());
        #[cfg(windows)]
        let command = format!("echo run>>\"{}\"& echo value", count.display());
        format!("#({command})")
    }

    pub(super) fn request(client: u64, left: &str, right: &str) -> StatusRequest {
        StatusRequest {
            client: ClientId(client),
            formats: Arc::new(StatusFormats {
                left: left.to_owned(),
                right: right.to_owned(),
                style: String::new(),
                left_style: String::new(),
                right_style: String::new(),
                left_length: u16::MAX,
                right_length: u16::MAX,
                ..StatusFormats::default()
            }),
            row_formats: Arc::new(BTreeMap::new()),
            option_snapshot: Arc::new(StatusRowVariables::default()),
            message_line: 0,
            customized: false,
            title_format: Arc::new(None),
            environment: Arc::new(Vec::new()),
            default_terminal: Arc::new("tmux-256color".to_owned()),
            startup: false,
            context: Arc::new(StatusContext::from(StatusValues {
                session_name: "work".to_owned(),
                ..StatusValues::default()
            })),
            facts: Arc::new(FormatHookFacts {
                client: Some(ClientFormatFacts::default()),
                ..FormatHookFacts::default()
            }),
            client_scheme: None,
            message_styles: Arc::new((String::new(), String::new())),
            modes: Vec::new(),
            pane_borders: Arc::default(),
            references: MuxEngine::default().cached_format_references_for_templates([left, right]),
        }
    }

    pub(super) fn engine_request(
        client: u64,
        engine: &MuxEngine,
        session: Option<SessionId>,
    ) -> StatusRequest {
        let snapshot = engine.state.snapshot();
        let formats = engine.status_formats_for_session(session);
        let row_formats = engine.status_format_array_for_session(session);
        let title_format = (session.is_some() && engine.set_titles_for_session(session))
            .then(|| engine.set_titles_string_for_session(session));
        let message_styles = engine.message_styles_for_session(session);
        let needs = status_line_needs(
            engine,
            &formats,
            &row_formats,
            title_format.as_deref(),
            &message_styles,
        );
        let references = engine.cached_format_references_for_templates(status_line_templates(
            &formats,
            &row_formats,
            title_format.as_deref(),
            &message_styles,
        ));
        let context = status_context(&snapshot, engine, session, None)
            .detach_with_references(needs, &references);
        StatusRequest {
            client: ClientId(client),
            formats: Arc::new(formats),
            row_formats: Arc::new(row_formats),
            option_snapshot: engine.cached_format_option_snapshot(),
            message_line: engine.message_line_for_session(session),
            customized: engine.status_customized_for_session(session),
            title_format: Arc::new(title_format),
            environment: Arc::new(engine.job_environment(None)),
            default_terminal: Arc::new(engine.default_terminal_for_spawn().to_owned()),
            startup: false,
            context: Arc::new(context),
            facts: Arc::new(FormatHookFacts {
                client: session.map(|_| ClientFormatFacts::default()),
                ..FormatHookFacts::default()
            }),
            client_scheme: None,
            message_styles: Arc::new(message_styles),
            modes: Vec::new(),
            pane_borders: Arc::default(),
            references,
        }
    }

    pub(super) fn execute(
        engine: &mut MuxEngine,
        context: &mut zz_mux::ExecutionContext,
        args: &[&str],
    ) {
        engine
            .execute(
                context,
                &zz_protocol::CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{args:?}: {error:?}"));
    }

    #[test]
    fn browser_url_status_uses_active_tab_and_is_empty_for_terminal() {
        let mut engine = MuxEngine::default();
        let (_, _, terminal) = engine.state.create_session("urls").unwrap();
        let browser = engine
            .state
            .split_pane(
                terminal,
                Axis::Horizontal,
                PaneKind::Browser(zz_protocol::BrowserDescriptor {
                    tabs: vec![
                        "https://example.com/first".to_owned(),
                        "https://example.com/active".to_owned(),
                    ],
                    active_tab: 1,
                    profile: "default".to_owned(),
                }),
            )
            .unwrap();
        let mut request = request(1, "url=#{browser_url}", "");
        Arc::make_mut(&mut request.facts).mux = Arc::new(engine.format_facts());
        let mut renderer = StatusRenderer::default();
        Arc::make_mut(&mut request.context).pane_id = browser.to_string();
        assert_eq!(
            renderer.render_initial(&request).left,
            "url=https://example.com/active"
        );
        Arc::make_mut(&mut request.context).pane_id = terminal.to_string();
        assert_eq!(renderer.render_initial(&request).left, "url=");
    }

    #[test]
    fn last_command_status_format_reports_codes_and_claims_unknown_values() {
        let pane = PaneId(9);
        let terminal = Arc::new(TerminalSession::spawn_empty_with_appearance(
            64,
            Arc::new(zz_terminal::TerminalAppearance::default()),
        ));
        let facts = FormatHookFacts {
            terminals: Arc::new(BTreeMap::from([(pane, Arc::clone(&terminal))])),
            ..FormatHookFacts::default()
        };
        let mut hooks = DaemonFormatHooks::command(&facts);
        let context = StatusContext::from(StatusValues {
            pane_id: pane.to_string(),
            ..StatusValues::default()
        });
        for (bytes, expected) in [
            (b"".as_slice(), ""),
            (b"\x1b]133;D;7\x07".as_slice(), "7"),
            (b"\x1b]133;A\x07".as_slice(), "7"),
            (b"\x1b]133;D;0\x07".as_slice(), "0"),
            (b"\x1b]133;D\x07".as_slice(), ""),
        ] {
            assert!(terminal.feed(Arc::from(bytes)));
            terminal
                .capture(zz_terminal::CaptureOptions::default())
                .expect("processed output");
            assert_eq!(
                hooks.variable("pane_last_command_status", &context),
                Some(expected.to_owned())
            );
            assert_eq!(
                zz_mux::expand_format_values("#{pane_last_command_status}", &context, &mut hooks),
                expected
            );
        }
        for pane_id in ["%99", ""] {
            let context = StatusContext::from(StatusValues {
                pane_id: pane_id.to_owned(),
                ..StatusValues::default()
            });
            assert_eq!(
                hooks.variable("pane_last_command_status", &context),
                Some(String::new())
            );
        }
    }

    #[test]
    fn daemon_delegated_format_consumers_match_mux_inventory() {
        let delegated = zz_mux::delegated_format_variable_names().collect::<Vec<_>>();
        assert_eq!(delegated.len(), 66);

        let session = SessionId(1);
        let pane = PaneId(1);
        let facts = FormatHookFacts {
            session_last_attached: Arc::new(BTreeMap::from([(session, 1)])),
            pane_pipes: Arc::new(BTreeMap::from([(pane, 4242)])),
            buffer: Some(BufferFormatFacts {
                name: String::new(),
                data: Arc::from([]),
                created: UNIX_EPOCH,
            }),
            client: Some(ClientFormatFacts {
                last_session: "last".to_owned(),
                theme: "dark".to_owned(),
                viewport: Some(ClientViewportFacts {
                    columns: 80,
                    rows: 23,
                    window_width: 200,
                    window_height: 60,
                    cursor: None,
                }),
                ..ClientFormatFacts::default()
            }),
            copy_modes: Arc::new(BTreeMap::from([(
                pane,
                vec![(String::new(), Arc::new(CopyModeFacts::default()))],
            )])),
            ..FormatHookFacts::default()
        };
        let context = StatusContext::from(StatusValues {
            session_id: session.to_string(),
            window_id: "@1".to_owned(),
            pane_id: pane.to_string(),
            ..StatusValues::default()
        });
        let mut hooks = DaemonFormatHooks::command(&facts);
        for name in delegated {
            assert!(
                hooks.variable(name, &context).is_some() || PANE_OUTPUT_FORMATS.contains(&name),
                "daemon format hook does not consume {name}"
            );
        }
        let recorded = TerminalFacts {
            output: zz_terminal::PaneOutputFacts {
                output_generation: 3,
                last_output_time: 100,
                last_prompt_time: 90,
                command_start_time: 95,
                command_end_time: 99,
                command_running: false,
                command_status: Some(2),
            },
            private_modes: 0b1_0100,
            ..TerminalFacts::default()
        };
        for name in PANE_OUTPUT_FORMATS {
            assert!(
                pane_output_variable(name, &recorded, 200).is_some(),
                "{name}"
            );
        }
        assert_eq!(
            pane_output_variable("pane_private_modes", &recorded, 200).as_deref(),
            Some("7,25")
        );
        assert_eq!(
            pane_output_variable("pane_command_duration", &recorded, 200).as_deref(),
            Some("4")
        );
        let running = TerminalFacts {
            output: zz_terminal::PaneOutputFacts {
                command_running: true,
                command_end_time: 0,
                command_status: None,
                ..recorded.output
            },
            ..recorded
        };
        assert_eq!(
            pane_output_variable("pane_command_duration", &running, 200).as_deref(),
            Some("105")
        );
        assert_eq!(
            pane_output_variable("pane_command_status", &running, 200),
            None
        );
        assert_eq!(
            pane_output_variable("pane_last_prompt_time", &TerminalFacts::default(), 200),
            None
        );
    }

    #[test]
    fn copy_mode_toggle_and_selection_formats_answer_from_the_copy_view() {
        let pane = PaneId(1);
        let context = StatusContext::from(StatusValues {
            pane_id: pane.to_string(),
            ..StatusValues::default()
        });
        let answer = |facts: &FormatHookFacts| {
            zz_mux::expand_format_values(
                "#{selection_active}:#{rectangle_toggle}",
                &context,
                &mut DaemonFormatHooks::command(facts),
            )
        };
        let in_mode = FormatHookFacts {
            copy_modes: Arc::new(BTreeMap::from([(
                pane,
                vec![(
                    String::new(),
                    Arc::new(CopyModeFacts {
                        rectangle_toggle: true,
                        selection_active: true,
                        ..CopyModeFacts::default()
                    }),
                )],
            )])),
            ..FormatHookFacts::default()
        };
        assert_eq!(answer(&in_mode), "1:1");
        let entered = FormatHookFacts {
            copy_modes: Arc::new(BTreeMap::from([(
                pane,
                vec![(String::new(), Arc::new(CopyModeFacts::default()))],
            )])),
            ..FormatHookFacts::default()
        };
        assert_eq!(answer(&entered), "0:0");
        assert_eq!(answer(&FormatHookFacts::default()), ":");
    }

    #[test]
    fn status_context_uses_the_attached_format_client() {
        let mut engine = MuxEngine::default();
        let mut execution = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut execution,
            &["new-session", "-s", "status-active"],
        );
        let session = execution.session.expect("session id");
        let snapshot = engine.state.snapshot();

        let attached = status_context(&snapshot, &engine, Some(session), None);
        assert_eq!(attached.session_active, Some(true));

        let detached = status_context(&snapshot, &engine, None, None);
        assert_eq!(detached.session_active, None);
    }

    #[test]
    fn status_option_snapshot_retargets_every_loop_after_the_engine_is_released() {
        let mut engine = MuxEngine::default();
        let (attached, first_window, first_pane) = engine.state.create_session("attached").unwrap();
        let second_pane = engine
            .state
            .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
            .unwrap();
        let (second_window, _) = engine
            .state
            .create_window_at(
                attached,
                None,
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .unwrap();
        let (other, _, _) = engine.state.create_session("other").unwrap();
        let mut context =
            zz_mux::ExecutionContext::new(Some(attached), Some(first_window), Some(first_pane));
        let second_window = second_window.to_string();
        let first_pane = first_pane.to_string();

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "mouse", "off"],
        );
        execute(&mut engine, &mut context, &["set-option", "mouse", "on"]);
        execute(
            &mut engine,
            &mut context,
            &["set-window-option", "-g", "automatic-rename", "off"],
        );
        execute(
            &mut engine,
            &mut context,
            &[
                "set-window-option",
                "-t",
                &second_window,
                "automatic-rename",
                "on",
            ],
        );
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-gp", "allow-set-title", "on"],
        );
        execute(
            &mut engine,
            &mut context,
            &[
                "set-option",
                "-p",
                "-t",
                &first_pane,
                "allow-set-title",
                "off",
            ],
        );

        execute(
            &mut engine,
            &mut context,
            &[
                "set-option",
                "status-left",
                "OPTCHAIN:#{mouse}:#{S:#{mouse}}:#{W:#{automatic-rename}}:#{P:#{allow-set-title}}",
            ],
        );
        execute(
            &mut engine,
            &mut context,
            &["set-option", "status-left-length", "100"],
        );
        let request = engine_request(1, &engine, Some(attached));
        assert!(
            request
                .option_snapshot
                .sessions
                .contains_key(&other.to_string())
        );
        assert!(request.option_snapshot.windows.contains_key(&second_window));
        assert!(
            request
                .option_snapshot
                .panes
                .contains_key(&second_pane.to_string())
        );
        drop(engine);

        let status = StatusRenderer::default().render_initial(&request);
        assert!(
            status.left.contains("OPTCHAIN:1:10:01:01"),
            "{}",
            status.left
        );
    }

    #[test]
    fn the_status_line_publishes_the_theme_the_way_the_pin_resolves_it_per_client() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "alpha", "-n", "main"],
        );
        let session = context.session;
        let mut renderer = StatusRenderer::default();

        let stock = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(
            stock.theme.slot(0),
            Some(TmuxColour::Basic(0)),
            "themeblack"
        );
        assert_eq!(
            stock.theme.slot(4),
            Some(TmuxColour::Basic(2)),
            "themegreen"
        );
        assert!(!stock.theme.is_circular());

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "dark-theme-green", "colour124"],
        );
        let indexed = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(indexed.theme.slot(4), Some(TmuxColour::Indexed(124)));

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "light-theme-green", "colour99"],
        );
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "theme", "light"],
        );
        let light = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(light.theme.slot(4), Some(TmuxColour::Indexed(99)));

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "theme", "terminal"],
        );
        let terminal = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(
            terminal.theme,
            zz_protocol::ThemeColours([
                TmuxColour::Basic(0),
                TmuxColour::Basic(7),
                TmuxColour::Basic(7),
                TmuxColour::Basic(0),
                TmuxColour::Basic(2),
                TmuxColour::Basic(3),
                TmuxColour::Basic(1),
                TmuxColour::Basic(4),
                TmuxColour::Basic(6),
                TmuxColour::Basic(5),
            ])
        );

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "theme", "dark"],
        );
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-s", "dark-theme-red", "themered"],
        );
        let circular = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(circular.theme.slot(6), Some(TmuxColour::Basic(8)));
        assert!(!circular.theme.is_circular());
        assert_eq!(circular.validate(), Ok(()));
    }

    #[test]
    fn set_titles_expands_per_client_and_survives_status_off() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "alpha", "-n", "main"],
        );
        let session = context.session;
        let mut renderer = StatusRenderer::default();

        let baseline = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(baseline.title, "");

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "set-titles", "on"],
        );
        let titled = renderer.render_initial(&engine_request(1, &engine, session));
        assert!(
            titled.title.starts_with("alpha:0:main"),
            "default set-titles-string expands in client context: {}",
            titled.title
        );

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "set-titles-string", "#S custom"],
        );
        let custom = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(custom.title, "alpha custom");

        execute(&mut engine, &mut context, &["set-option", "status", "off"]);
        let off = renderer.render_initial(&engine_request(1, &engine, session));
        assert!(off.rows.is_empty());
        assert_eq!(off.title, "alpha custom");

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "set-titles", "off"],
        );
        let untitled = renderer.render_initial(&engine_request(1, &engine, session));
        assert_eq!(untitled.title, "");
    }

    #[test]
    fn personalized_rows_expand_per_session_and_dedupe() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "alpha", "-n", "main"],
        );
        let alpha = context.session.expect("alpha session");
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-d", "-s", "beta", "-n", "logs"],
        );
        let beta = engine
            .state
            .sessions
            .iter()
            .find(|(_, session)| session.name == "beta")
            .map(|(id, _)| *id)
            .expect("beta session");
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "status-right", "static"],
        );

        let requests = [
            engine_request(1, &engine, Some(alpha)),
            engine_request(2, &engine, Some(beta)),
        ];
        let mut renderer = StatusRenderer::default();
        let first = renderer.render_changed(&requests);
        assert_eq!(first.len(), 2);
        let alpha_status = &first[0].1;
        let beta_status = &first[1].1;
        assert_eq!(alpha_status.rows.len(), 1);
        assert!(
            alpha_status.rows[0].contains("[alpha]"),
            "row 0 carries status-left: {}",
            alpha_status.rows[0]
        );
        assert!(
            alpha_status.rows[0].contains("0:main"),
            "row 0 carries the window list: {}",
            alpha_status.rows[0]
        );
        assert!(
            alpha_status.rows[0].contains("static"),
            "row 0 carries status-right: {}",
            alpha_status.rows[0]
        );
        assert!(beta_status.rows[0].contains("[beta]"));
        assert!(beta_status.rows[0].contains("0:logs"));
        assert_ne!(alpha_status.rows, beta_status.rows);
        assert_eq!(alpha_status.validate(), Ok(()));
        assert_eq!(beta_status.validate(), Ok(()));
        assert!(renderer.render_changed(&requests).is_empty());
    }

    #[test]
    fn status_row_counts_follow_the_status_option() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "work", "-n", "main"],
        );
        let session = context.session.expect("session id");
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "status", "off"],
        );
        let off =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        assert!(off.rows.is_empty());
        assert_eq!(off.message_line, 0);
        assert!(off.customized);
        assert!(off.is_empty());

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "status", "5"],
        );
        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "message-line", "4"],
        );
        let five =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        assert_eq!(five.rows.len(), 5);
        assert!(five.rows[0].contains("[work]"));
        assert!(five.rows[1].starts_with("#[align=left]    P: "));
        assert!(five.rows[2].starts_with("#[align=left]    S: "));
        assert!(!five.rows[1].contains("#{R:"));
        assert!(!five.rows[2].contains("#{R:"));
        assert_eq!(five.rows[3], "");
        assert_eq!(five.rows[4], "");
        assert_eq!(five.message_line, 4);
        assert_eq!(five.validate(), Ok(()));

        execute(
            &mut engine,
            &mut context,
            &["set-option", "-g", "status", "2"],
        );
        let two =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        assert_eq!(two.rows.len(), 2);
        assert_eq!(
            two.message_line, 1,
            "message-line clamps below the row count"
        );
        assert_eq!(two.validate(), Ok(()));
    }

    #[test]
    fn per_window_status_overrides_style_the_rendered_row() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "work", "-n", "main"],
        );
        execute(
            &mut engine,
            &mut context,
            &["new-window", "-d", "-n", "other"],
        );
        let session = context.session.expect("session id");
        execute(
            &mut engine,
            &mut context,
            &[
                "set-window-option",
                "-t",
                "work:0",
                "window-status-current-format",
                "OVERRIDE",
            ],
        );
        let status =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        let row = status.rows.first().expect("status row");
        assert!(row.contains("OVERRIDE"), "row: {row}");
        assert!(
            !row.contains("0:main"),
            "the current window's default label must be replaced: {row}"
        );
        assert!(
            row.contains("1:other"),
            "windows without overrides keep the global format: {row}"
        );
    }

    #[test]
    fn window_status_separator_uses_each_item_scope_and_skips_the_trailing_item() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut context,
            &["new-session", "-s", "work", "-n", "main"],
        );
        execute(
            &mut engine,
            &mut context,
            &["new-window", "-d", "-n", "logs"],
        );
        let session = context.session.expect("session id");
        for args in [
            &["set-option", "-g", "status-left", ""] as &[&str],
            &["set-option", "-g", "status-right", ""],
            &[
                "set-option",
                "-g",
                "status-format[0]",
                "#{W:#{T:window-status-format}#{?loop_last_flag,,#{E:window-status-separator}},#{T:window-status-current-format}#{?loop_last_flag,,#{E:window-status-separator}}}",
            ],
            &[
                "set-window-option",
                "-g",
                "window-status-format",
                "N#{window_index}:#{window_name}",
            ],
            &[
                "set-window-option",
                "-g",
                "window-status-current-format",
                "C#{window_index}:#{window_name}",
            ],
            &[
                "set-window-option",
                "-g",
                "window-status-separator",
                "#{?#{==:#{window_index},0},#[fg=red]G#{window_index}:#{window_name},#[fg=blue]BAD}",
            ],
        ] {
            execute(&mut engine, &mut context, args);
        }

        let global =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        assert_eq!(global.left, "");
        assert_eq!(global.right, "");
        assert_eq!(global.rows, ["C0:main#[fg=red]G0:mainN1:logs"]);

        execute(
            &mut engine,
            &mut context,
            &[
                "set-window-option",
                "-t",
                "work:0",
                "window-status-separator",
                "#{?#{==:#{window_index},0},#[bold]L#{window_index}:#{window_name},#[reverse]BAD}",
            ],
        );
        let local =
            StatusRenderer::default().render_initial(&engine_request(2, &engine, Some(session)));
        assert_eq!(local.rows, ["C0:main#[bold]L0:mainN1:logs"]);
    }

    #[test]
    fn sparse_session_status_formats_keep_blank_rows_without_compaction() {
        let mut engine = MuxEngine::default();
        let mut context = zz_mux::ExecutionContext::default();
        execute(&mut engine, &mut context, &["new-session", "-s", "work"]);
        let session = context.session.expect("session id");
        execute(&mut engine, &mut context, &["set-option", "status", "5"]);
        execute(
            &mut engine,
            &mut context,
            &["set-option", "status-format[3]", "MARK #S"],
        );
        let status =
            StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
        assert_eq!(status.rows, ["", "", "", "MARK work", ""]);
        assert!(status.customized);
        assert_eq!(status.validate(), Ok(()));
    }

    #[test]
    fn base_style_applies_status_style_then_fg_bg_overrides() {
        let mut renderer = StatusRenderer::default();
        let mut styled = request(1, "", "");
        Arc::make_mut(&mut styled.formats).style = "bg=blue,fg=white".to_owned();
        Arc::make_mut(&mut styled.formats).foreground = "red".to_owned();
        let status = renderer.render_initial(&styled);
        assert_eq!(status.base_style, "bg=blue,fg=white,fg=red");
        assert_eq!(status.validate(), Ok(()));

        let mut dynamic = request(2, "", "");
        Arc::make_mut(&mut dynamic.formats).style = "fg=#{?window_zoomed,red,green}".to_owned();
        Arc::make_mut(&mut dynamic.formats).background = "black".to_owned();
        let status = renderer.render_initial(&dynamic);
        assert_eq!(status.base_style, "fg=green,bg=black");
        assert_eq!(status.validate(), Ok(()));
    }

    #[test]
    fn an_unparseable_expanded_status_style_degrades_instead_of_dropping_the_event() {
        let mut renderer = StatusRenderer::default();
        let mut broken = request(1, "LEFT", "RIGHT");
        Arc::make_mut(&mut broken.formats).style = "bg=#{@theme_bg}".to_owned();
        let status = renderer.render_initial(&broken);
        assert_eq!(status.base_style, "");
        assert_eq!(status.rows.len(), 1);
        assert!(status.left.ends_with("LEFT"));
        assert!(status.right.ends_with("RIGHT"));
        assert_eq!(
            status.validate(),
            Ok(()),
            "the event must survive the encode seam"
        );

        let mut overridden = request(2, "", "");
        Arc::make_mut(&mut overridden.formats).style = "bg=#{@theme_bg}".to_owned();
        Arc::make_mut(&mut overridden.formats).foreground = "red".to_owned();
        let status = renderer.render_initial(&overridden);
        assert_eq!(status.base_style, "fg=red");
        assert_eq!(status.validate(), Ok(()));
    }

    #[test]
    fn message_line_clamps_against_the_published_row_count() {
        let mut renderer = StatusRenderer::default();
        let mut clamped = request(1, "", "");
        Arc::make_mut(&mut clamped.formats).lines = 2;
        clamped.message_line = 4;
        let status = renderer.render_initial(&clamped);
        assert_eq!(status.rows.len(), 2);
        assert_eq!(status.message_line, 1);

        let mut disabled = request(2, "", "");
        Arc::make_mut(&mut disabled.formats).enabled = false;
        disabled.message_line = 3;
        assert_eq!(renderer.render_initial(&disabled).message_line, 0);
    }

    #[test]
    fn only_changed_clients_are_published() {
        let mut renderer = StatusRenderer::default();
        let requests = [request(1, "[#S]", ""), request(2, "[#S]", "")];

        let first = renderer.render_changed(&requests);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].1.left, "[work]");
        assert!(renderer.render_changed(&requests).is_empty());

        let renamed = [request(1, "[#S]", ""), {
            let mut request = request(2, "[#S]", "");
            Arc::make_mut(&mut request.context).session_name = "infra".to_owned();
            request
        }];
        let second = renderer.render_changed(&renamed);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, ClientId(2));
        assert_eq!(second[0].1.left, "[infra]");
    }

    #[test]
    fn status_sides_left_trim_display_width_without_counting_styles() {
        assert_eq!(trim_status_left("a#[bold]界bc", 4), "a#[bold]界b");
        assert_eq!(trim_status_left("##[bold]界", 3), "##[b");
    }

    #[test]
    fn status_sides_carry_base_then_side_styles() {
        let mut request = request(1, "abcdef", "uvwxyz");
        Arc::make_mut(&mut request.formats).style = "bg=blue,fg=white".to_owned();
        Arc::make_mut(&mut request.formats).foreground = "red".to_owned();
        Arc::make_mut(&mut request.formats).left_style = "bold".to_owned();
        Arc::make_mut(&mut request.formats).right_style = "italics".to_owned();
        Arc::make_mut(&mut request.formats).left_length = 4;
        Arc::make_mut(&mut request.formats).right_length = 3;
        let status = StatusRenderer::default().render_initial(&request);
        assert_eq!(
            status.left,
            "#[bg=blue,fg=white,fg=red]#[bold]#[push-default]abcd"
        );
        assert_eq!(
            status.right,
            "#[bg=blue,fg=white,fg=red]#[italics]#[push-default]uvw"
        );
    }

    #[test]
    fn status_style_wrapping_stays_inside_the_wire_limit() {
        let mut request = request(1, &"x".repeat(MAX_STATUS_TEXT_BYTES), "");
        Arc::make_mut(&mut request.formats).style = "bold,".repeat(800);
        Arc::make_mut(&mut request.formats).left_style = "italics,".repeat(800);
        let status = StatusRenderer::default().render_initial(&request);
        assert_eq!(status.validate(), Ok(()));
        assert!(status.left.is_char_boundary(status.left.len()));
    }

    #[test]
    fn oversized_row_title_and_base_style_stay_inside_the_wire_limit() {
        let overflow = "#{R:x,9000}";
        let mut request = request(1, overflow, overflow);
        Arc::make_mut(&mut request.formats).lines = 2;
        Arc::make_mut(&mut request.formats).style = format!("bold,{}", "italics,".repeat(900));
        *Arc::make_mut(&mut request.title_format) = Some(overflow.to_owned());
        *Arc::make_mut(&mut request.row_formats) =
            BTreeMap::from([(0, overflow.to_owned()), (1, overflow.to_owned())]);
        let status = StatusRenderer::default().render_initial(&request);
        assert_eq!(status.validate(), Ok(()));
        assert_eq!(status.title.len(), MAX_STATUS_TEXT_BYTES);
        assert_eq!(status.rows.len(), 2);
        for row in &status.rows {
            assert_eq!(row.len(), MAX_STATUS_TEXT_BYTES);
        }

        let unstyled = self::request(1, overflow, "");
        let status = StatusRenderer::default().render_initial(&unstyled);
        assert_eq!(status.validate(), Ok(()));
        assert!(status.left.len() <= MAX_STATUS_TEXT_BYTES);

        let mut markers = self::request(1, "#{R:#[bold],700}x", "");
        Arc::make_mut(&mut markers.formats).left_length = 10;
        let status = StatusRenderer::default().render_initial(&markers);
        assert_eq!(status.validate(), Ok(()));
        assert!(status.left.len() <= MAX_STATUS_TEXT_BYTES);
    }

    #[test]
    fn a_disabled_status_renders_empty() {
        let mut renderer = StatusRenderer::default();
        let mut request = request(1, "[#S]", "%H");
        Arc::make_mut(&mut request.formats).enabled = false;
        let status = renderer.render_initial(&request);
        assert!(status.is_empty());
    }

    #[test]
    fn status_pane_index_uses_effective_pane_base_and_pane_order() {
        let mut engine = MuxEngine::default();
        let (session, window, target) = engine.state.create_session("work").unwrap();
        let source = engine
            .state
            .split_pane(target, Axis::Horizontal, PaneKind::Terminal)
            .unwrap();
        engine
            .state
            .join_pane(
                source,
                target,
                Axis::Vertical,
                SplitSize::Default,
                true,
                false,
                false,
            )
            .unwrap();
        engine
            .execute(
                &mut zz_mux::ExecutionContext::default(),
                &zz_protocol::CommandInvocation::new(
                    "set-window-option",
                    ["-g", "pane-base-index", "1"],
                ),
            )
            .unwrap();
        let snapshot = engine.state.snapshot();
        let mut layout_order = Vec::new();
        snapshot.sessions[0].windows[0]
            .layout
            .panes(&mut layout_order);

        assert_eq!(layout_order, [source, target]);
        assert_eq!(engine.state.windows[&window].pane_order(), [target, source]);
        let context = status_context(&snapshot, &engine, Some(session), Some(window));
        assert_eq!(context.pane_index, 2);
        assert_eq!(
            context.window_layout,
            engine.state.windows[&window].layout_string(
                zz_mux::LayoutFormat::V2,
                engine.state.pane_base_index(window)
            )
        );
    }

    #[test]
    fn content_search_reads_visible_terminal_rows() {
        let pane = PaneId(9);
        let terminal = Arc::new(TerminalSession::spawn_output_view(
            "search".to_owned(),
            "alpha\nbravo\ncharlie".to_owned(),
        ));
        terminal.attach_view(TerminalViewId(1));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !matches!(terminal.latest_viewport().status, SessionStatus::Running) {
            assert!(
                Instant::now() < deadline,
                "output view did not become ready"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let facts = FormatHookFacts {
            terminals: Arc::new(BTreeMap::from([(pane, terminal)])),
            buffer: None,
            ..FormatHookFacts::default()
        };
        let context = StatusContext::from(StatusValues {
            pane_id: pane.to_string(),
            ..StatusValues::default()
        });
        let mut hooks = DaemonFormatHooks::command(&facts);
        assert_eq!(
            expand_format_values(
                "#{C:bravo}|#{C/i:BRAVO}|#{C/r:^charlie$}|#{C:missing}",
                &context,
                &mut hooks,
            ),
            "2|2|3|0"
        );
    }

    #[test]
    fn window_cell_metrics_default_to_the_pin_size_and_follow_a_reporting_client() {
        let format = "#{window_cell_width}|#{window_cell_height}";
        let window = StatusContext::from(StatusValues {
            window_id: "@1".to_owned(),
            ..StatusValues::default()
        });

        let bare = FormatHookFacts::default();
        let mut hooks = DaemonFormatHooks::command(&bare);
        assert_eq!(expand_format_values(format, &window, &mut hooks), "16|32");

        let unsized_client = FormatHookFacts {
            client: Some(ClientFormatFacts {
                cell_width: "0".to_owned(),
                cell_height: String::new(),
                ..ClientFormatFacts::default()
            }),
            ..FormatHookFacts::default()
        };
        let mut hooks = DaemonFormatHooks::command(&unsized_client);
        assert_eq!(expand_format_values(format, &window, &mut hooks), "16|32");

        let reporting_client = FormatHookFacts {
            client: Some(ClientFormatFacts {
                cell_width: "9".to_owned(),
                cell_height: "18".to_owned(),
                ..ClientFormatFacts::default()
            }),
            ..FormatHookFacts::default()
        };
        let mut hooks = DaemonFormatHooks::command(&reporting_client);
        assert_eq!(expand_format_values(format, &window, &mut hooks), "9|18");

        let mut hooks = DaemonFormatHooks::command(&reporting_client);
        assert_eq!(
            expand_format_values(format, &StatusContext::default(), &mut hooks),
            "|"
        );
    }

    #[test]
    fn buffer_samples_preserve_utf8_and_escape_control_and_invalid_bytes() {
        assert_eq!(buffer_sample("α\n\t\\".as_bytes()), "α\\n\\t\\\\");
        assert_eq!(buffer_sample(&[0xff]), "\\377");
        assert_eq!(buffer_sample(&[0, b'x', 0, b'7']), "\\0x\\0007");
        assert_eq!(buffer_full(b"a\0b"), "a\0b");
        assert_eq!(
            buffer_sample(&vec![b'x'; 201]),
            format!("{}...", "x".repeat(200))
        );
    }

    #[test]
    fn commands_run_once_and_then_come_from_the_cache() {
        let mut renderer = renderer();
        let directory = tempfile::Builder::new()
            .prefix("zz-status-cache-")
            .tempdir_in(".")
            .expect("the cache fixture is created");
        let source = directory.path().join("value");
        std::fs::write(&source, "first\n").expect("the first value is written");
        #[cfg(unix)]
        let command = format!("cat '{}'", source.display());
        #[cfg(windows)]
        let command = format!("type {}", source.display());
        let format = format!("#({command})");
        let requests = [request(1, &format, "")];

        let first = renderer.render_changed(&requests);
        assert_eq!(
            first[0].1.left, "",
            "a first render starts the command without waiting"
        );
        assert_eq!(settled(&mut renderer, &requests[0]).left, "first");
        std::fs::write(&source, "second\n").expect("the second value is written");
        assert_eq!(renderer.render_forced(&requests[0]).left, "first");
        assert_eq!(settled(&mut renderer, &requests[0]).left, "second");
    }

    #[cfg(unix)]
    #[test]
    fn attached_status_jobs_start_in_each_clients_working_directory() {
        let directory = tempfile::tempdir().expect("working directory fixture");
        let first_cwd = directory.path().join("first");
        let second_cwd = directory.path().join("second");
        std::fs::create_dir(&first_cwd).expect("first cwd");
        std::fs::create_dir(&second_cwd).expect("second cwd");
        let first_cwd = std::fs::canonicalize(first_cwd).expect("first cwd resolves");
        let second_cwd = std::fs::canonicalize(second_cwd).expect("second cwd resolves");
        let mut first = request(1, "#(pwd -P)", "");
        Arc::make_mut(&mut first.context).session_path = first_cwd.to_string_lossy().into_owned();
        Arc::make_mut(&mut first.facts).client = Some(ClientFormatFacts::default());
        let mut second = request(2, "#(pwd -P)", "");
        Arc::make_mut(&mut second.context).session_path = second_cwd.to_string_lossy().into_owned();
        Arc::make_mut(&mut second.facts).client = Some(ClientFormatFacts::default());

        let mut renderer = renderer();
        let statuses = [
            (first.client, settled(&mut renderer, &first)),
            (second.client, settled(&mut renderer, &second)),
        ];

        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].1.left, first_cwd.to_string_lossy());
        assert_eq!(statuses[1].1.left, second_cwd.to_string_lossy());
    }

    #[cfg(unix)]
    #[test]
    fn a_client_with_no_attached_session_runs_no_status_job() {
        let directory = tempfile::tempdir().expect("working directory fixture");
        let source = directory.path().join("value");
        std::fs::write(&source, "first\n").expect("the first value is written");
        let format = format!("#(cat '{}')", source.display());
        let cwd = std::fs::canonicalize(directory.path()).expect("working directory resolves");
        let mut attached = request(1, &format, "");
        Arc::make_mut(&mut attached.context).session_path = cwd.to_string_lossy().into_owned();
        let mut clientless = request(2, &format, "");
        Arc::make_mut(&mut clientless.context).session_path = cwd.to_string_lossy().into_owned();
        Arc::make_mut(&mut clientless.facts).client = None;
        let mut renderer = renderer();

        assert_eq!(settled(&mut renderer, &attached).left, "first");
        assert_eq!(renderer.shell_cache.len(), 1);
        assert_eq!(settled(&mut renderer, &clientless).left, "");
        assert_eq!(renderer.shell_cache.len(), 1);
        assert_eq!(
            renderer.shell_cache.keys().next().unwrap().0,
            ClientId(1),
            "a clientless render adds no second job for the same command"
        );
    }

    #[cfg(unix)]
    #[test]
    fn status_command_cache_keeps_attached_clients_independent() {
        let directory = tempfile::tempdir().expect("working directory fixture");
        let source = directory.path().join("value");
        std::fs::write(&source, "first\n").expect("the first value is written");
        let format = format!("#(cat '{}')", source.display());
        let cwd = std::fs::canonicalize(directory.path()).expect("working directory resolves");
        let mut first = request(1, &format, "");
        Arc::make_mut(&mut first.context).session_path = cwd.to_string_lossy().into_owned();
        Arc::make_mut(&mut first.facts).client = Some(ClientFormatFacts::default());
        let mut renderer = renderer();

        assert_eq!(settled(&mut renderer, &first).left, "first");
        std::fs::write(&source, "second\n").expect("the second value is written");
        let mut second = request(2, &format, "");
        Arc::make_mut(&mut second.context).session_path = cwd.to_string_lossy().into_owned();
        Arc::make_mut(&mut second.facts).client = Some(ClientFormatFacts::default());

        assert_eq!(settled(&mut renderer, &second).left, "second");
        assert_eq!(renderer.shell_cache.len(), 2);
        renderer.forget(ClientId(1));
        assert_eq!(renderer.shell_cache.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn status_commands_receive_tmux_and_working_directory_without_tmux_pane() {
        let directory = tempfile::Builder::new()
            .prefix("zz-status-environment-")
            .tempdir_in(".")
            .expect("the working directory is created");
        let pane_directory = tempfile::Builder::new()
            .prefix("zz-status-pane-decoy-")
            .tempdir_in(".")
            .expect("the pane working directory is created");
        let socket = "/tmp/zz-status-environment.sock";
        let mut status_request = request(
            1,
            "#(echo \"$TMUX|$ZZ_SOCKET|$PWD|${TMUX_PANE-unset}\")",
            "",
        );
        Arc::make_mut(&mut status_request.context).socket_path = socket.to_owned();
        Arc::make_mut(&mut status_request.context).session_path = directory
            .path()
            .canonicalize()
            .expect("the working directory resolves")
            .to_string_lossy()
            .into_owned();
        Arc::make_mut(&mut status_request.context).pane_current_path = pane_directory
            .path()
            .canonicalize()
            .expect("the pane working directory resolves")
            .to_string_lossy()
            .into_owned();

        let status = settled(&mut renderer(), &status_request);

        assert_eq!(
            status.left,
            format!(
                "{socket},{},-1|{socket}|{}|unset",
                std::process::id(),
                status_request.context.session_path
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn status_commands_use_global_only_clean_environment_and_the_startup_gate() {
        let mut engine = MuxEngine::default();
        engine.seed_global_environment([
            ("STATUS_SCOPE", "global"),
            ("TMUX_PANE", "status-global-pane"),
            ("TERM", "startup-term"),
            ("TERM_PROGRAM", "startup-program"),
            ("TERM_PROGRAM_VERSION", "startup-version"),
            ("COLORTERM", "startup-colorterm"),
            (
                zz_daemon_client::STARTUP_REENTRY_ENVIRONMENT_VARIABLE,
                "stale-reentry",
            ),
            (
                crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE,
                "stale-executable",
            ),
        ]);
        let mut execution = zz_mux::ExecutionContext::default();
        execute(
            &mut engine,
            &mut execution,
            &["new-session", "-d", "-s", "status-environment"],
        );
        let session = execution.session.expect("status session");
        for args in [
            vec!["set-environment", "-g", "-h", "STATUS_HIDDEN", "hidden"],
            vec!["set-environment", "-g", "-r", "STATUS_UNSET"],
            vec![
                "set-environment",
                "-t",
                "status-environment",
                "STATUS_SCOPE",
                "session",
            ],
            vec![
                "set-environment",
                "-t",
                "status-environment",
                "TMUX_PANE",
                "session-pane",
            ],
            vec!["set-option", "-g", "default-terminal", "status-terminal"],
        ] {
            execute(&mut engine, &mut execution, &args);
        }
        let command = "#(printf '%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s' \"$STATUS_SCOPE\" \"${STATUS_HIDDEN-unset}\" \"${STATUS_UNSET-unset}\" \"${HOME-unset}\" \"${TERM-unset}\" \"${TERM_PROGRAM-unset}\" \"${TERM_PROGRAM_VERSION-unset}\" \"${COLORTERM-unset}\" \"$TMUX\" \"$TMUX_PANE\" \"${ZZ_STARTUP_REENTRY-unset}\" \"${ZZ_TMUX_EXECUTABLE-unset}\")";
        let socket = "/tmp/zz-status-clean-environment.sock";
        let expected_tmux = format!("{socket},{},-1", std::process::id());

        let mut post_startup = engine_request(1, &engine, Some(session));
        post_startup.formats = StatusFormats {
            enabled: true,
            left: command.to_owned(),
            style: String::new(),
            left_style: String::new(),
            right_style: String::new(),
            foreground: "default".to_owned(),
            background: "default".to_owned(),
            left_length: u16::MAX,
            right_length: u16::MAX,
            ..StatusFormats::default()
        }
        .into();
        Arc::make_mut(&mut post_startup.context).socket_path = socket.to_owned();
        let status = settled(&mut renderer(), &post_startup);
        assert_eq!(
            status.left,
            format!(
                "global|unset|unset|unset|status-terminal|tmux|3.8-zz|truecolor|{expected_tmux}|status-global-pane|unset|unset"
            )
        );

        let mut startup = engine_request(1, &engine, Some(session));
        startup.formats = StatusFormats {
            enabled: true,
            left: command.to_owned(),
            style: String::new(),
            left_style: String::new(),
            right_style: String::new(),
            foreground: "default".to_owned(),
            background: "default".to_owned(),
            left_length: u16::MAX,
            right_length: u16::MAX,
            ..StatusFormats::default()
        }
        .into();
        Arc::make_mut(&mut startup.context).socket_path = socket.to_owned();
        startup.startup = true;
        let status = settled(&mut renderer(), &startup);
        assert_eq!(
            status.left,
            format!(
                "global|unset|unset|unset|startup-term|startup-program|startup-version|startup-colorterm|{expected_tmux}|status-global-pane|unset|unset"
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn status_commands_resolve_literal_tmux_through_the_private_zz_shim() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temporary status shim");
        let executable = directory.path().join("fake-zz");
        std::fs::write(
            &executable,
            b"#!/bin/sh\nprintf '%s|%s' \"$1\" \"$ZZ_SOCKET\"\n",
        )
        .expect("write fake zz executable");
        let shim = directory.path().join("shim");
        std::fs::create_dir(&shim).expect("create shim directory");
        std::fs::write(
            shim.join("tmux"),
            b"#!/bin/sh\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n",
        )
        .expect("write tmux shim");
        for path in [&executable, &shim.join("tmux")] {
            let mut permissions = std::fs::metadata(path)
                .expect("shim metadata")
                .permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(path, permissions).expect("make shim runnable");
        }

        let socket = "/tmp/zz-status-shim.sock";
        let mut status_request = request(1, "#(tmux status)", "");
        Arc::make_mut(&mut status_request.context).socket_path = socket.to_owned();
        let mut renderer = renderer();
        renderer.set_tmux_shim(shim, executable);

        assert_eq!(
            settled(&mut renderer, &status_request).left,
            format!("status|{socket}")
        );
    }

    #[test]
    fn strftime_and_shell_failures_degrade_to_text() {
        let mut renderer = renderer();
        let status = renderer.render_initial(&request(1, "%H:%M", "#(exit 3)"));
        assert_eq!(status.left.len(), 5, "a clock renders as HH:MM");
        assert!(
            status.right.is_empty(),
            "a failing command renders as blank"
        );
    }

    #[cfg(unix)]
    #[test]
    fn status_job_output_wakes_the_renderer_without_a_periodic_deadline() {
        let mut renderer = renderer();
        struct JobWake(std::sync::mpsc::Sender<()>);
        impl std::task::Wake for JobWake {
            fn wake(self: Arc<Self>) {
                let _ = self.0.send(());
            }
        }
        let (sender, updates) = std::sync::mpsc::channel();
        renderer.set_job_waker(std::task::Waker::from(Arc::new(JobWake(sender))));
        let request = request(1, "#(printf ready)", "");
        assert!(renderer.render_initial(&request).left.is_empty());
        let deadline = Instant::now() + Duration::from_millis(750);
        loop {
            updates
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("job notification");
            renderer.poll_jobs();
            if renderer
                .shell_cache
                .values()
                .any(|entry| entry.output.as_deref() == Some("ready"))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "shell output did not wake the renderer"
            );
        }
        assert!(
            Instant::now() < deadline,
            "shell output did not arrive promptly"
        );
    }

    #[cfg(unix)]
    #[test]
    fn status_jobs_keep_the_last_line_and_trailing_spaces() {
        let status = settled(
            &mut renderer(),
            &request(1, "#(printf 'one\\ntwo \\t\\n')", "#(printf 'one\\ntail')"),
        );
        assert_eq!(status.left, "two \t");
        assert_eq!(status.right, "tail");
    }

    #[cfg(unix)]
    #[test]
    fn status_jobs_preserve_complete_long_lines_in_the_cache() {
        let mut renderer = renderer();
        let expected = format!("{}TAIL", "0".repeat(5000));
        settled(
            &mut renderer,
            &request(
                1,
                &format!("#(printf '{expected}\\n')"),
                &format!("#(printf '{expected}')"),
            ),
        );
        assert_eq!(renderer.shell_cache.len(), 2);
        for entry in renderer.shell_cache.values() {
            assert_eq!(entry.output.as_deref(), Some(expected.as_str()));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_running_job_publishes_lines_and_is_cancelled_on_forget() {
        let directory = tempfile::tempdir().expect("job fixture");
        let release = directory.path().join("release");
        let format = format!(
            "#(mkfifo '{}'; printf 'ready\\n'; read token < '{}'; printf 'done\\n')",
            release.display(),
            release.display()
        );
        let mut request = request(1, &format, "");
        Arc::make_mut(&mut request.facts).client = Some(ClientFormatFacts::default());
        let mut renderer = renderer();
        let started = Instant::now();
        assert!(renderer.render_initial(&request).left.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            renderer.poll_jobs();
            if renderer.render_initial(&request).left == "ready" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "running job output never published"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let pid = renderer
            .shell_cache
            .values()
            .next()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .pid;
        renderer.forget(request.client);
        assert!(renderer.shell_cache.is_empty());
        let deadline = Instant::now() + Duration::from_secs(3);
        while rustix::process::test_kill_process(
            rustix::process::Pid::from_raw(pid as i32).unwrap(),
        )
        .is_ok()
        {
            assert!(
                Instant::now() < deadline,
                "cancelled status job was not reaped"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_same_job_in_two_status_formats_runs_once() {
        let directory = tempfile::tempdir().expect("job count fixture");
        let count = directory.path().join("count");
        let format = counted_value_job(&count);
        let mut renderer = renderer();
        let request = request(1, &format, &format);
        assert_eq!(renderer.render_forced(&request).left, "");
        let status = settled(&mut renderer, &request);
        assert_eq!(status.left, "value");
        assert_eq!(status.right, "value");
        assert_eq!(std::fs::read_to_string(count).unwrap(), "run\n");
    }
}

#[cfg(test)]
mod terminfo_tests {
    use super::*;

    #[test]
    fn the_infocmp_reader_finds_the_extended_section() {
        let Some(entries) = terminfo_entries("xterm-256color") else {
            return;
        };
        assert!(entries.iter().any(|entry| entry.starts_with("AX=")));
        assert!(entries.iter().any(|entry| entry.starts_with("colors=256")));
        assert!(
            entries
                .iter()
                .any(|entry| entry.starts_with("clear=\u{1b}["))
        );
        let term = client_terminal_facts(
            "xterm-256color",
            Some("truecolor"),
            "",
            &["xterm*:clipboard:ccolour:cstyle:focus:title".to_owned()],
            &[],
        )
        .expect("term");
        assert!(term.has_capability("smcup"));
        assert!(term.has_feature("RGB"));
        assert!(term.has_feature("bpaste"));
    }

    #[test]
    fn the_stock_arrays_move_the_other_terms_the_way_the_pin_does() {
        let engine = zz_mux::MuxEngine::default();
        let features = engine.terminal_features_option();
        let overrides = engine.terminal_overrides_option();
        let facts = |term: &str| client_terminal_facts(term, None, "", &features, &overrides);
        if let Some(screen) = facts("screen-256color") {
            assert!(screen.has_feature("bpaste"));
            assert!(screen.has_feature("focus"));
            assert!(screen.has_capability("Enfcs"));
            assert!(!screen.has_capability("XT"));
        }
        if let Some(tmux) = facts("tmux-256color") {
            assert!(tmux.has_feature("bpaste"));
            assert!(tmux.has_feature("focus"));
            assert!(tmux.has_capability("Enfcs"));
        }
        if let Some(linux) = facts("linux") {
            assert!(linux.has_feature("title"));
            assert!(!linux.has_capability("AX"));
        }
        if let Some(rxvt) = facts("rxvt") {
            assert!(!rxvt.has_capability("kf1"));
            assert!(!rxvt.has_capability("kf63"));
        }
    }

    #[test]
    fn the_infocmp_reader_speaks_both_ncurses_generations() {
        let entries = parse_infocmp_entries(
            "#\tReconstructed via infocmp\nxterm-256color|xterm with 256 colors,\n\tam,\n\tcolors#0x100,\n\tpairs#65536,\n\tclear=\\E[H\\E[2J,\n\tkf1=\\EOP,\n\tbel=^G,\n\tacsc=``aaffggiijjkk\\,\\:\\^\\\\,\n\tsmcup@,\n\tU8#1,\n\tSs=\\E[%p1%d q$<5>,\n",
        );
        assert_eq!(
            entries,
            [
                "xterm-256color|xterm with 256 colors=1",
                "am=1",
                "colors=256",
                "pairs=65536",
                "clear=\u{1b}[H\u{1b}[2J",
                "kf1=\u{1b}OP",
                "bel=\u{7}",
                "acsc=``aaffggiijjkk,:^\\",
                "U8=1",
                "Ss=\u{1b}[%p1%d q$<5>",
            ]
        );
        let term = TtyTerm::create(
            "xterm-256color",
            &entries,
            None,
            "",
            &[],
            &["linux*:AX@".to_owned()],
        );
        assert!(term.has_capability("colors"));
        assert!(term.has_feature("bpaste"));
        assert!(!term.has_capability("smcup"));
    }
}
