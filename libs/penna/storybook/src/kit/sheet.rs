use ala::{
    AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div, px,
};
use penna::roles;
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Icon, IconName, StyledExt as _, h_flex,
    sheet::{BottomSheet, bottom_sheet, sheet_action, sheet_close, sheet_detail_row, sheet_option},
    v_flex,
};

use crate::creator::{SAFE_BOTTOM, SCREEN_RADIUS};
use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "sheet",
    name: "Sheet",
    group: "Kit",
    summary: "The bottom sheet a touch screen opens in place of a dropdown. It floats up from the bottom of its parent over a scrim, rounded on every corner, and drags down to dismiss.",
    sections: &[Section {
        id: "bottom-sheet",
        name: "Bottom sheet",
        summary: "Phone-sized frames, each sheet 8pt in from the sides and resting on a 34pt bottom safe area: a picker with a checked option and an action row, a short sheet of commands, and choices that each say what they do.",
        build: |_, cx| stateless(sheets, cx),
    }],
};

const LANGUAGES: [&str; 5] = ["English", "Deutsch", "Español", "Português", "Français"];

fn phone(content: impl IntoElement, sheet: BottomSheet, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .relative()
        .w(px(390.0))
        .h(px(600.0))
        .overflow_hidden()
        .rounded(px(SCREEN_RADIUS))
        .framed(cx)
        .bg(theme.surface(Elevation::Panel).fill)
        .child(content)
        .child(sheet)
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

fn sheets(_: &mut Window, cx: &mut App) -> AnyElement {
    let picker = bottom_sheet(
        "language-sheet",
        "Language",
        [sheet_close("language-close").into_any_element()],
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
        |_, _| {},
    );
    let commands = bottom_sheet(
        "pane-sheet",
        "Pane",
        [sheet_close("pane-close").into_any_element()],
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
        |_, _| {},
    );
    let muted = cx.theme().foreground.muted();
    let choices = bottom_sheet(
        "new-sheet",
        "New",
        [sheet_close("new-close").into_any_element()],
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
                roles::PHONE_ROW,
                Some(Icon::new(IconName::SquareTerminal)),
                "Terminal",
                "A shell in the current folder",
                cx,
            ))
            .child(sheet_detail_row(
                "new-browser",
                roles::PHONE_ROW,
                Some(Icon::new(IconName::Globe)),
                "Browser",
                "A web page on macbook, localhost included",
                cx,
            )),
        px(SAFE_BOTTOM),
        |_, _| {},
    );
    states()
        .columns(2)
        .state("picker", phone(backdrop("Settings", cx), picker, cx))
        .state("commands", phone(backdrop("work", cx), commands, cx))
        .state(
            "choices with a line",
            phone(backdrop("macbook", cx), choices, cx),
        )
        .into_any_element()
}
