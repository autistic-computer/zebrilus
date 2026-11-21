use ala::{Context, IntoElement, SharedString, WeakEntity, Window};
use penna::{
    Ground, IconName,
    blocks::sidebar::chrome_button,
    components::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
};
use zz_client::{
    ChromeAction,
    navigation::{RenameTarget, kill_session_prompt_command, rename_prompt_command},
};
use zz_protocol::{CommandInvocation, SessionId};
use zz_terminal::TerminalViewAction;
use zz_ui::settings::{
    SettingsSection,
    about::{DOCS_URL, ISSUES_URL, RELEASES_URL, ZZ_ABOUT},
};

use super::AppShell;

#[derive(Clone)]
struct MenuState {
    connected: bool,
    writable: bool,
    has_terminal: bool,
    sessions: Vec<(String, SessionId)>,
}

impl MenuState {
    fn read(shell: &WeakEntity<AppShell>, cx: &ala::App) -> Self {
        let Some(shell) = shell.upgrade() else {
            return Self {
                connected: false,
                writable: false,
                has_terminal: false,
                sessions: Vec::new(),
            };
        };
        let app = shell.read(cx);
        let connection = app.connection.read(cx);
        let connected = connection.connected;
        let writable = connected && !connection.core.attached_read_only();
        let has_terminal = app
            .active_window(cx)
            .is_some_and(|window| app.terminals.contains_key(&window.active_pane));
        let mut sessions = connection
            .core
            .snapshot()
            .sessions
            .iter()
            .map(|session| (session.name.clone(), session.id))
            .collect::<Vec<_>>();
        sessions.sort();
        Self {
            connected,
            writable,
            has_terminal,
            sessions,
        }
    }
}

pub(super) fn app_menu_button(shell: WeakEntity<AppShell>, ground: Ground) -> impl IntoElement {
    chrome_button("web-app-menu", IconName::Menu, "App menu")
        .on(ground)
        .dropdown_menu(move |popup, window, cx| build_menu(popup, &shell, window, cx))
}

fn build_menu(
    popup: PopupMenu,
    shell: &WeakEntity<AppShell>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let state = MenuState::read(shell, cx);
    let app_name = ZZ_ABOUT.name;
    let s_app = shell.clone();
    let s_file = shell.clone();
    let s_edit = shell.clone();
    let s_view = shell.clone();
    let file_state = state.clone();
    let edit_state = state.clone();
    let view_state = state;

    popup
        .submenu_with_icon(None, app_name, window, cx, move |menu, _, _| {
            menu.item(open_settings_item(
                format!("About {app_name}"),
                SettingsSection::About,
                &s_app,
            ))
            .item(open_settings_item(
                "Settings…",
                SettingsSection::Appearance,
                &s_app,
            ))
        })
        .submenu_with_icon(None, "File", window, cx, move |menu, window, cx| {
            file_menu(menu, &s_file, &file_state, window, cx)
        })
        .submenu_with_icon(None, "Edit", window, cx, move |menu, _, _| {
            edit_menu(menu, &s_edit, &edit_state)
        })
        .submenu_with_icon(None, "View", window, cx, move |menu, window, cx| {
            view_menu(menu, &s_view, &view_state, window, cx)
        })
        .submenu_with_icon(None, "Help", window, cx, move |menu, _, _| {
            help_menu(menu, app_name)
        })
}

fn file_menu(
    menu: PopupMenu,
    shell: &WeakEntity<AppShell>,
    state: &MenuState,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let writable = state.writable;
    let sessions = state.sessions.clone();
    let s_switch = shell.clone();
    let switch_disabled = !state.connected || sessions.is_empty();

    let menu = menu
        .item(chrome_item(
            "New Session",
            ChromeAction::NewSession,
            !writable,
            shell,
        ))
        .item(chrome_item(
            "New Window",
            ChromeAction::NewWindow,
            !writable,
            shell,
        ))
        .item(chrome_item(
            "Split Right",
            ChromeAction::SplitRight,
            !writable,
            shell,
        ))
        .item(chrome_item(
            "Split Down",
            ChromeAction::SplitDown,
            !writable,
            shell,
        ))
        .item(tmux_item(
            "New Browser Pane",
            "split-window --kind browser",
            !writable,
            shell,
        ))
        .separator();

    let menu = if switch_disabled {
        menu.item(PopupMenuItem::new("Switch Session").disabled(true))
    } else {
        menu.submenu_with_icon(None, "Switch Session", window, cx, move |mut sub, _, _| {
            for (name, id) in &sessions {
                let s = s_switch.clone();
                let id = *id;
                let name = name.clone();
                sub = sub.item(PopupMenuItem::new(name.clone()).on_click(move |_, _, cx| {
                    let _ = s.update(cx, |app, cx| app.attach_named_session(id, &name, cx));
                }));
            }
            sub
        })
    };

    menu.item(session_prompt_item(
        "Rename Session…",
        SessionPrompt::RenameSession,
        !writable,
        shell,
    ))
    .item(session_prompt_item(
        "Rename Window…",
        SessionPrompt::RenameWindow,
        !writable,
        shell,
    ))
    .separator()
    .item(chrome_item(
        "Close Pane",
        ChromeAction::ClosePane,
        !writable,
        shell,
    ))
    .item(tmux_item("Kill Window", "kill-window", !writable, shell))
    .item(session_prompt_item(
        "Kill Session…",
        SessionPrompt::KillSession,
        !writable,
        shell,
    ))
    .separator()
    .item(chrome_item(
        "Detach",
        ChromeAction::Detach,
        !state.connected,
        shell,
    ))
}

fn edit_menu(menu: PopupMenu, shell: &WeakEntity<AppShell>, state: &MenuState) -> PopupMenu {
    let s_find = shell.clone();
    let s_clear = shell.clone();
    menu.item(
        PopupMenuItem::new("Find…")
            .disabled(!state.has_terminal)
            .on_click(move |_, window, cx| {
                let _ = s_find.update(cx, |app, cx| app.open_active_terminal_search(window, cx));
            }),
    )
    .item(tmux_item("Copy Mode", "copy-mode", !state.writable, shell))
    .item(
        PopupMenuItem::new("Clear Scrollback")
            .disabled(!state.has_terminal)
            .on_click(move |_, _, cx| {
                let _ = s_clear.update(cx, AppShell::clear_active_terminal_scrollback);
            }),
    )
}

fn view_menu(
    menu: PopupMenu,
    shell: &WeakEntity<AppShell>,
    state: &MenuState,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let writable = state.writable;
    let s_select = shell.clone();
    menu.item(chrome_item(
        "Command Palette…",
        ChromeAction::OpenCommandPalette,
        false,
        shell,
    ))
    .item(tmux_item(
        "Choose Window…",
        "choose-tree -w",
        !writable,
        shell,
    ))
    .separator()
    .item(chrome_item(
        "Toggle Sidebar",
        ChromeAction::ToggleSidebar,
        false,
        shell,
    ))
    .separator()
    .item(tmux_item("Zoom Pane", "resize-pane -Z", !writable, shell))
    .submenu_with_icon(None, "Select Pane", window, cx, move |mut sub, _, _| {
        for (label, command) in [
            ("Up", "select-pane -U"),
            ("Down", "select-pane -D"),
            ("Left", "select-pane -L"),
            ("Right", "select-pane -R"),
        ] {
            sub = sub.item(tmux_item(label, command, !writable, &s_select));
        }
        sub
    })
    .separator()
    .item(chrome_item("Zoom In", ChromeAction::UiZoomIn, false, shell))
    .item(chrome_item(
        "Zoom Out",
        ChromeAction::UiZoomOut,
        false,
        shell,
    ))
    .item(chrome_item(
        "Reset Zoom",
        ChromeAction::UiZoomReset,
        false,
        shell,
    ))
}

fn help_menu(menu: PopupMenu, app_name: &str) -> PopupMenu {
    menu.item(url_item(format!("{app_name} Documentation"), DOCS_URL))
        .item(url_item("Keyboard Shortcuts", DOCS_URL))
        .item(url_item("Release Notes", RELEASES_URL))
        .separator()
        .item(url_item("Report an Issue", ISSUES_URL))
}

fn open_settings_item(
    label: impl Into<SharedString>,
    section: SettingsSection,
    shell: &WeakEntity<AppShell>,
) -> PopupMenuItem {
    let shell = shell.clone();
    PopupMenuItem::new(label).on_click(move |_, _, cx| {
        let _ = shell.update(cx, |app, cx| {
            app.settings = Some(section);
            app.focused_pane = None;
            cx.notify();
        });
    })
}

fn chrome_item(
    label: &'static str,
    action: ChromeAction,
    disabled: bool,
    shell: &WeakEntity<AppShell>,
) -> PopupMenuItem {
    let shell = shell.clone();
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            let _ = shell.update(cx, |app, cx| app.chrome_command(action, window, cx));
        })
}

fn tmux_item(
    label: &'static str,
    command: &'static str,
    disabled: bool,
    shell: &WeakEntity<AppShell>,
) -> PopupMenuItem {
    let shell = shell.clone();
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, _, cx| {
            let _ = shell.update(cx, |app, cx| app.tmux_command(command, cx));
        })
}

fn url_item(label: impl Into<SharedString>, url: &'static str) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, _, cx| cx.open_url(url))
}

#[derive(Clone, Copy)]
enum SessionPrompt {
    RenameSession,
    RenameWindow,
    KillSession,
}

fn session_prompt_item(
    label: &'static str,
    kind: SessionPrompt,
    disabled: bool,
    shell: &WeakEntity<AppShell>,
) -> PopupMenuItem {
    let shell = shell.clone();
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, _, cx| {
            let _ = shell.update(cx, |app, cx| app.run_session_prompt(kind, cx));
        })
}

impl AppShell {
    fn attach_named_session(&self, id: SessionId, name: &str, cx: &mut Context<Self>) {
        let listed = self
            .connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .any(|session| session.id == id && session.name == name);
        if listed {
            self.connection
                .update(cx, |connection, cx| connection.attach(id, cx));
        }
    }

    fn open_active_terminal_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_window(cx).map(|window| window.active_pane)
            && let Some(terminal) = self.terminals.get(&pane).cloned()
        {
            terminal.update(cx, |terminal, cx| terminal.open_search(window, cx));
        }
    }

    fn clear_active_terminal_scrollback(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_window(cx).map(|window| window.active_pane)
            && let Some(terminal) = self.terminals.get(&pane).cloned()
        {
            terminal.update(cx, |terminal, cx| {
                terminal.view(TerminalViewAction::ClearHistory, cx);
            });
        }
    }

    fn run_session_prompt(&self, kind: SessionPrompt, cx: &mut Context<Self>) {
        let Some(command) = self.session_prompt_command(kind, cx) else {
            return;
        };
        let connection = self.connection.read(cx);
        if connection.connected && !connection.core.attached_read_only() {
            self.invoke(command, cx);
        }
    }

    fn session_prompt_command(
        &self,
        kind: SessionPrompt,
        cx: &Context<Self>,
    ) -> Option<CommandInvocation> {
        let core = &self.connection.read(cx).core;
        let attached = core.attached_session()?;
        let session = core
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.id == attached)?;
        Some(match kind {
            SessionPrompt::RenameSession => {
                rename_prompt_command(RenameTarget::Session(session.id), &session.name).1
            }
            SessionPrompt::RenameWindow => {
                let window = self.active_window(cx)?;
                rename_prompt_command(RenameTarget::Window(window.id), &window.name).1
            }
            SessionPrompt::KillSession => kill_session_prompt_command(session.id, &session.name),
        })
    }
}
