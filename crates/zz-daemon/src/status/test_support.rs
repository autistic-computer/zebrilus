use super::*;
use std::time::{Duration, Instant};
use zz_mux::StatusValues;

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
    let context =
        status_context(&snapshot, engine, session, None).detach_with_references(needs, &references);
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
