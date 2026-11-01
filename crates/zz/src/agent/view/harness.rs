use ala::{AnyElement, Context, Entity, IntoElement, Window};
use std::sync::Arc;
use zz_client::agent_settings::{
    fork_chat_command, real_session, short_folder, side_chat_command, source_message,
};

use zz_ui::agent::refs::{ChatRefs, open_path};
use zz_ui::command::{PaletteTarget, target_hosts};
use zz_ui::target::{RefTarget, ResolvedTarget};

use crate::command::palette::palette_backend;
use zz_ui::agent::controls::{agent_directory_chip, agent_settings_menu, agent_side_chat_button};

use super::{AgentPaneState, AgentView};

impl AgentView {
    pub(super) fn chat_refs(&self, view: &Entity<Self>, cx: &ala::App) -> ChatRefs {
        let view = view.downgrade();
        ChatRefs {
            hosts: target_hosts(&palette_backend(self.mux.clone()).tree(cx)),
            cwd: Some(self.chat_cwd()),
            open: Arc::new(move |target, window, cx| {
                view.update(cx, |view, cx| view.open_ref(target, window, cx))
                    .ok();
            }),
        }
    }

    fn open_ref(&mut self, target: &ResolvedTarget, _: &mut Window, cx: &mut Context<Self>) {
        let backend = palette_backend(self.mux.clone());
        let place = match &target.target {
            RefTarget::Host => {
                backend.activate_host(target.host, cx);
                return;
            }
            RefTarget::Session(id) => PaletteTarget::Session(*id),
            RefTarget::Window(id) => PaletteTarget::Window(*id),
            RefTarget::Pane(id) => PaletteTarget::Pane(*id),
            RefTarget::Path { .. } => {
                let anchor = target.here.then_some(self.pane);
                if let Some(command) = open_path(&target.target, anchor) {
                    backend.execute(target.host, command, cx);
                }
                return;
            }
        };
        backend.activate(target.host, place, cx);
    }

    pub(super) fn render_harness_controls(
        &self,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &mut ala::App,
    ) -> (Option<AnyElement>, Vec<AnyElement>) {
        let Some(settings) = state.settings.as_ref() else {
            return (None, Vec::new());
        };
        let local = self.mux.read(cx).attached_is_local();
        let directory = {
            let home = std::env::home_dir().filter(|_| local);
            let label = short_folder(
                &self.chat_cwd(),
                home.as_deref().and_then(|home| home.to_str()),
            );
            Some(
                agent_directory_chip(("agent-directory", self.pane.0), label, false, cx)
                    .into_any_element(),
            )
        };
        let pick = {
            let controller = self.controller.clone();
            let pane = self.pane;
            move |option: &'static str, value: String, _: &mut Window, cx: &mut ala::App| {
                controller.update(cx, |controller, cx| {
                    controller.set_option(pane, option, &value, cx);
                });
            }
        };
        let menu = agent_settings_menu(
            ("agent-settings", self.pane.0),
            settings,
            state.connection.accepts_setting(),
            pick,
            cx,
        );
        let side = {
            let view = view.clone();
            agent_side_chat_button(
                ("agent-side-chat", self.pane.0),
                state.session_id.as_deref().is_some_and(real_session),
                move |_, cx| view.update(cx, AgentView::open_side_chat),
            )
            .into_any_element()
        };
        (directory, vec![menu, side])
    }

    fn chat_cwd(&self) -> String {
        self.pane_state.cwd.to_string_lossy().into_owned()
    }

    fn session(&self) -> Option<&str> {
        self.pane_state
            .session_id
            .as_deref()
            .filter(|session| real_session(session))
    }

    fn open_side_chat(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let command = side_chat_command(
            self.pane_state.provider,
            session,
            &self.chat_cwd(),
            self.pane,
            self.pane_state.settings.as_ref(),
        );
        self.mux.read(cx).execute(command);
    }

    pub(super) fn fork_from(&mut self, item: &str, cx: &mut Context<Self>) {
        let (Some(message), Some(session)) = (source_message(item), self.session()) else {
            return;
        };
        let command = fork_chat_command(
            self.pane_state.provider,
            session,
            &self.chat_cwd(),
            Some(message),
            self.pane_state.settings.as_ref(),
        );
        self.mux.read(cx).execute(command);
    }

    pub(super) fn forks(&self) -> bool {
        self.pane_state
            .session_id
            .as_deref()
            .is_some_and(real_session)
            && self.pane_state.settings.is_some()
    }
}
