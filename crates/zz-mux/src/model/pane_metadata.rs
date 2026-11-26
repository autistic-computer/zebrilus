use super::*;

impl MuxState {
    pub fn update_pane_title(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
    ) -> Result<bool, ServerError> {
        let title = title.into();
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if pane_state.title == title {
            return Ok(false);
        }
        pane_state.title = title;
        self.bump_generation();
        Ok(true)
    }

    pub(crate) fn pin_pane_title(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
    ) -> Result<bool, ServerError> {
        let changed = self.update_pane_title(pane, title)?;
        if let Some(pane_state) = self.pane_mut(pane) {
            pane_state.title_pinned = true;
        }
        Ok(changed)
    }

    pub fn update_pane_title_from_terminal(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
        program_wrote_title: bool,
    ) -> Result<bool, ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !program_wrote_title && pane_state.title_pinned {
            return Ok(false);
        }
        pane_state.title_pinned = false;
        self.update_pane_title(pane, title)
    }

    pub fn update_agent_title(
        &mut self,
        pane: PaneId,
        title: Option<&str>,
    ) -> Result<bool, ServerError> {
        let kind = &self
            .pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .kind;
        if !matches!(kind, PaneKind::Agent(_)) {
            return Ok(false);
        }
        let fallback = pane_title(kind, "");
        match title {
            Some(title) => self.update_pane_title_from_terminal(pane, title, true),
            None => self.update_pane_title_from_terminal(pane, fallback, false),
        }
    }

    pub fn set_pane_bell(&mut self, pane: PaneId, bell: bool) -> bool {
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        if pane_state.bell == bell {
            return false;
        }
        pane_state.bell = bell;
        self.bump_generation();
        true
    }

    pub fn set_pane_status(&mut self, pane: PaneId, status: Option<PaneStatus>) -> bool {
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        if pane_state.status == status {
            return false;
        }
        pane_state.status = status;
        self.bump_generation();
        true
    }

    pub fn update_pane_summary(
        &mut self,
        pane: PaneId,
        edit: impl FnOnce(&mut PaneSummary),
    ) -> bool {
        let Some(current) = self.pane(pane) else {
            return false;
        };
        let mut summary = current.summary.clone();
        edit(&mut summary);
        if current.summary == summary {
            return false;
        }
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        pane_state.summary = summary;
        self.bump_generation();
        true
    }

    pub fn clear_pane_alerts(&mut self, pane: PaneId) -> bool {
        let bell = self.set_pane_bell(pane, false);
        let unseen = self.update_pane_summary(pane, |summary| summary.unseen = false);
        bell || unseen
    }

    pub fn materialize_pane(
        &mut self,
        pane: PaneId,
        kind: PaneKind,
    ) -> Result<Option<PaneId>, ServerError> {
        if matches!(kind, PaneKind::Picker { .. }) {
            return Err(ServerError::InvalidCommand(
                "a pane picker cannot materialize another picker".to_owned(),
            ));
        }
        let inherit_cwd_from = match &self
            .pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .kind
        {
            PaneKind::Picker { inherit_cwd_from } => *inherit_cwd_from,
            _ => {
                return Err(ServerError::InvalidCommand(format!(
                    "pane {pane} is not awaiting a type selection"
                )));
            }
        };
        let inherit_cwd_from = inherit_cwd_from
            .filter(|donor| self.cwd_donor(*donor) == Some(*donor))
            .or_else(|| self.cwd_donor(pane));
        let title = pane_title(&kind, &self.default_pane_title);
        let pane_state = self
            .pane_mut(pane)
            .expect("the validated picker pane still exists");
        pane_state.title = title;
        pane_state.title_pinned = false;
        pane_state.kind = kind;
        self.bump_generation();
        Ok(inherit_cwd_from)
    }

    pub fn convert_pane(&mut self, pane: PaneId, kind: PaneKind) -> Result<(), ServerError> {
        let default_title = self.default_pane_title.clone();
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !matches!(
            (&pane_state.kind, &kind),
            (PaneKind::Terminal, PaneKind::Agent(_)) | (PaneKind::Agent(_), PaneKind::Terminal)
        ) {
            let into = if matches!(kind, PaneKind::Agent(_)) {
                "an agent"
            } else {
                "a terminal"
            };
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} cannot turn into {into}"
            )));
        }
        if !pane_state.title_pinned {
            pane_state.title = pane_title(&kind, &default_title);
        }
        pane_state.kind = kind;
        pane_state.status = None;
        pane_state.summary.process.clear();
        pane_state.summary.flip_armed = false;
        self.revive_pane(pane)?;
        self.bump_generation();
        Ok(())
    }

    pub fn update_agent(
        &mut self,
        pane: PaneId,
        update: impl FnOnce(&mut AgentDescriptor),
    ) -> Result<bool, ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Agent(agent) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an agent"
            )));
        };
        let before = agent.clone();
        update(agent);
        if *agent == before {
            return Ok(false);
        }
        self.bump_generation();
        Ok(true)
    }

    pub fn update_browser_url(&mut self, pane: PaneId, url: String) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        let index = browser.active_tab.min(browser.tabs.len().saturating_sub(1));
        match browser.tabs.get_mut(index) {
            Some(active) if *active == url => return Ok(()),
            Some(active) => *active = url,
            None => browser.tabs.push(url),
        }
        self.bump_generation();
        Ok(())
    }

    pub fn update_browser_tabs(
        &mut self,
        pane: PaneId,
        tabs: Vec<String>,
        active_tab: usize,
    ) -> Result<(), ServerError> {
        if tabs.is_empty() {
            return Err(ServerError::InvalidCommand(
                "a browser pane needs at least one tab".to_owned(),
            ));
        }
        if active_tab >= tabs.len() {
            return Err(ServerError::InvalidCommand(format!(
                "active tab {active_tab} is out of range for {} tabs",
                tabs.len()
            )));
        }
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        if browser.tabs == tabs && browser.active_tab == active_tab {
            return Ok(());
        }
        browser.tabs = tabs;
        browser.active_tab = active_tab;
        self.bump_generation();
        Ok(())
    }

    pub fn update_editor_cwd(&mut self, pane: PaneId, cwd: String) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Editor(editor) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an editor"
            )));
        };
        let next = EditorDescriptor {
            path: editor.path.clone(),
            cwd,
        };
        next.validate()
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        if *editor == next {
            return Ok(());
        }
        *editor = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_editor_path(
        &mut self,
        pane: PaneId,
        path: Option<String>,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Editor(editor) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an editor"
            )));
        };
        let next = EditorDescriptor {
            path,
            cwd: editor.cwd.clone(),
        };
        next.validate()
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        if *editor == next {
            return Ok(());
        }
        *editor = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_view(
        &mut self,
        pane: PaneId,
        descriptor: ViewDescriptor,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::View(view) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a view"
            )));
        };
        if *view == descriptor {
            return Ok(());
        }
        if !pane_state.title_pinned {
            pane_state.title.clone_from(&descriptor.title);
        }
        *view = descriptor;
        self.bump_generation();
        Ok(())
    }

    pub fn update_simulator_device(
        &mut self,
        pane: PaneId,
        device: &str,
    ) -> Result<(), ServerError> {
        let next = simulator_descriptor(device)?;
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Simulator(simulator) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a simulator"
            )));
        };
        if *simulator == next {
            return Ok(());
        }
        *simulator = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_browser_profile(
        &mut self,
        pane: PaneId,
        profile: &str,
    ) -> Result<(), ServerError> {
        let profile = normalize_browser_profile_name(profile)
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        if browser.profile == profile {
            return Ok(());
        }
        browser.profile = profile;
        browser.tabs = vec![browser.url().to_owned()];
        browser.active_tab = 0;
        self.bump_generation();
        Ok(())
    }
}

pub(crate) fn simulator_descriptor(device: &str) -> Result<SimulatorDescriptor, ServerError> {
    if device.len() > MAX_SIMULATOR_DEVICE_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "simulator device cannot exceed {MAX_SIMULATOR_DEVICE_BYTES} bytes"
        )));
    }
    Ok(SimulatorDescriptor {
        device: device.to_owned(),
    })
}

pub(super) fn pane_title(kind: &PaneKind, terminal: &str) -> String {
    match kind {
        PaneKind::Picker { .. } => "new pane".to_owned(),
        PaneKind::Terminal if terminal.is_empty() => "terminal".to_owned(),
        PaneKind::Terminal => terminal.to_owned(),
        PaneKind::Browser(browser) if browser.url() == "about:blank" => "browser".to_owned(),
        PaneKind::Browser(browser) => browser.url().to_owned(),
        PaneKind::Editor(_) => "editor".to_owned(),
        PaneKind::Agent(_) => "agent".to_owned(),
        PaneKind::View(view) => view.title.clone(),
        PaneKind::Simulator(_) => "Simulator".to_owned(),
    }
}
