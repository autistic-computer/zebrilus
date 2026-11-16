use crate::config::settings::OpenSettings;
use crate::mux::client::MuxClient;
use crate::mux::prefix::display_keystroke;
use ala::{
    App, Context, FocusHandle, Focusable, KeyDownEvent, Keystroke, MouseButton, Render, Window,
    prelude::*,
};
use penna::components::button::{Button, ButtonVariants as _};
use zz_client::{ChromeAction, UI_TABLE};
use zz_ui::shell::{
    empty_workspace, empty_workspace_action, empty_workspace_connect, empty_workspace_keys,
    empty_workspace_prefix_hints,
};

const DEFAULT_PREFIX: &str = "C-b";

struct Hint {
    key: &'static str,
    label: &'static str,
}

const ACTIONS: [Hint; 2] = [
    Hint {
        key: "enter",
        label: "New session",
    },
    Hint {
        key: crate::config::settings::KEYBIND,
        label: "Settings",
    },
];

fn activates_new_session(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    !modifiers.control && !modifiers.alt && !modifiers.platform && keystroke.key.as_str() == "enter"
}

pub(crate) struct NewSessionView {
    mux: ala::Entity<MuxClient>,
    focus_handle: FocusHandle,
}

impl NewSessionView {
    pub(crate) fn new(mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self {
            mux,
            focus_handle: cx.focus_handle(),
        }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus_handle
    }

    fn activate(&self, cx: &App) {
        if let Some(host) = self.mux.read(cx).first_host() {
            self.mux.read(cx).new_session(host);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if activates_new_session(&event.keystroke) {
            self.activate(cx);
            cx.stop_propagation();
        }
    }
}

impl Focusable for NewSessionView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NewSessionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        if !self.mux.read(cx).has_hosts() {
            return empty_workspace(
                "new-session-empty-state",
                empty_workspace_connect(
                    Button::new("new-session-add-host")
                        .primary()
                        .label("Add host…")
                        .on_click(|_, window, cx| {
                            super::add_host::open(window, cx);
                            cx.stop_propagation();
                        }),
                    cx,
                ),
                cx,
            )
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_handle.focus(window, cx);
            })
            .into_any_element();
        }
        let view = cx.entity();
        let prefix = self
            .mux
            .read(cx)
            .canonical_prefix()
            .unwrap_or_else(|| DEFAULT_PREFIX.to_owned());
        let prefix = display_keystroke(&prefix);
        let mut actions = ACTIONS.iter();
        let new_session = actions.next().expect("the new-session hint");
        let settings = actions.next().expect("the settings hint");
        let settings_key = crate::keymap::chord_for(cx, UI_TABLE, ChromeAction::OpenSettings);
        let activate = empty_workspace_action(
            "new-session-activate",
            Keystroke::parse(new_session.key).ok(),
            new_session.label,
            true,
            self.focus_handle.is_focused(window),
            cx,
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            view.update(cx, |view, cx| view.activate(cx));
            cx.stop_propagation();
        });
        let open_settings = empty_workspace_action(
            "new-session-settings",
            Keystroke::parse(settings_key.as_deref().unwrap_or(settings.key)).ok(),
            settings.label,
            false,
            false,
            cx,
        )
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(OpenSettings), cx);
        });
        let hints = {
            let mux = self.mux.read(cx);
            empty_workspace_prefix_hints(
                prefix.as_ref(),
                mux.prefix_bindings(),
                display_keystroke,
                cx,
            )
        };
        empty_workspace(
            "new-session-empty-state",
            empty_workspace_keys(
                [
                    activate.into_any_element(),
                    open_settings.into_any_element(),
                ]
                .into_iter()
                .chain(hints),
            ),
            cx,
        )
        .track_focus(&self.focus_handle)
        .on_key_down(cx.listener(Self::on_key_down))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus_handle.focus(window, cx);
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use ala::{Modifiers, TestAppContext, VisualTestContext};
    use zz_daemon_client::DaemonError;

    use super::*;

    fn key(value: &str) -> Keystroke {
        Keystroke {
            modifiers: Modifiers::default(),
            key: value.to_owned(),
            key_char: Some(value.to_owned()),
        }
    }

    #[test]
    fn enter_activates_the_empty_session_card() {
        assert!(activates_new_session(&key("enter")));
        assert!(!activates_new_session(&key("space")));
        let mut modified = key("enter");
        modified.modifiers.platform = true;
        assert!(!activates_new_session(&modified));
    }

    #[test]
    fn every_hint_spells_a_key_ala_can_parse() {
        for hint in &ACTIONS {
            assert!(
                Keystroke::parse(hint.key).is_ok(),
                "`{}` ({})",
                hint.key,
                hint.label
            );
        }
        for hint in &zz_client::prefix_keys::PREFIX_HINTS {
            assert!(
                display_keystroke(hint.key).is_some(),
                "`{}` ({})",
                hint.key,
                hint.label
            );
        }
        assert!(crate::mux::prefix::display_keystroke(DEFAULT_PREFIX).is_some());
    }

    #[ala::test]
    fn the_empty_state_draws_without_a_daemon(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            NewSessionView::new(mux, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
}
