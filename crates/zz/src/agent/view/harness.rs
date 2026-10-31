use ala::{
    AnyElement, AnyView, Context, Entity, IntoElement, KeyDownEvent, SharedString, StyleRefinement,
    Window, div, prelude::*,
};
use zz_client::agent_settings::{
    self, fork_command, real_session, side_chat_command, source_message,
};
use zz_protocol::CommandInvocation;
use zz_ui::agent::controls::{agent_directory_chip, agent_settings_menu, agent_side_chat_button};
use zz_ui::agent::presentation::terminal_sheet;

use super::{AgentPaneState, AgentView};
use crate::terminal::view::TerminalView;

const SIDE_CHAT_WIDTH: &str = "46%";
const SIDE_CHAT_HEIGHT: &str = "72%";

pub(super) struct Sheet {
    title: SharedString,
    seen: u64,
    request: Option<u64>,
}

impl AgentView {
    pub(crate) fn set_terminal(&mut self, terminal: Entity<TerminalView>) {
        self.terminal = Some(terminal);
    }

    pub(super) fn open_sheet(
        &mut self,
        command: &str,
        request: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(terminal) = self.terminal.clone() else {
            return false;
        };
        let agent = self.pane_state.provider.label();
        let title = if command.is_empty() {
            agent.to_owned()
        } else {
            format!("{command} · {agent}")
        };
        self.sheet = Some(Sheet {
            title: title.into(),
            seen: self.pane_state.dialogs_closed,
            request,
        });
        terminal.read(cx).focus().focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn close_sheet(
        &mut self,
        escape: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet.take().is_none() {
            return;
        }
        if escape {
            let pane = self.pane;
            self.controller
                .update(cx, |controller, cx| controller.cancel(pane, cx));
        }
        self.focus(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn follow_dialogs(&mut self) -> bool {
        let state = &self.pane_state;
        if self.sheet.as_ref().is_some_and(|sheet| {
            state.dialogs_closed > sheet.seen
                || sheet.request.is_some_and(|request| {
                    !state
                        .pending_permissions
                        .iter()
                        .any(|pending| pending.request_id == request)
                })
        }) {
            self.sheet = None;
            self.refocus = true;
            return true;
        }
        false
    }

    pub(super) fn sheet_open(&self) -> bool {
        self.sheet.is_some()
    }

    pub(super) fn render_sheet(
        &mut self,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if std::mem::take(&mut self.refocus) {
            self.focus(cx).focus(window, cx);
        }
        let sheet = self.sheet.as_ref()?;
        let terminal = self.terminal.clone()?;
        let body = AnyView::from(terminal)
            .cached(StyleRefinement::default().size_full())
            .into_any_element();
        let done = {
            let view = view.clone();
            move |window: &mut Window, cx: &mut ala::App| {
                view.update(cx, |view, cx| view.close_sheet(true, window, cx));
            }
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                    let modifiers = event.keystroke.modifiers;
                    if view.pane_state.provider == zz_protocol::AgentProvider::Codex
                        && event.keystroke.key == "escape"
                        && !modifiers.platform
                        && !modifiers.control
                        && !modifiers.alt
                        && !modifiers.shift
                    {
                        view.close_sheet(false, window, cx);
                    }
                }))
                .child(terminal_sheet(
                    ("agent-terminal-sheet", self.pane.0),
                    sheet.title.clone(),
                    body,
                    done,
                    cx,
                ))
                .into_any_element(),
        )
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
        let terminal = {
            let view = view.clone();
            let command = settings
                .terminal
                .clone()
                .unwrap_or_else(|| "/model".to_owned());
            move |window: &mut Window, cx: &mut ala::App| {
                view.update(cx, |view, cx| {
                    view.run_terminal_command(&command, window, cx);
                });
            }
        };
        let menu = agent_settings_menu(
            ("agent-settings", self.pane.0),
            settings,
            enabled,
            pick,
            terminal,
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

    fn run_terminal_command(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane;
        let result = self.controller.update(cx, |controller, cx| {
            controller.prompt(pane, command, Vec::new(), cx)
        });
        match result {
            Ok(()) => {
                if !self.open_sheet(command, None, window, cx) {
                    cx.emit(super::ShowTerminal);
                }
            }
            Err(error) => {
                self.submission_error = Some(error);
                cx.notify();
            }
        }
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
