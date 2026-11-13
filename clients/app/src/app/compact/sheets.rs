use ala::{AnyElement, AppContext as _, Context, Entity, IntoElement as _, SharedString, Window};
use penna::{
    IconName,
    blocks::sheet::{close_sheet, floating_sheet},
    blocks::touch::TouchRow,
    components::input::{InputEvent, InputState, SelectAll},
};
use zz_protocol::{PaneId, SessionId, WindowId};
use zz_ui::compact::{Confirm, LinkAction, Rename, hold_sheet, link_sheet};

use super::super::AppShell;
use crate::connection::Connection;

pub(in super::super) const HOLD_SHEET: &str = "phone-hold-sheet";
pub(in super::super) const RENAME_SHEET: &str = "phone-rename-sheet";
pub(in super::super) const CONFIRM_SHEET: &str = "phone-confirm-sheet";
pub(in super::super) const LINK_SHEET: &str = "phone-link-sheet";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Target {
    Pane(PaneId),
    Window(WindowId),
    Session(SessionId),
}

impl Target {
    fn noun(self) -> &'static str {
        match self {
            Self::Pane(_) => "pane",
            Self::Window(_) => "window",
            Self::Session(_) => "session",
        }
    }

    fn rename(self, name: &str) -> (&'static str, Vec<String>) {
        match self {
            Self::Pane(pane) => (
                "select-pane",
                vec!["-t".into(), pane.to_string(), "-T".into(), name.to_owned()],
            ),
            Self::Window(window) => (
                "rename-window",
                vec!["-t".into(), window.to_string(), name.to_owned()],
            ),
            Self::Session(session) => (
                "rename-session",
                vec!["-t".into(), session.to_string(), name.to_owned()],
            ),
        }
    }

    fn close(self) -> (&'static str, Vec<String>) {
        match self {
            Self::Pane(pane) => ("kill-pane", vec!["-t".into(), pane.to_string()]),
            Self::Window(window) => ("kill-window", vec!["-t".into(), window.to_string()]),
            Self::Session(session) => ("kill-session", vec!["-t".into(), session.to_string()]),
        }
    }
}

#[derive(Clone)]
pub(in super::super) struct Thing {
    pub connection: Entity<Connection>,
    pub target: Target,
    pub name: String,
    pub closing: Option<String>,
}

#[derive(Clone)]
pub(in super::super) enum PhoneSheet {
    Hold(Thing),
    Rename(Thing),
    Close(Thing),
    Link(String),
}

impl PhoneSheet {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(in super::super) fn id(&self) -> &'static str {
        match self {
            Self::Hold(_) => HOLD_SHEET,
            Self::Rename(_) => RENAME_SHEET,
            Self::Close(_) => CONFIRM_SHEET,
            Self::Link(_) => LINK_SHEET,
        }
    }

    fn thing(&self) -> Option<&Thing> {
        match self {
            Self::Hold(thing) | Self::Rename(thing) | Self::Close(thing) => Some(thing),
            Self::Link(_) => None,
        }
    }
}

fn rename_form(thing: &Thing) -> Rename {
    Rename {
        id: RENAME_SHEET.into(),
        title: format!("Rename {}", thing.target.noun()).into(),
        current: thing.name.clone().into(),
    }
}

fn run(connection: &Entity<Connection>, (command, args): (&str, Vec<String>), cx: &mut ala::App) {
    connection.update(cx, |connection, cx| connection.command(command, args, cx));
}

impl AppShell {
    pub(in super::super) fn open_phone_sheet(
        &mut self,
        sheet: PhoneSheet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let PhoneSheet::Rename(thing) = &sheet {
            let input = self.rename_input(window, cx);
            let name = thing.name.clone();
            input.update(cx, |input, cx| input.set_value(name, window, cx));
            cx.on_next_frame(window, move |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
                window.request_virtual_keyboard();
                cx.on_next_frame(window, |_, window, cx| {
                    window.dispatch_action(Box::new(SelectAll), cx);
                });
            });
        }
        self.compact.sheet = Some(sheet);
        cx.notify();
    }

    fn rename_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some((input, _)) = &self.compact.rename {
            return input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.save_rename(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            });
        self.compact.rename = Some((input.clone(), subscription));
        input
    }

    fn save_rename(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (Some(PhoneSheet::Rename(thing)), Some((input, _))) =
            (&self.compact.sheet, &self.compact.rename)
        else {
            return;
        };
        let typed = input.read(cx).value();
        if !rename_form(thing).savable(&typed) {
            return;
        }
        run(&thing.connection, thing.target.rename(typed.trim()), cx);
        close_sheet(RENAME_SHEET, cx);
    }

    fn sheet_after(&mut self, next: PhoneSheet, cx: &mut Context<Self>) {
        self.compact.next_sheet = Some(next);
        close_sheet(HOLD_SHEET, cx);
    }

    fn sheet_dismissed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let renaming = matches!(self.compact.sheet, Some(PhoneSheet::Rename(_)));
        self.compact.sheet = None;
        if renaming {
            window.dismiss_virtual_keyboard();
            self.focused_pane = None;
        }
        if let Some(next) = self.compact.next_sheet.take() {
            self.open_phone_sheet(next, window, cx);
        }
        cx.notify();
    }

    pub(in super::super) fn phone_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.compact.sheet.clone()?;
        let dismiss = cx.weak_entity();
        let on_dismiss = move |window: &mut Window, cx: &mut ala::App| {
            let _ = dismiss.update(cx, |this, cx| this.sheet_dismissed(window, cx));
        };
        if let PhoneSheet::Link(uri) = &sheet {
            return Some(self.link_sheet(uri, on_dismiss, cx));
        }
        let thing = sheet.thing()?.clone();
        Some(match sheet {
            PhoneSheet::Hold(_) => {
                let rename = PhoneSheet::Rename(thing.clone());
                let close = PhoneSheet::Close(thing.clone());
                let (rename_label, close_label) = match thing.target {
                    Target::Session(_) => ("Rename session", "Close session"),
                    Target::Window(_) => ("Rename", "Close window"),
                    Target::Pane(_) => ("Rename", "Close"),
                };
                let rows = [
                    TouchRow::new("hold-rename", rename_label)
                        .icon(IconName::Pencil)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.sheet_after(rename.clone(), cx);
                        })),
                    TouchRow::new("hold-close", close_label)
                        .icon(IconName::Xmark)
                        .danger()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.sheet_after(close.clone(), cx);
                        })),
                ];
                hold_sheet(HOLD_SHEET, thing.name, rows, on_dismiss, cx).into_any_element()
            }
            PhoneSheet::Close(_) => {
                let connection = thing.connection.clone();
                let target = thing.target;
                Confirm {
                    id: CONFIRM_SHEET.into(),
                    question: format!("Close this {}?", target.noun()).into(),
                    detail: thing.closing.map(SharedString::from),
                    verb: "Close".into(),
                }
                .sheet(
                    move |_, cx| run(&connection, target.close(), cx),
                    on_dismiss,
                    window,
                    cx,
                )
                .into_any_element()
            }
            PhoneSheet::Rename(_) => {
                let input = self.rename_input(window, cx);
                let connection = thing.connection.clone();
                let target = thing.target;
                let sheet = rename_form(&thing).sheet(
                    &input,
                    move |name, _, cx| run(&connection, target.rename(&name), cx),
                    on_dismiss,
                    window,
                    cx,
                );
                floating_sheet(sheet, window)
            }
            PhoneSheet::Link(_) => return None,
        })
    }

    fn link_sheet(
        &self,
        uri: &str,
        on_dismiss: impl Fn(&mut Window, &mut ala::App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut actions = Vec::new();
        if zz_client::terminal_view::openable_uri(uri) {
            actions.push(LinkAction::Browser);
            if super::super::settings::phone() {
                actions.push(LinkAction::Phone);
            }
        }
        actions.push(LinkAction::Copy);
        let picked = uri.to_owned();
        let view = cx.weak_entity();
        link_sheet(
            LINK_SHEET,
            uri.to_owned(),
            &actions,
            move |action, _, cx| {
                match action {
                    LinkAction::Browser => {
                        let _ = view.update(cx, |this, cx| this.open_link_in_browser(&picked, cx));
                    }
                    LinkAction::Phone => cx.open_url(&picked),
                    LinkAction::Copy => {
                        cx.write_to_clipboard(ala::ClipboardItem::new_string(picked.clone()));
                    }
                }
                close_sheet(LINK_SHEET, cx);
            },
            on_dismiss,
            cx,
        )
        .into_any_element()
    }
}
