use std::{cell::RefCell, rc::Rc};

use ala::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, Keystroke,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, ScrollWheelEvent, Window,
    prelude::*,
};
use zz_client::{MenuBox, MenuKeyResult, MenuPointerKind, resolve_menu_key, resolve_menu_mouse};
use zz_protocol::{InputMessage, MenuAction, MenuState};
use zz_terminal::KeyAction;

use crate::mux::{client::MuxClient, prefix::terminal_key_input};
use zz_ui::command::floating::{MenuHits, display_menu, menu_enter, menu_hit_cell};

pub(crate) use zz_ui::command::floating::{
    RELEASE_BUTTONS, WHEEL_BUTTONS, menu_press_buttons as press_buttons,
};

pub(crate) struct MenuView {
    focus_handle: FocusHandle,
    mux: Entity<MuxClient>,
    state: MenuState,
    selected: Option<usize>,
    hits: Rc<RefCell<MenuHits>>,
}

impl MenuView {
    pub(crate) fn new(mux: Entity<MuxClient>, state: MenuState, cx: &mut Context<Self>) -> Self {
        let selected = state.selected.and_then(|index| usize::try_from(index).ok());
        Self {
            focus_handle: cx.focus_handle(),
            mux,
            state,
            selected,
            hits: Rc::default(),
        }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(crate) fn state(&self) -> &MenuState {
        &self.state
    }

    pub(crate) fn synchronize(&mut self, state: MenuState, cx: &mut Context<Self>) {
        self.selected = self
            .selected
            .filter(|_| !state.items.is_empty())
            .map(|selected| selected.min(state.items.len() - 1));
        self.state = state;
        cx.notify();
    }

    fn send(&self, action: MenuAction, cx: &App) {
        self.mux.read(cx).send_input(InputMessage::Menu { action });
    }

    pub(crate) fn pointer(
        &mut self,
        kind: MenuPointerKind,
        buttons: u8,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.state.mouse_keys && kind == MenuPointerKind::Motion {
            return;
        }
        let (column, row) = menu_hit_cell(&self.state, &self.hits.borrow(), position);
        let frame = MenuBox {
            left: self.state.left,
            top: self.state.top,
            width: self.state.width,
            items: self.state.items.len(),
        };
        match resolve_menu_mouse(
            &self.state,
            self.selected,
            frame,
            kind,
            buttons,
            column,
            row,
        ) {
            MenuKeyResult::Action(action) => self.send(action, cx),
            MenuKeyResult::Select(selected) => {
                if self.selected != selected {
                    self.selected = selected;
                    cx.notify();
                }
            }
            MenuKeyResult::Consumed => {}
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match resolve_keystroke(&self.state, self.selected, &event.keystroke) {
            MenuKeyResult::Action(action) => self.send(action, cx),
            MenuKeyResult::Select(selected) => {
                self.selected = selected;
                cx.notify();
            }
            MenuKeyResult::Consumed => {}
        }
        cx.stop_propagation();
    }
}

impl Focusable for MenuView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MenuView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = display_menu(
            &self.state,
            self.selected,
            window.viewport_size(),
            &self.hits,
            cx,
        )
        .track_focus(&self.focus_handle)
        .on_key_down(cx.listener(Self::on_key_down))
        .on_key_up(|_, _, cx| cx.stop_propagation())
        .capture_any_mouse_down(cx.listener(|menu, event: &MouseDownEvent, _, cx| {
            menu.pointer(
                MenuPointerKind::Press,
                press_buttons(event.button),
                event.position,
                cx,
            );
            cx.stop_propagation();
        }))
        .capture_any_mouse_up(cx.listener(|menu, event: &MouseUpEvent, _, cx| {
            menu.pointer(
                MenuPointerKind::Release,
                RELEASE_BUTTONS,
                event.position,
                cx,
            );
            cx.stop_propagation();
        }))
        .on_mouse_move(cx.listener(|menu, event: &MouseMoveEvent, _, cx| {
            let (kind, buttons) = event
                .pressed_button
                .map_or((MenuPointerKind::Motion, RELEASE_BUTTONS), |button| {
                    (MenuPointerKind::Drag, press_buttons(button))
                });
            menu.pointer(kind, buttons, event.position, cx);
            cx.stop_propagation();
        }))
        .on_scroll_wheel(cx.listener(|menu, event: &ScrollWheelEvent, _, cx| {
            menu.pointer(MenuPointerKind::Wheel, WHEEL_BUTTONS, event.position, cx);
            cx.stop_propagation();
        }));
        menu_enter(surface)
    }
}

fn resolve_keystroke(
    state: &MenuState,
    selected: Option<usize>,
    keystroke: &Keystroke,
) -> MenuKeyResult {
    let input = terminal_key_input(keystroke, KeyAction::Press);
    resolve_menu_key(state, selected, &input)
}

#[cfg(test)]
mod tests {
    use ala::Modifiers;
    use zz_protocol::{MenuItem, PopupBorderLines};

    use super::*;

    fn state() -> MenuState {
        MenuState {
            left: 0,
            top: 0,
            width: 20,
            height: 6,
            client_columns: 80,
            client_rows: 24,
            cell_width_px: 8,
            cell_height_px: 18,
            title: String::new(),
            style: "default".to_owned(),
            selected_style: "default".to_owned(),
            border_style: "default".to_owned(),
            border_lines: PopupBorderLines::Single,
            items: vec![
                Some(MenuItem {
                    name: "Quit item".to_owned(),
                    key: Some("q".to_owned()),
                    annotation: Some("q".to_owned()),
                    enabled: true,
                }),
                None,
                Some(MenuItem {
                    name: "Disabled".to_owned(),
                    key: None,
                    annotation: None,
                    enabled: false,
                }),
                Some(MenuItem {
                    name: "Last".to_owned(),
                    key: None,
                    annotation: None,
                    enabled: true,
                }),
            ],
            selected: Some(0),
            stay_open: false,
            mouse_keys: false,
        }
    }

    fn key(value: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            key: value.to_owned(),
            key_char: key_char.map(str::to_owned),
            modifiers,
        }
    }

    #[test]
    fn ala_shift_tab_reaches_the_shared_backtab_path() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            resolve_keystroke(&state(), Some(3), &key("tab", None, shift)),
            MenuKeyResult::Select(Some(0))
        );
    }
}
