use zz_gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Keystroke, MouseButton,
    Render, Window, prelude::*,
};
use zz_protocol::{CommandInvocation, PaneId};
use zz_ui::IconName;

use crate::mux::client::MuxClient;
use crate::window::corners::WindowCorners;
use crate::{browser, config};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneChoice {
    Terminal,
    Browser,
    Editor,
}

const CHOICES: [PaneChoice; 3] = [
    PaneChoice::Terminal,
    PaneChoice::Browser,
    PaneChoice::Editor,
];

fn choices(cx: &App) -> Vec<PaneChoice> {
    CHOICES
        .into_iter()
        .filter(|choice| match choice {
            PaneChoice::Terminal => true,
            PaneChoice::Browser => browser::controller::is_available(cx),
            PaneChoice::Editor => config::editor_pane_enabled(cx),
        })
        .collect()
}

impl PaneChoice {
    const fn command_argument(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Browser => "browser",
            Self::Editor => "editor",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Browser => "Browser",
            Self::Editor => "Editor",
        }
    }

    const fn shortcut(self) -> &'static str {
        match self {
            Self::Terminal => "t",
            Self::Browser => "b",
            Self::Editor => "e",
        }
    }

    const fn icon(self) -> IconName {
        match self {
            Self::Terminal => IconName::SquareTerminal,
            Self::Browser => IconName::BrandChrome,
            Self::Editor => IconName::File,
        }
    }

    const fn element_id(self) -> &'static str {
        match self {
            Self::Terminal => "pane-picker-terminal",
            Self::Browser => "pane-picker-browser",
            Self::Editor => "pane-picker-editor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerAction {
    Activate(PaneChoice),
    Move(isize),
    Confirm,
    Close,
}

fn picker_action(keystroke: &Keystroke, choices: &[PaneChoice]) -> Option<PickerAction> {
    let modifiers = keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    if let Some(choice) = choices
        .iter()
        .find(|choice| choice.shortcut() == keystroke.key)
    {
        return Some(PickerAction::Activate(*choice));
    }
    match keystroke.key.as_str() {
        "j" | "down" => Some(PickerAction::Move(1)),
        "k" | "up" => Some(PickerAction::Move(-1)),
        "enter" => Some(PickerAction::Confirm),
        "escape" => Some(PickerAction::Close),
        _ => None,
    }
}

fn wrapping_step(selected: usize, delta: isize, len: usize) -> usize {
    (selected + len).wrapping_add_signed(delta) % len
}

fn materialize_command(pane: PaneId, choice: PaneChoice) -> CommandInvocation {
    CommandInvocation::new(
        "select-pane-kind",
        [
            "-t".to_owned(),
            pane.to_string(),
            choice.command_argument().to_owned(),
        ],
    )
}

fn close_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new("kill-pane", ["-t".to_owned(), pane.to_string()])
}

fn select_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new("select-pane", ["-t".to_owned(), pane.to_string()])
}

pub(crate) struct PanePickerView {
    pane: PaneId,
    mux: zz_gpui::Entity<MuxClient>,
    focus_handle: FocusHandle,
    selected: usize,
    window_corners: WindowCorners,
}

impl PanePickerView {
    pub(crate) fn new(
        pane: PaneId,
        mux: zz_gpui::Entity<MuxClient>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            pane,
            mux,
            focus_handle: cx.focus_handle(),
            selected: 0,
            window_corners: WindowCorners::NONE,
        }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(crate) fn set_window_corners(&mut self, corners: WindowCorners, cx: &mut Context<Self>) {
        if self.window_corners != corners {
            self.window_corners = corners;
            let view = cx.weak_entity();
            cx.defer(move |cx| {
                view.update(cx, |_, cx| cx.notify()).ok();
            });
        }
    }

    fn activate(&self, choice: PaneChoice, cx: &Context<Self>) {
        self.mux
            .read(cx)
            .execute(materialize_command(self.pane, choice));
    }

    fn close(&self, cx: &Context<Self>) {
        self.mux.read(cx).execute(close_command(self.pane));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let choices = choices(cx);
        match picker_action(&event.keystroke, &choices) {
            Some(PickerAction::Activate(choice)) => self.activate(choice, cx),
            Some(PickerAction::Move(delta)) => {
                self.selected = wrapping_step(self.selected, delta, choices.len());
                cx.notify();
            }
            Some(PickerAction::Confirm) => {
                if let Some(choice) = choices.get(self.selected).copied() {
                    self.activate(choice, cx);
                }
            }
            Some(PickerAction::Close) => self.close(cx),
            None => return,
        }
        cx.stop_propagation();
    }

    fn row(&self, index: usize, choice: PaneChoice, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected == index;
        let view = cx.entity();
        let hover_view = view.clone();
        zz_ui::pane::pane_picker_row(
            (choice.element_id(), self.pane.0),
            choice.title(),
            choice.icon(),
            choice.shortcut(),
            selected,
            true,
            cx,
        )
        .on_mouse_move(move |_, _, cx| {
            hover_view.update(cx, |picker, cx| {
                if picker.selected != index {
                    picker.selected = index;
                    cx.notify();
                }
            });
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            view.update(cx, |picker, cx| picker.activate(choice, cx));
            cx.stop_propagation();
        })
    }
}

impl Focusable for PanePickerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PanePickerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let select_mux = self.mux.clone();
        let pane = self.pane;
        let choices = choices(cx);
        self.selected = self.selected.min(choices.len() - 1);
        let rows = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| self.row(index, *choice, cx).into_any_element())
            .collect::<Vec<_>>();
        zz_ui::pane::pane_picker_surface(
            ("pane-picker", self.pane.0),
            config::pane_content_radii(cx, self.window_corners),
            rows,
            cx,
        )
        .track_focus(&self.focus_handle)
        .on_key_down(cx.listener(Self::on_key_down))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            select_mux.read(cx).execute(select_command(pane));
            focus_handle.focus(window, cx);
        })
    }
}

#[cfg(test)]
mod tests {
    use zz_gpui::Modifiers;

    use super::*;

    fn key(value: &str) -> Keystroke {
        Keystroke {
            modifiers: Modifiers::default(),
            key: value.to_owned(),
            key_char: Some(value.to_owned()),
        }
    }

    #[test]
    fn direct_shortcuts_activate_each_choice_and_escape_closes() {
        assert_eq!(
            CHOICES,
            [
                PaneChoice::Terminal,
                PaneChoice::Browser,
                PaneChoice::Editor,
            ],
            "the ungated picker catalog contains all three pane kinds"
        );
        assert_eq!(
            picker_action(&key("t"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Terminal))
        );
        assert_eq!(
            picker_action(&key("b"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Browser))
        );
        assert_eq!(
            picker_action(&key("e"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Editor))
        );
        assert_eq!(
            picker_action(&key("escape"), &CHOICES),
            Some(PickerAction::Close)
        );
    }

    #[test]
    fn pane_choices_use_pane_specific_icons() {
        assert_eq!(PaneChoice::Browser.icon(), IconName::BrandChrome);
        assert_eq!(PaneChoice::Editor.icon(), IconName::File);
    }

    #[test]
    fn gated_out_choices_take_their_hotkeys_with_them() {
        let editor_gated = [PaneChoice::Terminal, PaneChoice::Browser];
        assert_eq!(picker_action(&key("e"), &editor_gated), None);
        assert_eq!(
            picker_action(&key("t"), &editor_gated),
            Some(PickerAction::Activate(PaneChoice::Terminal))
        );
    }

    #[test]
    fn vertical_keys_move_the_cursor_and_enter_takes_it() {
        for key_name in ["j", "down"] {
            assert_eq!(
                picker_action(&key(key_name), &CHOICES),
                Some(PickerAction::Move(1))
            );
        }
        for key_name in ["k", "up"] {
            assert_eq!(
                picker_action(&key(key_name), &CHOICES),
                Some(PickerAction::Move(-1))
            );
        }
        assert_eq!(
            picker_action(&key("enter"), &CHOICES),
            Some(PickerAction::Confirm)
        );
    }

    #[test]
    fn horizontal_keys_do_not_drive_the_picker() {
        for key_name in ["left", "right", "h", "l"] {
            assert_eq!(picker_action(&key(key_name), &CHOICES), None);
        }
    }

    #[test]
    fn the_cursor_wraps_at_both_ends() {
        assert_eq!(wrapping_step(0, 1, CHOICES.len()), 1);
        assert_eq!(wrapping_step(CHOICES.len() - 1, 1, CHOICES.len()), 0);
        assert_eq!(wrapping_step(0, -1, CHOICES.len()), CHOICES.len() - 1);
        assert_eq!(wrapping_step(1, -1, CHOICES.len()), 0);
    }

    #[test]
    fn command_modifiers_do_not_drive_the_picker() {
        let mut keystroke = key("t");
        keystroke.modifiers.platform = true;
        assert_eq!(picker_action(&keystroke, &CHOICES), None);
    }

    #[test]
    fn activation_targets_the_existing_picker_pane() {
        assert_eq!(
            materialize_command(PaneId(42), PaneChoice::Browser),
            CommandInvocation::new("select-pane-kind", ["-t", "%42", "browser"])
        );
        assert_eq!(
            materialize_command(PaneId(42), PaneChoice::Editor),
            CommandInvocation::new("select-pane-kind", ["-t", "%42", "editor"])
        );
        assert_eq!(
            close_command(PaneId(7)),
            CommandInvocation::new("kill-pane", ["-t", "%7"])
        );
        assert_eq!(
            select_command(PaneId(7)),
            CommandInvocation::new("select-pane", ["-t", "%7"])
        );
    }
}
