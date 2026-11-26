use super::*;

impl Shared {
    #[cfg(test)]
    pub(super) fn with_appearance(server_id: u64, appearance: Arc<TerminalAppearance>) -> Self {
        Self::configured_with_boot_environment(
            server_id,
            appearance,
            AppearanceProvenance::default(),
            false,
            std::env::temp_dir().join("zz-test-paste"),
            std::env::temp_dir().join("zz-test.sock"),
            None,
            "/usr/bin/vi",
            std::iter::empty::<(String, String)>(),
        )
    }

    pub(super) fn configured(
        server_id: u64,
        bootstrap: bool,
        appearance: Arc<TerminalAppearance>,
        appearance_provenance: AppearanceProvenance,
        load_user_config: bool,
        paste_directory: PathBuf,
        socket_path: PathBuf,
    ) -> Self {
        let visual = std::env::var_os("VISUAL");
        let editor = std::env::var_os("EDITOR");
        let default_editor = editor_from_environment(visual.as_deref(), editor.as_deref());
        let shared = Self::configured_with_boot_environment(
            server_id,
            appearance,
            appearance_provenance,
            load_user_config,
            paste_directory,
            socket_path,
            editor_derived_keys(visual.as_deref(), editor.as_deref()),
            &default_editor,
            daemon_environment(),
        );
        if bootstrap {
            shared.inner.lock().attachment.cold_bootstrap =
                ColdBootstrapLease::AwaitingFirstExternal;
        }
        shared
    }

    pub(super) fn configured_with_boot_environment<I, K, V>(
        server_id: u64,
        appearance: Arc<TerminalAppearance>,
        appearance_provenance: AppearanceProvenance,
        load_user_config: bool,
        paste_directory: PathBuf,
        socket_path: PathBuf,
        editor_keys: Option<&str>,
        default_editor: &str,
        environment: I,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let environment = environment
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect::<Vec<(String, String)>>();
        let default_shell = resolve_runtime_default_shell(
            environment
                .iter()
                .find_map(|(name, value)| (name == "SHELL").then(|| OsStr::new(value))),
        );
        let mut state = ServerState {
            config: Config {
                active_color_scheme: appearance.color_scheme,
                appearance,
                appearance_provenance,
                ..Config::default()
            },
            ..ServerState::default()
        };
        if let Some(keys) = editor_keys {
            state
                .engine
                .set_default_mode_keys(keys)
                .expect("daemon mode-keys default is valid");
            state
                .engine
                .set_default_status_keys(keys)
                .expect("daemon status-keys default is valid");
        }
        state.engine.set_automatic_rename_throttle(true);
        state.engine.initialize_default_editor(default_editor);
        state.engine.initialize_default_shell(default_shell);
        state.engine.seed_global_environment(environment);
        let (host, host_short) = host_names();
        state.engine.set_format_server_context(
            host.clone(),
            host_short.clone(),
            socket_path.display().to_string(),
            unix_timestamp(),
        );
        state
            .engine
            .set_format_server_identity(std::process::id(), daemon_uid(), daemon_user());
        for option in MuxOptionKey::ALL {
            let value = state.engine.mux_option_value(option);
            state
                .config
                .mux_options
                .set(option, value.clone(), MuxOptionSource::Default);
            state
                .config
                .mux_option_underlay
                .set(option, value, MuxOptionSource::Default);
        }
        #[cfg(feature = "agent")]
        let (agent_tx, agent_rx) = agent_publisher::Sender::new();
        let (watcher_tx, watcher_rx) = watchers::Sender::new();
        let (timer_tx, timer_rx) = crossbeam_channel::unbounded();
        let timer_tx = timers::TimerSender::new(timer_tx);
        let mut status = StatusRenderer::default();
        status.set_job_waker(std::task::Waker::from(Arc::new(timer_tx.clone())));
        let status_job_needs = status.job_needs();
        let server = SharedServer {
            accept_wake: AcceptWake::new(),
            control_wake: Arc::new(shard_sink::ControlWake::new()),
            helpers: helpers::Pool::default(),
            #[cfg(any(windows, test))]
            helper_dispatching: AtomicBool::new(false),
            terminal_requests: Arc::default(),
            agent_state_waits: Mutex::default(),
            inner: Mutex::new(state),
            client_writers: Mutex::new(BTreeMap::new()),
            command_queue_cancels: Mutex::new(BTreeMap::new()),
            response_admissions: Mutex::new(ResponseAdmissionState::default()),
            response_admissions_changed: Condvar::new(),
            default_attach_effects: Mutex::new(()),
            pipe_effects: Mutex::new(()),
            #[cfg(unix)]
            pipe_jobs: pipe_jobs::Client::default(),
            #[cfg(all(test, unix))]
            shell_loop_start: Mutex::new(()),
            prompt_history_effects: Mutex::new(history_io::Saves::default()),
            background_insertions: Mutex::new(BackgroundInsertions::default()),
            next_background_insertion: AtomicU64::new(0),
            #[cfg(feature = "agent")]
            agent: Mutex::new(None),
            #[cfg(feature = "agent")]
            agent_effects: Mutex::new(()),
            #[cfg(feature = "agent")]
            chat_deliveries: Mutex::default(),
            #[cfg(feature = "agent")]
            agent_catalog_pending: Mutex::new(BTreeSet::new()),
            providers: Mutex::new(providers::Providers::default()),
            #[cfg(all(feature = "agent", unix))]
            agent_accounts: Mutex::new(accounts::Accounts::default()),
            #[cfg(all(feature = "agent", unix))]
            agent_logins: Mutex::new(agent_login::Logins::default()),
            #[cfg(feature = "agent")]
            agent_stopped: AtomicBool::new(false),
            #[cfg(all(feature = "agent", unix))]
            agent_peers: Mutex::new(BTreeMap::new()),
            #[cfg(feature = "agent")]
            agent_tx,
            #[cfg(feature = "agent")]
            agent_rx: Mutex::new(Some(agent_rx)),
            kitty_image_frames: Mutex::new(BTreeMap::new()),
            terminal_frames: Arc::default(),
            pasted_images: Mutex::new(BTreeMap::new()),
            status: Mutex::new(status),
            stopping: AtomicBool::new(false),
            shutdown_pending: AtomicBool::new(false),
            shutdown_forced: AtomicBool::new(false),
            shutdown_announced: Mutex::new(false),
            shutdown_blockers: Mutex::new(ShutdownBlockerState::default()),
            shutdown_drops_event_hooks: AtomicBool::new(false),
            #[cfg(unix)]
            shutdown_cleanup_started: AtomicBool::new(false),
            #[cfg(unix)]
            shutdown_cleanup_complete: AtomicBool::new(false),
            startup_ready: Mutex::new(true),
            startup_changed: Condvar::new(),
            startup_config_causes: Mutex::new(None),
            exit_empty_armed: AtomicBool::new(false),
            restart_requested: AtomicBool::new(false),
            server_id,
            mux_config_selection: Mutex::new((load_user_config, None)),
            zz_mux_config_path: Mutex::new(None),
            paste_directory,
            socket_path,
            #[cfg(test)]
            delivered_wrap_search_commands: Mutex::new(Vec::new()),
            #[cfg(test)]
            response_admission_hook: Mutex::new(None),
            #[cfg(test)]
            destroy_unattached_hook: Mutex::new(None),
            #[cfg(test)]
            begin_stopping_hook: Mutex::new(None),
            #[cfg(unix)]
            tmux_shim: Mutex::new(None),
            status_job_needs,
            watcher_tx,
            watcher_rx: Mutex::new(Some(watcher_rx)),
            timer_tx,
            timer_rx: Mutex::new(Some(timer_rx)),
            publish_flush: Mutex::new(timers::PublishFlush::default()),
            snapshot_order: Mutex::new(()),
            #[cfg(all(feature = "agent", unix))]
            peer_probe: AtomicBool::new(false),
            pending_execs: Mutex::new(Vec::new()),
            #[cfg(unix)]
            pending_wait_queues: Mutex::new(Vec::new()),
            #[cfg(windows)]
            exec_links: Mutex::new(BTreeMap::new()),
            prompt_history_source: Mutex::new(history_io::Load::default()),
            prompt_history_settled: AtomicBool::new(true),
            connection_threads: Arc::default(),
            #[cfg(unix)]
            lifecycle: lifecycle::Inbox::default(),
            #[cfg(unix)]
            loop_active: AtomicBool::new(false),
        };
        Self {
            server: Arc::new(server),
            command_item: None,
            owner: None,
            watcher_effects: None,
        }
    }

    pub(super) fn publish_effective_mux_options(&self, sessions: Option<&BTreeSet<SessionId>>) {
        let recipients = {
            let mut inner = self.inner.lock();
            let clients = inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
                .copied()
                .filter(|client| {
                    sessions.is_none_or(|sessions| {
                        client_attached_session(&inner, *client)
                            .is_some_and(|session| sessions.contains(&session))
                    })
                })
                .collect::<Vec<_>>();
            clients
                .into_iter()
                .filter_map(|client| {
                    let options = effective_mux_options(&inner, client);
                    if inner
                        .client(client)
                        .and_then(|c| c.published_mux_options.as_ref())
                        == Some(&options)
                    {
                        return None;
                    }
                    inner
                        .client_entry(client)
                        .published_mux_options
                        .replace(options.clone());
                    ctrl::options_event(&inner, client, options).map(|payload| (client, payload))
                })
                .collect::<Vec<_>>()
        };
        for (client, payload) in recipients {
            self.publish_to_client(client, payload);
        }
    }

    pub(super) fn publish_effective_mux_options_to(
        &self,
        client: ClientId,
        outbound: &Arc<OutboundMailbox>,
    ) -> bool {
        let mut inner = self.inner.lock();
        if !inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .is_some_and(|subscriber| Arc::ptr_eq(subscriber, outbound))
        {
            return false;
        }
        let options = effective_mux_options(&inner, client);
        if inner
            .client(client)
            .and_then(|c| c.published_mux_options.as_ref())
            == Some(&options)
        {
            return outbound.is_open();
        }
        if let Some(payload) = ctrl::options_event(&inner, client, options.clone())
            && !outbound.enqueue_reliable(&Self::event(payload))
        {
            return false;
        }
        inner
            .client_entry(client)
            .published_mux_options
            .replace(options);
        true
    }

    pub(super) fn refresh_published_appearance(&self) {
        let update = {
            let mut inner = self.inner.lock();
            let merged = published_appearance(&inner);
            let previous = inner
                .config
                .published_style_appearance
                .clone()
                .unwrap_or_else(|| Arc::clone(&inner.config.appearance));
            if *previous == *merged {
                inner.config.published_style_appearance = Some(merged);
                None
            } else {
                inner.config.published_style_appearance = Some(Arc::clone(&merged));
                Some((merged, inner.config.appearance_provenance.clone()))
            }
        };
        if let Some((appearance, provenance)) = update {
            self.publish(EventPayload::AppearanceChanged {
                appearance: Box::new((*appearance).clone()),
                provenance,
            });
        }
    }

    pub(super) fn set_config_overrides(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        entries: &[ConfigOverrideEntry],
    ) {
        let (appearance_entries, mux_entries) = partition_config_overrides(entries);

        let (color_scheme, restore) = {
            let mut inner = self.inner.lock();
            if kind != ClientKind::Interactive
                || inner.client(client).is_none_or(|c| c.subscriber.is_none())
            {
                log::warn!(
                    target: "zz_daemon::diagnostics::appearance",
                    "ignored configuration overrides from non-interactive client={client}",
                );
                return;
            }
            if inner.client(client).is_some_and(|c| c.remote) {
                log::debug!(
                    target: "zz_daemon::diagnostics::appearance",
                    "ignored configuration overrides from remote client={client}",
                );
                return;
            }
            let previous_mux_keys = inner
                .config
                .mux_config_overrides
                .iter()
                .filter_map(|(key, _)| MuxOptionKey::from_config_key(key))
                .collect::<BTreeSet<_>>();
            let restore = previous_mux_keys
                .into_iter()
                .filter_map(|key| {
                    inner
                        .config
                        .mux_option_underlay
                        .get(key)
                        .cloned()
                        .map(|value| (key, value))
                })
                .collect::<Vec<_>>();
            inner
                .config
                .appearance_config_overrides
                .clone_from(&appearance_entries);
            inner.config.mux_config_overrides.clone_from(&mux_entries);
            (
                inner
                    .client(client)
                    .and_then(|c| c.color_scheme)
                    .unwrap_or(inner.config.active_color_scheme),
                restore,
            )
        };

        let load = resolve_appearance(color_scheme, &appearance_entries);
        log_appearance_load("config-overrides", &load);
        {
            let appearance = Arc::new(load.appearance);
            let provenance = load.provenance;
            let (terminals, changed, published) = {
                let mut inner = self.inner.lock();
                if inner.config.appearance_config_overrides != appearance_entries {
                    return;
                }
                let changed = *inner.config.appearance != *appearance
                    || inner.config.appearance_provenance != provenance;
                inner.config.active_color_scheme = color_scheme;
                inner.config.appearance = Arc::clone(&appearance);
                inner.config.appearance_provenance.clone_from(&provenance);
                let terminals = if changed {
                    terminal_appearance_updates(&inner)
                } else {
                    Vec::new()
                };
                let published = published_appearance(&inner);
                inner.config.published_style_appearance = Some(Arc::clone(&published));
                (terminals, changed, published)
            };
            log::info!(
                target: "zz_daemon::diagnostics::appearance",
                "applied configuration override set entries={}",
                appearance_entries.len(),
            );
            if changed {
                for (terminal, pane_appearance) in terminals {
                    terminal.set_appearance(pane_appearance);
                }
                self.publish(EventPayload::AppearanceChanged {
                    appearance: Box::new((*published).clone()),
                    provenance,
                });
            }
        }

        self.restore_mux_option_underlay(&restore, "config-overrides");
        self.apply_mux_config_overrides(&mux_entries, "config-overrides");
    }

    fn restore_mux_option_underlay(
        self: &Arc<Self>,
        entries: &[(MuxOptionKey, zz_protocol::MuxOptionValue)],
        reason: &str,
    ) {
        let mut context = ExecutionContext::default();
        for (option, entry) in entries {
            let command = mux_set_option_command(*option, &entry.value);
            if let Err(error) = self.execute_with_mux_source(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
                entry.source,
            ) {
                log::warn!(
                    target: "zz_daemon::diagnostics::mux_config",
                    "could not restore mux option underlay reason={reason} key={} error={error}",
                    option.as_str(),
                );
            }
        }
    }

    pub(super) fn apply_mux_config_overrides(
        self: &Arc<Self>,
        entries: &[ConfigOverrideEntry],
        reason: &str,
    ) -> MuxOverrideApplyReport {
        let mut report = MuxOverrideApplyReport::default();
        let mut context = ExecutionContext::default();
        for (key, value) in entries {
            let Some(option) = MuxOptionKey::from_config_key(key) else {
                let diagnostic = format!("unsupported mux override key: {key}");
                log::warn!(
                    target: "zz_daemon::diagnostics::mux_config",
                    "mux override reason={reason} key={key:?} disposition=invalid message={diagnostic}",
                );
                report.diagnostics.push(diagnostic);
                continue;
            };
            let command = mux_set_option_command(option, value);
            match self.execute_with_mux_source(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
                MuxOptionSource::Override,
            ) {
                Ok(_) => report.applied += 1,
                Err(error) => {
                    let diagnostic = error.to_string();
                    log::warn!(
                        target: "zz_daemon::diagnostics::mux_config",
                        "mux override reason={reason} key={} disposition=invalid message={diagnostic}",
                        option.as_str(),
                    );
                    report.diagnostics.push(diagnostic);
                }
            }
        }
        let (prefix, mode_keys) = {
            let inner = self.inner.lock();
            (
                inner.engine.mux_option_value(MuxOptionKey::Prefix),
                inner.engine.mux_option_value(MuxOptionKey::ModeKeys),
            )
        };
        log::info!(
            target: "zz_daemon::diagnostics::mux_config",
            "applied mux configuration override set reason={reason} entries={} prefix={prefix:?} mode_keys={mode_keys:?}",
            report.applied,
        );
        report
    }

    pub(super) fn apply_stored_mux_config_overrides(self: &Arc<Self>, reason: &str) {
        let entries = self.inner.lock().config.mux_config_overrides.clone();
        self.apply_mux_config_overrides(&entries, reason);
    }

    pub(super) fn set_client_color_scheme(
        self: &Arc<Self>,
        client: ClientId,
        color_scheme: TerminalColorScheme,
    ) {
        let (appearance_changed, appearance_config_overrides, hook_event) = {
            let mut inner = self.inner.lock();
            if inner.client(client).is_none_or(|c| c.subscriber.is_none())
                || inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
            {
                return;
            }
            if inner.client(client).is_some_and(|c| c.remote)
                && inner.clients.values().any(|c| {
                    !c.remote && c.kind == Some(ClientKind::Interactive) && c.subscriber.is_some()
                })
            {
                log::debug!(
                    target: "zz_daemon::diagnostics::appearance",
                    "ignored color scheme from remote client={client} while a local client is connected",
                );
                return;
            }
            if let Some(registered) = inner.client_mut(client) {
                registered.color_scheme = Some(color_scheme);
            }
            inner.config.active_color_scheme = color_scheme;
            (
                inner.config.appearance.color_scheme != color_scheme,
                inner.config.appearance_config_overrides.clone(),
                client_hook_event(
                    &inner,
                    match color_scheme {
                        TerminalColorScheme::Light => "client-light-theme",
                        TerminalColorScheme::Dark => "client-dark-theme",
                    },
                    client,
                ),
            )
        };
        if let Some(event) = hook_event {
            self.run_event_hooks(vec![event]);
        }
        log::debug!(
            target: "zz_daemon::diagnostics::appearance",
            "recorded system color scheme client={client} scheme={} appearance_changed={appearance_changed}",
            color_scheme.as_str(),
        );
        if !appearance_changed {
            return;
        }

        let load = resolve_appearance(color_scheme, &appearance_config_overrides);
        log_appearance_load("system-color-scheme", &load);
        let appearance = Arc::new(load.appearance);
        let provenance = load.provenance;
        let (terminals, published) = {
            let mut inner = self.inner.lock();
            if inner.config.active_color_scheme != color_scheme
                || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                || inner.config.appearance_config_overrides != appearance_config_overrides
            {
                return;
            }
            inner.config.appearance = Arc::clone(&appearance);
            inner.config.appearance_provenance.clone_from(&provenance);
            let published = published_appearance(&inner);
            inner.config.published_style_appearance = Some(Arc::clone(&published));
            (terminal_appearance_updates(&inner), published)
        };
        for (terminal, pane_appearance) in terminals {
            terminal.set_appearance(pane_appearance);
        }
        self.publish(EventPayload::AppearanceChanged {
            appearance: Box::new((*published).clone()),
            provenance,
        });
    }
}

pub(super) fn editor_derived_keys(
    visual: Option<&OsStr>,
    editor: Option<&OsStr>,
) -> Option<&'static str> {
    let editor = visual.or(editor)?;
    let editor = editor.to_string_lossy();
    let basename = editor.rsplit('/').next().unwrap_or(&editor);
    Some(if basename.contains("vi") {
        "vi"
    } else {
        "emacs"
    })
}

pub(super) fn editor_from_environment(visual: Option<&OsStr>, editor: Option<&OsStr>) -> String {
    visual.or(editor).map_or_else(
        || "/usr/bin/vi".to_owned(),
        |value| value.to_string_lossy().into_owned(),
    )
}

#[cfg(unix)]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
fn passwd_shell() -> Option<PathBuf> {
    use std::{ffi::CStr, mem::MaybeUninit, os::unix::ffi::OsStrExt as _};

    let mut passwd = MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0_u8; 16 * 1024];
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            passwd.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &raw mut result,
        )
    };
    if status != 0 || result.is_null() {
        return None;
    }
    let passwd = unsafe { passwd.assume_init() };
    if passwd.pw_shell.is_null() {
        return None;
    }
    let shell = unsafe { CStr::from_ptr(passwd.pw_shell) };
    Some(PathBuf::from(OsStr::from_bytes(shell.to_bytes())))
}

#[cfg(unix)]
fn resolve_runtime_default_shell(environment_shell: Option<&OsStr>) -> String {
    if let Some(shell) = environment_shell {
        let shell = Path::new(shell);
        if shell_is_valid(shell) {
            return shell.to_string_lossy().into_owned();
        }
    }
    if let Some(shell) = passwd_shell()
        && shell_is_valid(&shell)
    {
        return shell.to_string_lossy().into_owned();
    }
    "/bin/sh".to_owned()
}

#[cfg(not(unix))]
fn resolve_runtime_default_shell(environment_shell: Option<&OsStr>) -> String {
    environment_shell
        .and_then(OsStr::to_str)
        .unwrap_or("cmd.exe")
        .to_owned()
}

pub(super) fn daemon_environment() -> Vec<(String, String)> {
    let mut environment = std::env::vars_os()
        .filter(|(name, _)| {
            !crate::PARENT_CLAUDE_SESSION_ENVIRONMENT
                .iter()
                .any(|candidate| name == *candidate)
        })
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if let Ok(cwd) = std::env::current_dir() {
        let cwd = environment
            .get("PWD")
            .filter(|pwd| !pwd.is_empty())
            .filter(|pwd| {
                fs::canonicalize(pwd)
                    .ok()
                    .zip(fs::canonicalize(&cwd).ok())
                    .is_some_and(|(pwd, cwd)| pwd == cwd)
            })
            .cloned()
            .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
        environment.insert("PWD".to_owned(), cwd);
    }
    environment.into_iter().collect()
}

fn daemon_uid() -> String {
    #[cfg(unix)]
    {
        rustix::process::getuid().as_raw().to_string()
    }
    #[cfg(not(unix))]
    {
        String::new()
    }
}

fn daemon_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct MuxOverrideApplyReport {
    pub(super) applied: usize,
    pub(super) diagnostics: Vec<String>,
}

fn partition_config_overrides(
    entries: &[ConfigOverrideEntry],
) -> (Vec<ConfigOverrideEntry>, Vec<ConfigOverrideEntry>) {
    let mut appearance_entries = Vec::new();
    let mut mux_entries = Vec::new();
    for entry in entries {
        if zz_terminal::AppearanceConfigKey::from_config_key(&entry.0).is_some() {
            appearance_entries.push(entry.clone());
        } else if MuxOptionKey::from_config_key(&entry.0).is_some() {
            mux_entries.push(entry.clone());
        } else {
            log::warn!(
                target: "zz_daemon::diagnostics::mux_config",
                "ignored unsupported configuration override key={:?}",
                entry.0,
            );
        }
    }
    (appearance_entries, mux_entries)
}

fn mux_set_option_command(option: MuxOptionKey, value: &str) -> CommandInvocation {
    CommandInvocation::new("set-option", ["-g", "--", option.as_str(), value])
}
