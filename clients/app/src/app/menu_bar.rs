use ala::{Context, Menu, MenuItem, SharedString, Window};
use ala_platform::ios::MenuCommand;
use zz_client::{
    ChromeAction, UI_TABLE,
    navigation::{RenameTarget, kill_session_prompt_command, rename_prompt_command},
};
use zz_protocol::{CommandInvocation, SessionId};
use zz_terminal::TerminalViewAction;
use zz_ui::settings::about::{DOCS_URL, ISSUES_URL, RELEASES_URL};

use super::AppShell;

const SESSION: &str = "session:";
const TMUX: &str = "tmux:";
const URL: &str = "url:";
const RENAME_SESSION: &str = "rename-session";
const RENAME_WINDOW: &str = "rename-window";
const KILL_SESSION: &str = "kill-session";
const FIND: &str = "find";
const CLEAR_SCROLLBACK: &str = "clear-scrollback";
const BACK: &str = "back";
const FORWARD: &str = "forward";
const RELOAD: &str = "reload";

fn command(title: impl Into<SharedString>, id: impl Into<SharedString>) -> MenuItem {
    MenuItem::action(
        title,
        MenuCommand {
            id: id.into(),
            shortcut: None,
        },
    )
}

fn tmux(title: &'static str, command_line: &'static str) -> MenuItem {
    command(title, format!("{TMUX}{command_line}"))
}

fn link(title: &'static str, url: &'static str) -> MenuItem {
    command(title, format!("{URL}{url}"))
}

impl AppShell {
    pub(super) fn sync_menus(&mut self, cx: &mut Context<Self>) {
        let core = &self.connection.read(cx).core;
        let mut sessions = core
            .snapshot()
            .sessions
            .iter()
            .map(|session| (session.name.clone(), session.id))
            .collect::<Vec<_>>();
        sessions.sort();
        if self.menu_sessions.as_ref() != Some(&sessions) {
            self.menu_sessions = Some(sessions);
            cx.set_menus(self.menus());
        }
    }

    fn menus(&self) -> Vec<Menu> {
        let bindings = self.chrome.bindings();
        let chrome = |title: &'static str, action: ChromeAction| {
            MenuItem::action(
                title,
                MenuCommand {
                    id: action.name().into(),
                    shortcut: bindings
                        .iter()
                        .find(|(table, _, bound)| table == UI_TABLE && *bound == action)
                        .map(|(_, key, _)| key.clone().into())
                        .or_else(|| (action == ChromeAction::ClosePane).then(|| "D-w".into())),
                },
            )
        };
        let sessions = self.menu_sessions.as_deref().unwrap_or_default();
        vec![
            Menu::new("zz").items([chrome("Settings…", ChromeAction::OpenSettings)]),
            Menu::new("File").items([
                chrome("New Session", ChromeAction::NewSession),
                chrome("New Window", ChromeAction::NewWindow),
                chrome("Split Right", ChromeAction::SplitRight),
                chrome("Split Down", ChromeAction::SplitDown),
                tmux("New Browser Pane", "split-window --kind browser"),
                MenuItem::separator(),
                MenuItem::submenu(
                    Menu::new("Switch Session").items(
                        sessions
                            .iter()
                            .map(|(name, id)| command(name.clone(), format!("{SESSION}{id}"))),
                    ),
                ),
                command("Rename Session…", RENAME_SESSION),
                command("Rename Window…", RENAME_WINDOW),
                MenuItem::separator(),
                chrome("Close Pane", ChromeAction::ClosePane),
                tmux("Kill Window", "kill-window"),
                command("Kill Session…", KILL_SESSION),
                MenuItem::separator(),
                chrome("Detach", ChromeAction::Detach),
            ]),
            Menu::new("Edit").items([
                command("Find…", FIND),
                tmux("Copy Mode", "copy-mode"),
                command("Clear Scrollback", CLEAR_SCROLLBACK),
            ]),
            Menu::new("View").items([
                chrome("Command Palette…", ChromeAction::OpenCommandPalette),
                tmux("Choose Window…", "choose-tree -w"),
                chrome("Toggle Sidebar", ChromeAction::ToggleSidebar),
                MenuItem::separator(),
                tmux("Zoom Pane", "resize-pane -Z"),
                MenuItem::submenu(Menu::new("Select Pane").items([
                    tmux("Up", "select-pane -U"),
                    tmux("Down", "select-pane -D"),
                    tmux("Left", "select-pane -L"),
                    tmux("Right", "select-pane -R"),
                ])),
                MenuItem::separator(),
                chrome("Zoom In", ChromeAction::UiZoomIn),
                chrome("Zoom Out", ChromeAction::UiZoomOut),
                chrome("Reset Zoom", ChromeAction::UiZoomReset),
                MenuItem::separator(),
                command("Back", BACK),
                command("Forward", FORWARD),
                command("Reload", RELOAD),
            ]),
            Menu::new("Help").items([
                link("zz Documentation", DOCS_URL),
                link("Keyboard Shortcuts", DOCS_URL),
                link("Release Notes", RELEASES_URL),
                MenuItem::separator(),
                link("Report an Issue", ISSUES_URL),
            ]),
        ]
    }

    pub(super) fn menu_command(
        &mut self,
        command: &MenuCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = command.id.as_ref();
        if let Some(command) = id.strip_prefix(TMUX) {
            self.tmux_command(command, cx);
        } else if let Some(url) = id.strip_prefix(URL) {
            cx.open_url(url);
        } else if let Some(session) = id.strip_prefix(SESSION) {
            if let Ok(session) = session.parse::<SessionId>() {
                self.connection
                    .update(cx, |connection, cx| connection.attach(session, cx));
            }
        } else if let Some(action) = ChromeAction::from_name(id) {
            self.chrome_command(action, window, cx);
        } else {
            self.pane_menu_command(id, window, cx);
        }
    }

    fn pane_menu_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            RENAME_SESSION | RENAME_WINDOW | KILL_SESSION => {
                if let Some(command) = self.session_menu_command(id, cx) {
                    self.menu_invoke(command, cx);
                }
                return;
            }
            _ => {}
        }
        let Some(pane) = self.active_window(cx).map(|window| window.active_pane) else {
            return;
        };
        if let Some(terminal) = self.terminals.get(&pane).cloned() {
            match id {
                FIND => terminal.update(cx, |terminal, cx| terminal.open_search(window, cx)),
                CLEAR_SCROLLBACK => terminal.update(cx, |terminal, cx| {
                    terminal.view(TerminalViewAction::ClearHistory, cx);
                }),
                _ => {}
            }
        } else if let Some(browser) = self.browsers.get(&pane).cloned() {
            match id {
                BACK => browser.read(cx).go_back(),
                FORWARD => browser.read(cx).go_forward(),
                RELOAD => browser.update(cx, super::browser_pane::BrowserPane::reload_page),
                _ => {}
            }
        }
    }

    fn session_menu_command(&self, id: &str, cx: &Context<Self>) -> Option<CommandInvocation> {
        let core = &self.connection.read(cx).core;
        let attached = core.attached_session()?;
        let session = core
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.id == attached)?;
        Some(match id {
            RENAME_SESSION => {
                rename_prompt_command(RenameTarget::Session(session.id), &session.name).1
            }
            RENAME_WINDOW => {
                let window = self.active_window(cx)?;
                rename_prompt_command(RenameTarget::Window(window.id), &window.name).1
            }
            _ => kill_session_prompt_command(session.id, &session.name),
        })
    }

    fn menu_invoke(&self, command: CommandInvocation, cx: &mut Context<Self>) {
        let connection = self.connection.read(cx);
        if connection.connected && !connection.core.attached_read_only() {
            self.invoke(command, cx);
        }
    }
}
