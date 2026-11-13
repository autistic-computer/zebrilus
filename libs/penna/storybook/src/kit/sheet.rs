use ala::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Icon, IconName, Space, StyledExt as _, TextStyle,
    blocks::sheet::{
        BottomSheet, bottom_sheet, close_sheet, sheet_action, sheet_close, sheet_detail_row,
        sheet_option,
    },
    blocks::touch::TouchRow,
    h_flex, v_flex,
};

use crate::creator::{SAFE_BOTTOM, SCREEN_RADIUS};
use crate::story::Framed as _;
use crate::story::{Section, Story, states};

pub const STORY: Story = Story {
    id: "sheet",
    name: "Sheet",
    group: "Kit",
    summary: "The bottom sheet a touch screen opens in place of a dropdown. It floats up from the bottom of its parent over a scrim, rounded on every corner, and drags down to dismiss.",
    sections: &[
        Section {
            id: "bottom-sheet",
            name: "Bottom sheet",
            summary: "Phone-sized frames, each sheet 8pt in from the sides and resting on a 34pt bottom safe area: a picker with a checked option and an action row, a short sheet of commands, and choices that each say what they do. The close button and the scrim dismiss a sheet; a press on its empty frame, or any theme change, brings it back.",
            build: |window, cx| Sheets::view(window, cx),
        },
        Section {
            id: "hold-sheet",
            name: "Hold sheet",
            summary: "What holding a tile opens: titled with the thing's name over one muted line of what the tile leaves out, then one-line touch rows, the one that removes last in danger ink.",
            build: |_, cx| cx.new(|_| Held { open: true }).into(),
        },
    ],
};

const LANGUAGES: [&str; 5] = ["English", "Deutsch", "Español", "Português", "Français"];

fn phone(
    id: &'static str,
    title: &'static str,
    sheet: Option<BottomSheet>,
    cx: &mut Context<Sheets>,
    at: usize,
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
                this.open[at] = true;
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

    fn dismissed(at: usize, cx: &mut Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let view = cx.entity().downgrade();
        move |_, cx| {
            let _ = view.update(cx, |this, cx| {
                this.open[at] = false;
                cx.notify();
            });
        }
    }
}

fn close(sheet: &'static str, button: &'static str) -> ala::AnyElement {
    sheet_close(button)
        .on_click(move |_, _, cx| close_sheet(sheet, cx))
        .into_any_element()
}

impl Render for Sheets {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = bottom_sheet(
            "language-sheet",
            "Language",
            [close("language-sheet", "language-close")],
            v_flex()
                .list_inset(cx)
                .pb(px(8.0))
                .children(LANGUAGES.iter().enumerate().map(|(ix, name)| {
                    sheet_option(("language", ix), SharedString::from(*name), ix == 1, cx)
                }))
                .child(sheet_action(
                    "language-add",
                    IconName::Plus,
                    "Add a language",
                    cx,
                )),
            px(SAFE_BOTTOM),
            Self::dismissed(0, cx),
        );
        let commands = bottom_sheet(
            "pane-sheet",
            "Pane",
            [close("pane-sheet", "pane-close")],
            v_flex()
                .list_inset(cx)
                .pb(px(8.0))
                .child(sheet_action(
                    "pane-split",
                    IconName::PanelRight,
                    "Split right",
                    cx,
                ))
                .child(sheet_action("pane-zoom", IconName::ZoomIn, "Zoom pane", cx))
                .child(sheet_action(
                    "pane-close-row",
                    IconName::Xmark,
                    "Close pane",
                    cx,
                )),
            px(SAFE_BOTTOM),
            Self::dismissed(1, cx),
        );
        let muted = cx.theme().foreground.muted();
        let choices = bottom_sheet(
            "new-sheet",
            "New",
            [close("new-sheet", "new-close")],
            v_flex()
                .list_inset(cx)
                .pb(px(8.0))
                .child(
                    div()
                        .px(px(12.0))
                        .pb(px(10.0))
                        .type_style(TextStyle::Body, cx)
                        .text_color(muted)
                        .child("Opens as a new window on macbook."),
                )
                .child(sheet_detail_row(
                    "new-terminal",
                    Some(Icon::new(IconName::SquareTerminal)),
                    "Terminal",
                    "A shell in the current folder",
                    cx,
                ))
                .child(sheet_detail_row(
                    "new-browser",
                    Some(Icon::new(IconName::Globe)),
                    "Browser",
                    "A web page on macbook, localhost included",
                    cx,
                )),
            px(SAFE_BOTTOM),
            Self::dismissed(2, cx),
        );
        let [picker_open, commands_open, choices_open] = self.open;
        let picker = picker_open.then_some(picker);
        let commands = commands_open.then_some(commands);
        let choices = choices_open.then_some(choices);
        states()
            .columns(2)
            .state("picker", phone("phone-picker", "Settings", picker, cx, 0))
            .state("commands", phone("phone-commands", "work", commands, cx, 1))
            .state(
                "choices with a line",
                phone("phone-choices", "macbook", choices, cx, 2),
            )
    }
}

struct Held {
    open: bool,
}

impl Render for Held {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let sheet = self.open.then(|| {
            bottom_sheet(
                "machine-sheet",
                "xps",
                [close("machine-sheet", "machine-close")],
                v_flex()
                    .gap(px(2.0))
                    .px(cx.theme().row_inset)
                    .pb(px(8.0))
                    .child(
                        TouchRow::new("machine-connect", "Connect")
                            .icon(IconName::Server)
                            .in_sheet(cx),
                    )
                    .child(
                        TouchRow::new("machine-remove", "Remove machine")
                            .icon(IconName::Xmark)
                            .danger()
                            .in_sheet(cx),
                    ),
                px(SAFE_BOTTOM),
                move |_, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.open = false;
                        cx.notify();
                    });
                },
            )
            .detail("fabrico@xps")
        });
        let closed = sheet.is_none();
        states().state(
            "a machine",
            div()
                .id("phone-held")
                .relative()
                .w(px(390.0))
                .h(px(600.0))
                .overflow_hidden()
                .rounded(px(SCREEN_RADIUS))
                .framed(cx)
                .bg(cx.theme().surface(Elevation::Panel).fill)
                .child(backdrop("Machines", cx))
                .children(sheet)
                .when(closed, |frame| {
                    frame.on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    }))
                }),
        )
    }
}
