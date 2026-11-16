use ala::{App, ParentElement as _, Styled as _, Window, px};
use penna::{
    Chip, ChipLook, Emphasis, Panel, PanelLook, Sizable as _, Size, Tone,
    blocks::code_editor::VimMode,
    blocks::feedback::{alert_actions, alert_ground, inline_alert},
    components::button::{Button, ButtonVariants as _},
};
use zz_protocol::PaneId;

#[must_use]
pub fn vim_mode_chip(pane: PaneId, mode: VimMode) -> Chip {
    let chip = Chip::new(("editor-vim-mode", pane.0), mode.label())
        .look(ChipLook::Floating)
        .with_size(Size::XSmall);
    let chip = if matches!(mode, VimMode::Normal) {
        chip
    } else {
        chip.tone(Tone::Primary).emphasis(Emphasis::Solid)
    };
    chip.absolute()
        .right(px(8.0))
        .bottom(px(8.0))
        .h(px(18.0))
        .px(px(6.0))
}

pub fn editor_error(
    pane: PaneId,
    title: &'static str,
    message: &str,
    retry: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Panel {
    Panel::new(("editor-error", pane.0), PanelLook::Plain)
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .child(
            inline_alert(Some(title.into()), message.to_owned(), cx)
                .max_w(px(360.0))
                .child(
                    alert_actions(cx).child(
                        Button::new(format!("editor-retry-{}", pane.0))
                            .primary()
                            .on(alert_ground(cx))
                            .label("Try again")
                            .on_click(move |_, window, cx| {
                                retry(window, cx);
                                cx.stop_propagation();
                            }),
                    ),
                ),
        )
}
