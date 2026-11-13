use ala::{AnyElement, Context, Entity, Window};
use std::sync::Arc;
use zz_client::agent_settings::{self, fork_chat_command, real_session, source_message};

use zz_ui::agent::preview::ViewRequest;
use zz_ui::agent::refs::{ChatRefs, open_path};
use zz_ui::command::{PaletteTarget, target_hosts};
use zz_ui::target::{RefTarget, ResolvedTarget};

use crate::command::palette::palette_backend;
use zz_ui::agent::controls::{
    AccountAction, ContextUsage, UsagePill, agent_model_picker, agent_usage_pill,
};

use super::{AgentAccountPicked, AgentPaneState, AgentView};

impl AgentView {
    pub(super) fn chat_refs(&self, view: &Entity<Self>, cx: &ala::App) -> ChatRefs {
        let view = view.downgrade();
        let shows = view.clone();
        ChatRefs {
            hosts: target_hosts(&palette_backend(self.mux.clone()).tree(cx)),
            cwd: Some(self.chat_cwd()),
            open: Arc::new(move |target, window, cx| {
                view.update(cx, |view, cx| view.open_ref(target, window, cx))
                    .ok();
            }),
            view: Some(Arc::new(move |request, window, cx| {
                shows
                    .update(cx, |view, cx| view.open_view(&request, window, cx))
                    .ok();
            })),
        }
    }

    pub(super) fn open_view(&self, request: &ViewRequest, window: &Window, cx: &mut Context<Self>) {
        let mux = self.mux.read(cx);
        let rows = crate::workspace::pip_fill_rows(
            mux,
            self.pane,
            self.pane_canvas.get().size,
            window,
            cx,
        );
        mux.execute(request.command(self.pane, rows));
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
    ) -> Vec<AnyElement> {
        let enabled = state.connection.accepts_setting();
        let context = ContextUsage::reported(state.usage);
        let context = if self
            .controller
            .read(cx)
            .pane_chat(self.pane)
            .is_some_and(zz_ui::agent::chat::shows_chat)
        {
            context
        } else {
            context.in_empty_chat()
        };
        let account = agent_usage_pill(
            ("agent-account", self.pane.0),
            &UsagePill {
                provider: state.provider,
                account: self.account.as_deref(),
                accounts: self.accounts.as_deref(),
                quotas: &self.quotas,
                context,
                enabled,
            },
            {
                let mux = self.mux.clone();
                let view = view.downgrade();
                let provider = state.provider;
                move |action, _, cx| match action {
                    AccountAction::Refresh => {
                        let mux = mux.read(cx);
                        mux.request_agent_accounts(mux.attached_host());
                    }
                    AccountAction::Add => {
                        let mux = mux.read(cx);
                        mux.add_agent_account(mux.attached_host(), provider);
                    }
                    AccountAction::Use(account) => {
                        view.update(cx, |_, cx| cx.emit(AgentAccountPicked { account }))
                            .ok();
                    }
                }
            },
            cx,
        );
        let Some(settings) = state.settings.as_ref() else {
            return vec![account];
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
        let picker = agent_model_picker(
            ("agent-settings", self.pane.0),
            Some(state.provider),
            settings,
            enabled,
            pick,
            cx,
        );
        vec![account, picker]
    }

    pub(super) fn chat_cwd(&self) -> String {
        self.pane_state.cwd.to_string_lossy().into_owned()
    }

    fn session(&self) -> Option<&str> {
        self.pane_state
            .session_id
            .as_deref()
            .filter(|session| real_session(session))
    }

    pub(super) fn fork_from(&mut self, item: &str, cx: &mut Context<Self>) {
        let (Some(message), Some(session)) = (source_message(item), self.session()) else {
            return;
        };
        let snapshot = self.mux.read(cx).snapshot();
        let command = fork_chat_command(
            self.pane_state.provider,
            session,
            &self.chat_cwd(),
            self.pane,
            Some(message),
            self.pane_state.settings.as_ref(),
            agent_settings::pane_account(&snapshot, self.pane),
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
