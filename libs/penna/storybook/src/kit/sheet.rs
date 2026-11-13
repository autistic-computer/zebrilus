use ala::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Icon, IconName, Space, StyledExt as _, TextStyle,
    blocks::sheet::{
        BottomSheet, ModalAction, ModalTone, bottom_sheet, close_sheet, modal_body, modal_buttons,
        modal_groups, sheet_back, sheet_close, sheet_groups, sheet_note,
    },
    blocks::touch::{TouchGroup, TouchRow, TouchTrailing},
    h_flex, v_flex,
};

use crate::creator::{SAFE_BOTTOM, SCREEN_RADIUS};
use crate::story::Framed as _;
use crate::story::{Section, Story, states};

pub const STORY: Story = Story {
    id: "sheet",
    name: "Sheet",
    group: "Kit",
    summary: "What a touch screen opens over itself. A list or a page you drill into rises as a bottom sheet with a quiet caption and a close disc; a short ask (confirm, rename, a few actions, one control) is a card in the middle of the screen.",
    sections: &[
        Section {
            id: "bottom-sheet",
            name: "Bottom sheet",
            summary: "Phone-sized frames, each sheet 8pt in from the sides and resting on a 34pt bottom safe area: a picker, a page drilled into with its back chevron, and rows in two groups over a muted line. The close disc and the scrim dismiss a sheet; a press on its empty frame, or any theme change, brings it back.",
            build: |window, cx| Sheets::view(window, cx),
        },
        Section {
            id: "modal",
            name: "Modal",
            summary: "The centered card for a short ask: a hold menu titled with the thing's name over its actions, the one that removes in its own group, and a confirm with the question, what goes, Cancel and the verb.",
            build: |_, cx| cx.new(|_| Modals { open: [true; 2] }).into(),
        },
    ],
};

const LANGUAGES: [&str; 5] = ["English", "Deutsch", "Español", "Português", "Français"];
const MODELS: [&str; 4] = ["Default (recommended)", "Opus", "Sonnet", "Haiku"];

fn phone<V: 'static>(
    id: &'static str,
    title: &'static str,
    sheet: Option<BottomSheet>,
    cx: &mut Context<V>,
    reopen: impl Fn(&mut V) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let closed = sheet.is_none();
    div()
        .id(id)
        .relative()
        .w(px(390.0))
        .h(px(600.0))
        .overflow_hidden()
        .rounded(px(SCREEN_RADIUS))
        .framed(cx)
        .bg(theme.surface(Elevation::Panel).fill)
        .child(backdrop(title, cx))
        .children(sheet)
        .when(closed, |frame| {
            frame.on_click(cx.listener(move |this, _, _, cx| {
                reopen(this);
                cx.notify();
            }))
        })
}

fn backdrop(title: &'static str, cx: &App) -> impl IntoElement {
    let muted = cx.theme().foreground.muted();
    v_flex()
        .p(px(20.0))
        .gap(Space::S12)
        .child(div().type_style(TextStyle::TouchBody, cx).child(title))
        .children(["work", "scratch", "logs"].into_iter().map(move |name| {
            h_flex()
                .gap(Space::S8)
                .type_style(TextStyle::Lead, cx)
                .text_color(muted)
                .child(Icon::new(IconName::SquareTerminal).medium())
                .child(name)
        }))
}

struct Sheets {
    open: [bool; 3],
    _theme: Subscription,
}

impl Sheets {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|cx| Self {
            open: [true; 3],
            _theme: cx.observe_global::<penna::Theme>(|this: &mut Self, cx| {
                if this.open != [true; 3] {
                    this.open = [true; 3];
                    cx.notify();
                }
            }),
        })
        .into()
    }
}

fn dismissed<V: 'static>(
    cx: &mut Context<V>,
    shut: impl Fn(&mut V) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let view = cx.entity().downgrade();
    move |_, cx| {
        let _ = view.update(cx, |this, cx| {
            shut(this);
            cx.notify();
        });
    }
}

fn close(sheet: &'static str, button: &'static str, cx: &App) -> ala::AnyElement {
    sheet_close(button, cx)
        .on_click(move |_, _, cx| close_sheet(sheet, cx))
        .into_any_element()
}

impl Render for Sheets {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = bottom_sheet(
            "language-sheet",
            "Language",
            [close("language-sheet", "language-close", cx)],
            sheet_groups()
                .child(
                    TouchGroup::new()
                        .ruled()
                        .rows(LANGUAGES.iter().enumerate().map(|(ix, name)| {
                            let row = TouchRow::new(("language", ix), SharedString::from(*name));
                            if ix == 1 {
                                row.trailing(TouchTrailing::Check)
                            } else {
                                row
                            }
                        })),
                )
                .child(
                    TouchGroup::new()
                        .ruled()
                        .row(TouchRow::new("language-add", "Add a language").icon(IconName::Plus)),
                ),
            px(SAFE_BOTTOM),
            dismissed(cx, |this: &mut Self| this.open[0] = false),
        );
        let model =
            bottom_sheet(
                "model-sheet",
                "Model",
                [close("model-sheet", "model-close", cx)],
                sheet_groups().child(TouchGroup::new().ruled().rows(
                    MODELS.iter().enumerate().map(|(ix, name)| {
                        let row = TouchRow::new(("model", ix), SharedString::from(*name));
                        if ix == 0 {
                            row.trailing(TouchTrailing::Check)
                        } else {
                            row
                        }
                    }),
                )),
                px(SAFE_BOTTOM),
                dismissed(cx, |this: &mut Self| this.open[1] = false),
            )
            .back(sheet_back("model-back", cx));
        let setting = |id: &'static str, title: &'static str, icon, value: &'static str| {
            TouchRow::new(id, title)
                .icon(icon)
                .trailing(TouchTrailing::Value(value.into()))
                .more()
        };
        let options = bottom_sheet(
            "options-sheet",
            "Claude Code",
            [close("options-sheet", "options-close", cx)],
            sheet_groups()
                .child(
                    TouchGroup::new().ruled().row(
                        TouchRow::new("options-mention", "Mention")
                            .icon(IconName::ListTree)
                            .more(),
                    ),
                )
                .child(TouchGroup::new().ruled().rows([
                    setting(
                        "options-model",
                        "Model",
                        IconName::Sparkle,
                        "Default (recommended)",
                    ),
                    setting("options-effort", "Effort", IconName::Cpu, "Default"),
                    setting("options-mode", "Permissions", IconName::Hand, "Ask"),
                ]))
                .child(sheet_note("Context 0%", cx)),
            px(SAFE_BOTTOM),
            dismissed(cx, |this: &mut Self| this.open[2] = false),
        );
        let [picker_open, model_open, options_open] = self.open;
        states()
            .columns(2)
            .state(
                "picker",
                phone(
                    "phone-picker",
                    "Settings",
                    picker_open.then_some(picker),
                    cx,
                    |this| {
                        this.open[0] = true;
                    },
                ),
            )
            .state(
                "a page drilled into",
                phone(
                    "phone-model",
                    "work",
                    model_open.then_some(model),
                    cx,
                    |this| {
                        this.open[1] = true;
                    },
                ),
            )
            .state(
                "groups over a muted line",
                phone(
                    "phone-options",
                    "macbook",
                    options_open.then_some(options),
                    cx,
                    |this| {
                        this.open[2] = true;
                    },
                ),
            )
    }
}

struct Modals {
    open: [bool; 2],
}

impl Render for Modals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = bottom_sheet(
            "machine-sheet",
            "xps",
            [close("machine-sheet", "machine-close", cx)],
            modal_groups()
                .child(
                    TouchGroup::new()
                        .ruled()
                        .row(TouchRow::new("machine-connect", "Connect").icon(IconName::Server)),
                )
                .child(
                    TouchGroup::new().ruled().row(
                        TouchRow::new("machine-remove", "Remove machine")
                            .icon(IconName::Xmark)
                            .danger(),
                    ),
                ),
            px(SAFE_BOTTOM),
            dismissed(cx, |this: &mut Self| this.open[0] = false),
        )
        .modal();
        let confirm = bottom_sheet(
            "confirm-sheet",
            "Close this pane?",
            Vec::new(),
            modal_body().child(modal_buttons(
                "confirm-sheet",
                ModalAction::new("confirm-close", "Close", ModalTone::Danger, |_, _| {}),
                window,
                cx,
            )),
            px(SAFE_BOTTOM),
            dismissed(cx, |this: &mut Self| this.open[1] = false),
        )
        .modal()
        .detail("The chat stops. Its transcript stays.");
        let [menu_open, confirm_open] = self.open;
        states()
            .columns(2)
            .state(
                "hold a machine",
                phone(
                    "phone-held",
                    "Machines",
                    menu_open.then_some(menu),
                    cx,
                    |this| {
                        this.open[0] = true;
                    },
                ),
            )
            .state(
                "close asks first",
                phone(
                    "phone-confirm",
                    "work",
                    confirm_open.then_some(confirm),
                    cx,
                    |this| {
                        this.open[1] = true;
                    },
                ),
            )
    }
}
