use super::*;

static JOB_SERIAL: AtomicU64 = AtomicU64::new(0);

impl ShellCacheEntry {
    pub(super) fn set_output(&mut self, output: String) -> bool {
        if self.output.as_ref() == Some(&output) {
            return false;
        }
        self.output_needs = zz_mux::format_needs_without_engine([output.as_str()]);
        self.output = Some(output);
        true
    }

    pub(super) fn poll(&mut self, command: &str) -> bool {
        let Some(job) = self.job.as_mut() else {
            return false;
        };
        let mut latest = None;
        let mut complete = false;
        let mut streamed = false;
        let mut failed = false;
        for update in job.updates.try_iter() {
            job.fd = update.fd;
            job.pid = update.pid;
            latest = update.latest.or(latest);
            complete |= update.complete;
            streamed |= update.streamed;
            failed |= update.failed;
        }
        let mut changed = if failed {
            self.set_output(format!("<'{command}' didn't start>"))
        } else {
            latest.is_some_and(|output| self.set_output(output))
        };
        if streamed {
            self.last = shell_second();
        }
        if complete {
            self.job = None;
            changed |= self.last != shell_second();
        }
        changed
    }
}

impl Drop for ShellJob {
    fn drop(&mut self) {
        self.client.cancel(self.serial);
    }
}

pub(super) fn shell_second() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn run_shell(
    command: &str,
    context: &StatusContext,
    cwd: &Path,
    environment: &[(RawText, Option<RawText>)],
    default_terminal: &str,
    startup: bool,
    tmux_shim: Option<&Path>,
    zz_executable: Option<&Path>,
    job_waker: Option<std::task::Waker>,
    client: &crate::daemon::status_jobs::StatusClient,
) -> Option<ShellJob> {
    let mut process = shell_process(command);
    let socket_path = context.variable("socket_path").unwrap_or_default();
    let tmux = format!("{},{},-1", socket_path, std::process::id());
    configure_shell_job_environment(
        &mut process,
        environment,
        default_terminal,
        startup,
        &tmux,
        std::ffi::OsStr::new(socket_path.as_ref()),
        tmux_shim,
        zz_executable,
    );
    process.current_dir(cwd).env("PWD", cwd.as_os_str());
    let serial = JOB_SERIAL.fetch_add(1, Ordering::Relaxed);
    Some(ShellJob {
        updates: client.launch(serial, process, job_waker).ok()?,
        client: client.clone(),
        fd: -1,
        pid: 0,
        serial,
    })
}

pub(super) fn status_working_directory(context: &StatusContext) -> PathBuf {
    let session_path = context.variable("session_path").unwrap_or_default();
    let requested = Path::new(session_path.as_ref());
    if requested.is_dir() {
        requested.to_path_buf()
    } else {
        home_directory()
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}
