use ala::{AnyElement, App, Context, IntoElement, Keystroke, MouseButton, Window, prelude::*, px};
use penna::Root;
use zz_client::{ChromeAction, ClientCore, UI_TABLE};
use zz_ui::{
    command::tmux_keystroke,
    pane::PaneActionKeys,
    settings::SettingsSection,
    shell::{
        empty_workspace, empty_workspace_action, empty_workspace_keys, empty_workspace_prefix_hints,
    },
    which_key::{SheetKeys, WhichKeySheet},
};

use super::AppShell;

const DEFAULT_PREFIX: &str = "C-b";

fn sheet_keys(core: &ClientCore) -> SheetKeys<'_> {
    SheetKeys {
        tables: core.key_tables(),
        prefix_bindings: core.prefix_bindings(),
        prefix: core.canonical_prefix(),
        display: tmux_keystroke,
    }
}

fn activates_empty_workspace(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
        && keystroke.key == "enter"
}

impl AppShell {
    pub(super) fn pane_action_keys(&self, cx: &App) -> PaneActionKeys {
        let core = &self.connection.read(cx).core;
        PaneActionKeys::bound(
            core.canonical_prefix().as_deref(),
            core.prefix_bindings(),
            tmux_keystroke,
        )
    }

    pub(super) fn chrome_keystroke(&self, wanted: ChromeAction) -> Option<Keystroke> {
        let (key, _) = self
            .chrome
            .native
            .table_bindings(UI_TABLE)
            .into_iter()
            .find(|(_, action)| *action == wanted)?;
        Some(Keystroke {
            modifiers: ala::Modifiers {
                platform: key.command,
                control: key.control,
                alt: key.alt,
                shift: key.shift,
                function: false,
            },
            key: key.base,
            key_char: None,
        })
    }

    fn which_key_delay(&self) -> Option<std::time::Duration> {
        zz_client::prefix_keys::which_key_delay(self.preferences.which_key_delay)
    }

    fn workspace_covered(&self, window: &Window, cx: &App) -> bool {
        let core = &self.connection.read(cx).core;
        Self::narrow(window)
            || self.settings.is_some()
            || self.prompt.is_some()
            || self.pane_drag.is_some()
            || cx.has_active_drag()
            || core.command_prompt().is_some()
            || core.command_output().is_some()
            || core.menu().is_some()
            || core.confirm().is_some()
            || core.choose_tree().is_some()
            || core.choose_buffer().is_some()
            || core.display_panes().is_some()
            || window
                .root::<Root>()
                .flatten()
                .is_some_and(|root| root.read(cx).has_active_dialog())
    }

    pub(super) fn hide_which_key(&mut self, cx: &mut Context<Self>) {
        self.which_key_timer = None;
        if self.which_key.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn key_table_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_which_key(cx);
        let Some((table, false)) = self.connection.read(cx).core.key_table() else {
            return;
        };
        let table = table.to_owned();
        let Some(delay) = self.which_key_delay() else {
            return;
        };
        self.which_key_timer = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(delay).await;
            view.update_in(cx, |view, window, cx| {
                view.show_which_key(&table, window, cx);
            })
            .ok();
        }));
    }

    fn show_which_key(&mut self, table: &str, window: &Window, cx: &mut Context<Self>) {
        self.which_key_timer = None;
        let core = &self.connection.read(cx).core;
        if core.key_table() != Some((table, false)) || self.workspace_covered(window, cx) {
            return;
        }
        self.which_key = WhichKeySheet::build(sheet_keys(core), table, false);
        cx.notify();
    }

    pub(super) fn show_all_keys(
        &mut self,
        input: &zz_terminal::KeyInput,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let core = &self.connection.read(cx).core;
        if !core.prefix_armed()
            || self.which_key_delay().is_none()
            || self.workspace_covered(window, cx)
            || !zz_client::prefix_keys::is_all_keys_input(core.prefix_bindings(), input)
        {
            return false;
        }
        let Some(sheet) = WhichKeySheet::build(sheet_keys(core), "prefix", true) else {
            return false;
        };
        self.which_key = Some(sheet);
        cx.notify();
        true
    }

    pub(super) fn which_key_overlay(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if (self.which_key.is_some() || self.which_key_timer.is_some())
            && self.workspace_covered(window, cx)
        {
            self.which_key_timer = None;
            self.which_key = None;
        }
        let sheet = self.which_key.as_ref()?;
        let canvas = self.pane_canvas_bounds.get().size;
        let canvas = if canvas.width > px(0.) && canvas.height > px(0.) {
            canvas
        } else {
            window.viewport_size()
        };
        Some(sheet.element(px(0.), px(0.), canvas))
    }

    pub(super) fn empty_workspace_key(
        &self,
        event: &ala::KeyDownEvent,
        window: &Window,
        cx: &App,
    ) -> bool {
        let landing = self.preferences.flat_sidebar && self.flat.place.landing();
        !landing
            && !self.workspace_covered(window, cx)
            && !self.sidebar_focus.is_focused(window)
            && self.connection.read(cx).connected
            && self.active_window(cx).is_none()
            && activates_empty_workspace(&event.keystroke)
    }

    pub(super) fn open_empty_workspace(&self, cx: &mut Context<Self>) {
        let first = self
            .connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .first()
            .map(|session| session.id);
        if let Some(session) = first {
            self.connection
                .update(cx, |connection, cx| connection.attach(session, cx));
        } else {
            self.command("new-session", Vec::new(), cx);
        }
    }

    pub(super) fn empty_workspace(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let core = &self.connection.read(cx).core;
        let label = if core.snapshot().sessions.is_empty() {
            "New session"
        } else {
            "Attach to session"
        };
        let prefix = tmux_keystroke(
            &core
                .canonical_prefix()
                .unwrap_or_else(|| DEFAULT_PREFIX.to_owned()),
        );
        let hints = empty_workspace_prefix_hints(
            prefix.as_ref(),
            core.prefix_bindings(),
            tmux_keystroke,
            cx,
        );
        let activate = empty_workspace_action(
            "web-empty-activate",
            Keystroke::parse("enter").ok(),
            label,
            true,
            !self.sidebar_focus.is_focused(window),
            cx,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.open_empty_workspace(cx);
                cx.stop_propagation();
            }),
        );
        let settings = empty_workspace_action(
            "web-empty-settings",
            self.chrome_keystroke(ChromeAction::OpenSettings),
            "Settings",
            false,
            false,
            cx,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.settings = Some(SettingsSection::Appearance);
            cx.notify();
        }));
        empty_workspace(
            "web-empty-state",
            empty_workspace_keys(
                [activate.into_any_element(), settings.into_any_element()]
                    .into_iter()
                    .chain(hints),
            ),
            cx,
        )
        .into_any_element()
    }
}
