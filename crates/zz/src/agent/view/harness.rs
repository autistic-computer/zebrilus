use ala::{AnyElement, Context, Entity, IntoElement, Window};
use std::sync::Arc;
use zz_client::agent_settings::{
    self, fork_command, real_session, side_chat_command, source_message,
};

use zz_protocol::CommandInvocation;
use zz_ui::agent::refs::{ChatRefs, open_path};
use zz_ui::command::{PaletteTarget, target_hosts};
use zz_ui::target::{RefTarget, ResolvedTarget};

use crate::command::palette::palette_backend;
use zz_ui::agent::controls::{agent_directory_chip, agent_settings_menu, agent_side_chat_button};

use super::{AgentPaneState, AgentView};

const SIDE_CHAT_WIDTH: &str = "46%";
const SIDE_CHAT_HEIGHT: &str = "72%";

impl AgentView {
    pub(super) fn chat_refs(&self, view: &Entity<Self>, cx: &ala::App) -> ChatRefs {
        let view = view.downgrade();
        ChatRefs {
            hosts: target_hosts(&palette_backend(self.mux.clone()).tree(cx)),
            cwd: self
                .pane_state
                .settings
                .as_ref()
                .map(|settings| settings.cwd.clone())
                .filter(|cwd| !cwd.is_empty()),
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
        let enabled = state.connection.accepts_prompt();
        let local = self.mux.read(cx).attached_is_local();
        let directory = (!settings.cwd.is_empty()).then(|| {
            let home = std::env::home_dir().filter(|_| local);
            let label = settings.short_cwd(home.as_deref().and_then(|home| home.to_str()));
            let view = view.clone();
            agent_directory_chip(
                ("agent-directory", self.pane.0),
                label,
                enabled && local,
                cx,
            )
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| view.pick_directory(window, cx));
            })
            .into_any_element()
        });
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

    fn pick_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = cx.prompt_for_paths(ala::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Move this chat to".into()),
        });
        let controller = self.controller.clone();
        let pane = self.pane;
        cx.spawn_in(window, async move |_, window| {
            let path = selected.await.ok()?.ok()??.into_iter().next()?;
            let path = path.to_str()?.to_owned();
            window
                .update(|_, cx| {
                    controller.update(cx, |controller, cx| {
                        controller.set_option(pane, agent_settings::CWD, &path, cx);
                    });
                })
                .ok()
        })
        .detach();
    }

    fn chat_cwd(&self) -> String {
        self.pane_state
            .settings
            .as_ref()
            .map(|settings| settings.cwd.clone())
            .filter(|cwd| !cwd.is_empty())
            .unwrap_or_else(|| self.pane_state.cwd.to_string_lossy().into_owned())
    }

    fn open_side_chat(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self
            .pane_state
            .session_id
            .as_deref()
            .filter(|session| real_session(session))
        else {
            return;
        };
        let cwd = self.chat_cwd();
        let target = self.pane.to_string();
        let mut arguments = vec![
            "-t".to_owned(),
            target,
            "-c".to_owned(),
            cwd,
            "-x".to_owned(),
            SIDE_CHAT_WIDTH.to_owned(),
            "-y".to_owned(),
            SIDE_CHAT_HEIGHT.to_owned(),
            "-T".to_owned(),
            "Side chat".to_owned(),
        ];
        arguments.extend(side_chat_command(self.pane_state.provider, session));
        self.mux
            .read(cx)
            .execute(CommandInvocation::new("new-pane", arguments));
    }

    pub(super) fn fork_from(&mut self, item: &str, cx: &mut Context<Self>) {
        let Some(message) = source_message(item) else {
            return;
        };
        let Some(session) = self
            .pane_state
            .session_id
            .as_deref()
            .filter(|session| real_session(session))
        else {
            return;
        };
        let mut arguments = vec![
            "-c".to_owned(),
            self.chat_cwd(),
            "-n".to_owned(),
            "fork".to_owned(),
        ];
        arguments.extend(fork_command(self.pane_state.provider, session, message));
        self.mux
            .read(cx)
            .execute(CommandInvocation::new("new-window", arguments));
    }

    pub(super) fn forks(&self) -> bool {
        self.pane_state
            .session_id
            .as_deref()
            .is_some_and(real_session)
            && self.pane_state.settings.is_some()
    }
}
