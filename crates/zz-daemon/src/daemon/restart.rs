use super::*;

#[derive(Debug)]
pub enum Stopped {
    Exit,
    Restart(Restart),
}

#[derive(Debug)]
pub struct Restart {
    #[cfg(unix)]
    pub(super) lock: Option<StartLock>,
}

impl Restart {
    #[cfg(unix)]
    pub fn inheritable_lock_fd(&self) -> Option<i32> {
        use std::os::fd::AsRawFd as _;

        let file = &self.lock.as_ref()?._file;
        rustix::io::fcntl_setfd(file, rustix::io::FdFlags::empty()).ok()?;
        Some(file.as_raw_fd())
    }
}

impl Shared {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn restart_server(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        args: &[RawText],
        mux_source: MuxOptionSource,
        invoking_client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        if !args.is_empty() {
            return Err(ServerError::CommandParse("usage: restart-server".to_owned()).into());
        }
        self.restart_requested.store(true, Ordering::Release);
        self.execute_with_mux_source_inner(
            client,
            kind,
            context,
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
            mux_source,
            invoking_client_terminal,
            queue_execution,
            None,
        )
    }
}
