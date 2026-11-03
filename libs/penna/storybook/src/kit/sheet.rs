use ala::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Icon, IconName, StyledExt as _, h_flex,
    sheet::{
        BottomSheet, bottom_sheet, close_sheet, sheet_action, sheet_close, sheet_detail_row,
        sheet_option,
    },
    v_flex,
};

use crate::creator::{SAFE_BOTTOM, SCREEN_RADIUS};
use crate::story::Framed as _;
use crate::story::{Section, Story, states};

pub const STORY: Story = Story {
    id: "sheet",
    name: "Sheet",
    group: "Kit",
    summary: "The bottom sheet a touch screen opens in place of a dropdown. It floats up from the bottom of its parent over a scrim, rounded on every corner, and drags down to dismiss.",
    sections: &[Section {
        id: "bottom-sheet",
        name: "Bottom sheet",
        summary: "Phone-sized frames, each sheet 8pt in from the sides and resting on a 34pt bottom safe area: a picker with a checked option and an action row, a short sheet of commands, and choices that each say what they do. The close button and the scrim dismiss a sheet; a press on its empty frame, or any theme change, brings it back.",
        build: |window, cx| Sheets::view(window, cx),
    }],
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
        .gap_3()
        .child(div().text_lg().child(title))
        .children(["work", "scratch", "logs"].into_iter().map(move |name| {
            h_flex()
                .gap_2()
                .text_sm()
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
                        .text_size(px(13.0))
                        .line_height(px(18.0))
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
