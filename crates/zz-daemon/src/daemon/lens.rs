use super::*;

#[cfg(unix)]
use crate::agent::lens::{LensPane, LensSpec};

#[cfg(unix)]
struct TerminalLens {
    owner: Weak<Shared>,
    pane: PaneId,
}

#[cfg(unix)]
impl LensPane for TerminalLens {
    fn pid(&self) -> Option<u32> {
        let shared = self.owner.upgrade()?;
        let inner = shared.inner.lock();
        inner.engine.pane_runtime_facts(self.pane)?.pid
    }

    fn submit(&self, text: &str) -> Result<(), String> {
        let shared = self.owner.upgrade().ok_or("zz is shutting down")?;
        shared
            .paste_and_submit(self.pane, text, SEND_TEXT_TIMEOUT, true)
            .map_err(|error| error.to_string())
    }

    fn keys(&self, keys: &[&str]) -> Result<(), String> {
        let shared = self.owner.upgrade().ok_or("zz is shutting down")?;
        let terminal = shared
            .inner
            .lock()
            .terminals
            .get(&self.pane)
            .cloned()
            .ok_or_else(|| format!("{} exited", self.pane))?;
        let tokens = keys
            .iter()
            .map(|key| {
                if key.chars().count() == 1 {
                    zz_protocol::KeyToken::Literal((*key).to_owned())
                } else {
                    zz_protocol::KeyToken::Named((*key).to_owned())
                }
            })
            .collect::<Vec<_>>();
        crate::keys::send_tokens(&[terminal], &tokens)
            .then_some(())
            .ok_or_else(|| format!("{}: input queue is full", self.pane))
    }

    fn attach(&self, images: &[zz_protocol::agent_stream::AgentImage]) -> Result<(), String> {
        let shared = self.owner.upgrade().ok_or("zz is shutting down")?;
        let directory = shared.server.paste_directory.clone();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        for (index, image) in images.iter().enumerate() {
            let extension = match image.format.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                "image/webp" => "webp",
                other => return Err(format!("the CLI cannot take {other} images")),
            };
            let stem = format!("lens-{}-{stamp}-{index}", self.pane.0);
            let path = write_paste_upload(&directory, &stem, extension, &image.data)
                .map_err(|error| format!("could not save the image: {error}"))?;
            let path = path
                .to_str()
                .ok_or("the image landed on a path that is not valid UTF-8")?
                .to_owned();
            shared
                .paste_and_submit(self.pane, &path, SEND_TEXT_TIMEOUT, false)
                .map_err(|error| format!("could not paste the image: {error}"))?;
        }
        prune_paste_uploads(&directory, PASTE_UPLOAD_RETENTION);
        Ok(())
    }

    fn screen(&self) -> Option<String> {
        let shared = self.owner.upgrade()?;
        let terminal = shared.inner.lock().terminals.get(&self.pane).cloned()?;
        terminal.capture(CaptureOptions::default()).ok()
    }

    fn channel(&self) -> Option<PathBuf> {
        let shared = self.owner.upgrade()?;
        crate::agent::claude_plugin::lens_socket(&shared.socket_path, self.pane)
    }
}

pub(super) fn lens_provider(process: &[String]) -> Option<AgentProvider> {
    process.iter().find_map(|candidate| {
        let name = candidate.rsplit(['/', '\\']).next()?;
        let name = name.strip_suffix(".exe").unwrap_or(name);
        match name.to_ascii_lowercase().as_str() {
            "claude" => Some(AgentProvider::ClaudeCode),
            "codex" => Some(AgentProvider::Codex),
            _ => None,
        }
    })
}

impl Shared {
    pub(super) fn terminal_agent(&self, pane: PaneId) -> Option<AgentProvider> {
        let inner = self.inner.lock();
        let state = inner.engine.state.pane(pane)?;
        if !matches!(state.kind, PaneKind::Terminal) {
            return None;
        }
        lens_provider(&state.summary.process)
    }

    #[cfg(not(unix))]
    pub(super) fn open_lens(self: &Arc<Self>, _pane: PaneId) -> bool {
        false
    }

    #[cfg(unix)]
    pub(super) fn open_lens(self: &Arc<Self>, pane: PaneId) -> bool {
        let _effects = self.agent_effects.lock();
        if self.agent_stopped.load(Ordering::Acquire) {
            return false;
        }
        let Some(runtime) = self.agent_runtime() else {
            return false;
        };
        if runtime.pane_generation(pane).is_some() {
            return true;
        }
        let Some(provider) = self.terminal_agent(pane) else {
            return false;
        };
        let cwd = self
            .inner
            .lock()
            .terminals
            .get(&pane)
            .and_then(|terminal| terminal_working_directory(terminal))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.inner.lock().lens_panes.insert(pane);
        self.refresh_agent_visibility();
        let spec = AgentPaneSpec {
            provider,
            cwd,
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
            lens: Some(LensSpec {
                pane,
                provider,
                io: Arc::new(TerminalLens {
                    owner: Arc::downgrade(&self.server_owner()),
                    pane,
                }),
            }),
        };
        if runtime.open(pane, spec) {
            return true;
        }
        self.inner.lock().lens_panes.remove(&pane);
        self.refresh_agent_visibility();
        false
    }

    #[cfg(test)]
    pub(super) fn open_test_agent(self: &Arc<Self>, pane: PaneId) -> bool {
        let Some(runtime) = self.agent_runtime() else {
            return false;
        };
        self.inner.lock().lens_panes.insert(pane);
        self.refresh_agent_visibility();
        let spec = AgentPaneSpec {
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
            lens: None,
        };
        runtime.open(pane, spec)
    }

    pub(super) fn recent_agent_terminal(&self, origin: PaneId) -> Option<PaneId> {
        let candidates = {
            let inner = self.inner.lock();
            let window =
                &inner.engine.state.windows[&inner.engine.state.window_for_pane(origin)?];
            std::iter::once(&window.active_pane)
                .chain(window.last_panes())
                .chain(window.pane_order())
                .copied()
                .filter(|pane| *pane != origin)
                .collect::<Vec<_>>()
        };
        candidates
            .into_iter()
            .find(|pane| self.terminal_agent(*pane).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_running_claude_or_codex_gets_a_lens() {
        let process = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lens_provider(&process(&["/home/me/.local/bin/claude", "node"])),
            Some(AgentProvider::ClaudeCode)
        );
        assert_eq!(
            lens_provider(&process(&["node", "/opt/homebrew/bin/codex"])),
            Some(AgentProvider::Codex)
        );
        assert_eq!(
            lens_provider(&process(&["C:\\tools\\Codex.exe"])),
            Some(AgentProvider::Codex)
        );
        assert_eq!(lens_provider(&process(&["/bin/zsh", "vim"])), None);
    }
}
