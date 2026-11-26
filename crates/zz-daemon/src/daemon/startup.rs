use super::*;

impl Shared {
    #[cfg(unix)]
    pub(super) fn install_tmux_shim(&self) -> Result<(), DaemonError> {
        let shim = match std::env::var_os(crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE) {
            Some(executable) => TmuxShimGuard::install(PathBuf::from(executable), false)?,
            None => TmuxShimGuard::install(std::env::current_exe()?, true)?,
        };
        #[cfg(target_os = "macos")]
        if shim
            .executable
            .starts_with(std::env::temp_dir().join(PINNED_ROOT))
        {
            zz_terminal::pin_pty_exec_helper(&shim.executable);
        }
        self.status
            .lock()
            .set_tmux_shim(shim.directory.clone(), shim.executable.clone());
        *self.tmux_shim.lock() = Some(shim);
        Ok(())
    }

    pub(super) fn begin_startup(&self) {
        *self.startup_ready.lock() = false;
    }

    pub(super) fn finish_startup(&self) {
        *self.startup_ready.lock() = true;
        self.startup_changed.notify_all();
        self.resume_pending_execs();
    }

    #[cfg(windows)]
    pub(super) fn wait_for_startup(&self) -> bool {
        let mut ready = self.startup_ready.lock();
        while !*ready && !self.stopping.load(Ordering::Acquire) {
            self.startup_changed.wait(&mut ready);
        }
        *ready
    }

    #[cfg(test)]
    pub(super) fn new(server_id: u64) -> Self {
        Self::with_appearance(server_id, Arc::new(TerminalAppearance::default()))
    }

    #[cfg(test)]
    pub(super) fn initialize(self: &Arc<Self>, load_user_config: bool) -> Result<(), DaemonError> {
        self.initialize_with_mux_config_files(load_user_config, None, None)
    }

    #[cfg(any(windows, test))]
    pub(super) fn initialize_with_mux_config_files(
        self: &Arc<Self>,
        load_user_config: bool,
        mux_config_files: Option<&[PathBuf]>,
        initial_client_working_directory: Option<&Path>,
    ) -> Result<(), DaemonError> {
        #[cfg(any(windows, test))]
        self.start_timers()?;
        let mut context = ExecutionContext::default();
        *self.mux_config_selection.lock() =
            (load_user_config, mux_config_files.map(<[PathBuf]>::to_vec));
        let config_files = self.selected_mux_config_files();
        let mut report = ConfigLoadReport::startup();
        self.inner
            .lock()
            .config
            .startup_source_client_working_directory =
            initial_client_working_directory.map(Path::to_owned);
        let replay_result = self.replay_mux_config_files(
            &config_files,
            &mut context,
            &mut report,
            ClientTerminal::NoClient,
            initial_client_working_directory,
            SourceFileLoadOptions::default(),
        );
        self.inner
            .lock()
            .config
            .startup_source_client_working_directory = None;
        replay_result?;
        *self.startup_config_causes.lock() = report.take_startup_causes();
        self.finish_initialization();
        Ok(())
    }

    pub(super) fn finish_initialization(self: &Arc<Self>) {
        self.apply_stored_mux_config_overrides("startup-mux-replay");
        let history_settings = {
            let inner = self.inner.lock();
            prompt_history_path(inner.engine.history_file())
                .map(|path| (path, inner.engine.prompt_history_limit()))
        };
        if let Some(source) = history_settings {
            self.prompt_history_source.lock().source = Some(source);
            self.prompt_history_settled.store(false, Ordering::Release);
        }
    }

    #[cfg(windows)]
    fn warm_client_terminfo(&self, message: &ProtocolMessage) -> std::io::Result<()> {
        let environment = match message {
            ProtocolMessage::Hello(hello)
                if validate_hello(&hello.clone().into_client()).is_ok() =>
            {
                &hello.client.environment
            }
            ProtocolMessage::ClientHello(hello) if validate_hello(hello).is_ok() => {
                &hello.environment
            }
            _ => return Ok(()),
        };
        self.warm_terminfo(environment)
    }

    #[cfg(windows)]
    pub(super) fn warm_terminfo(&self, environment: &[RawText]) -> std::io::Result<()> {
        if crate::status::terminfo_is_warm(environment) {
            return Ok(());
        }
        self.helpers.require_off_loop()?;
        let (reply, result) = mpsc::sync_channel(1);
        self.helpers.submit_wait(helpers::Task::Terminfo {
            environment: environment.to_vec(),
            #[cfg(unix)]
            jobs: self
                .loop_active
                .load(Ordering::Acquire)
                .then(|| self.helpers.jobs.clone()),
            reply,
        })?;
        result
            .recv()
            .map_err(|_| std::io::Error::other("terminfo helper stopped"))
    }
}
