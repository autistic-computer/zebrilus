//! The agent's session history picker: a directory column beside the
//! sessions run there, as both the desktop and the thin clients draw it.

use zz_gpui::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, SharedString, Stateful,
    UniformListScrollHandle, div, prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Edge, ElementExt as _, Icon, IconName, Kind,
    StyledExt as _, Surface, Tone,
    button::Button,
    command::palette_shortcut_hint,
    h_flex,
    input::InputState,
    picker::{
        directory_row, muted_on, picker_empty, picker_footer, picker_header, picker_list,
        picker_modal_sized, picker_overlay, picker_row, picker_search,
    },
    roles,
    scroll::{GUTTER_WIDTH, ScrollableElement as _},
    v_flex,
};

const ROW_HEIGHT: f32 = 26.0;
const COMPACT_WIDTH: f32 = 560.0;

/// One session: its title, a check on the open one, and when it was last
/// used, or the button that deletes it while it is the selected row.
pub fn history_session_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    updated_at: Option<SharedString>,
    selected: bool,
    current: bool,
    delete: Option<Button>,
    cx: &App,
) -> Stateful<Div> {
    let row = cx
        .theme()
        .layer(Kind::Row, crate::picker::row_state(selected, true));
    let detail = muted_on(&row, cx);
    let deletable = delete.is_some();
    picker_row(id, selected, cx)
        .h(px(ROW_HEIGHT))
        .px_2()
        .text_size(crate::rems_from_px(12.0))
        .line_height(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(title.into()),
        )
        .when(current, |row| {
            row.child(Icon::new(IconName::Check).size(px(12.0)).text_color(detail))
        })
        .when(!deletable, |row| {
            row.when_some(updated_at, |row, timestamp| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(crate::rems_from_px(11.0))
                        .text_color(detail)
                        .child(timestamp),
                )
            })
        })
        .children(delete.map(|delete| {
            delete
                .on(row.ground)
                .icon(IconName::Xmark)
                .label("Delete")
                .tooltip("Delete this session")
        }))
}

/// A heading over a group of directories: the recent ones, then the rest.
pub fn history_directory_heading(index: usize, recent: bool, cx: &App) -> Div {
    div()
        .debug_selector(move || format!("agent-project-header-{index}"))
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .px_2p5()
        .text_size(crate::rems_from_px(11.0))
        .line_height(px(16.0))
        .text_color(cx.theme().foreground.muted())
        .child(if recent { "Recent" } else { "All directories" })
}

/// A directory and how many sessions ran in it.
pub fn history_directory_row(
    index: usize,
    label: impl Into<SharedString>,
    sessions: usize,
    highlighted: bool,
    cx: &App,
) -> Stateful<Div> {
    let count = muted_on(
        &cx.theme()
            .layer(Kind::Row, crate::picker::row_state(highlighted, true)),
        cx,
    );
    directory_row(("agent-project-directory", index), label, highlighted, cx)
        .h(px(ROW_HEIGHT))
        .px_2()
        .line_height(px(16.0))
        .debug_selector(move || format!("agent-project-row-{index}"))
        .when(sessions > 0, |row| {
            row.child(
                div()
                    .flex_none()
                    .text_size(crate::rems_from_px(11.0))
                    .text_color(count)
                    .child(sessions.to_string()),
            )
        })
}

/// The directory column, ruled off from the sessions beside or under it.
pub fn history_directories(
    compact: bool,
    list: impl IntoElement,
    scroll: &UniformListScrollHandle,
    notes: Option<AnyElement>,
    cx: &App,
) -> Div {
    v_flex()
        .flex_none()
        .min_w_0()
        .min_h_0()
        .when(compact, |column| {
            column
                .w_full()
                .h(relative(0.35))
                .rule_on(roles::BAR_RULE, Edge::Bottom, cx)
        })
        .when(!compact, |column| {
            column
                .w(relative(0.36))
                .rule_on(roles::BAR_RULE, Edge::Right, cx)
        })
        .list_inset(cx)
        .py(px(CHROME_GAP))
        .pr_0()
        .child(
            v_flex()
                .relative()
                .debug_selector(|| "agent-project-list".into())
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .pr(GUTTER_WIDTH)
                .child(list)
                .vertical_scrollbar(scroll),
        )
        .children(notes)
}

/// The keys the picker answers, for its footer.
pub fn history_hints(can_delete: bool) -> Div {
    h_flex()
        .items_center()
        .flex_wrap()
        .gap(px(CHROME_GAP))
        .child(palette_shortcut_hint(["up", "down"], "select"))
        .child(palette_shortcut_hint(["enter"], "open"))
        .child(palette_shortcut_hint(["tab"], "column"))
        .when(can_delete, |row| {
            row.child(palette_shortcut_hint(["delete"], "delete"))
        })
        .child(palette_shortcut_hint(["escape"], "close"))
}

/// The confirmation that takes the footer's place before a session is
/// deleted, in the danger tone.
pub fn history_delete_banner(compact: bool, cancel: Button, confirm: Button, cx: &App) -> Div {
    let notice = Surface::new(roles::PICKER_NOTICE)
        .tone(Tone::Danger)
        .on(cx.theme().ground_of(roles::PICKER));
    let ground = cx.theme().ground_of(notice);
    picker_footer(cx)
        .min_h(px(52.0))
        .justify_between()
        .flex_wrap()
        .surface(notice, cx)
        .rule_on(roles::BAR_RULE, Edge::Top, cx)
        .text_size(crate::rems_from_px(11.0))
        .text_color(cx.theme().foreground)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .when(compact, |this| this.w_full().flex_none())
                .child("Permanently delete this session from the agent’s local store?"),
        )
        .child(
            h_flex()
                .flex_none()
                .gap(px(CHROME_GAP))
                .child(cancel.on(ground).label("Cancel"))
                .child(confirm.on(ground).label("Delete")),
        )
}

/// The history picker over an agent pane: the search, the directory column,
/// the sessions in the chosen directory with its path and an optional
/// refresh over them, and `footer` under it all. `on_layout` hears whether
/// the picker is too narrow for two columns.
pub struct HistoryPicker {
    pub pane: u64,
    pub input: Entity<InputState>,
    pub compact: bool,
    pub on_layout: Box<dyn Fn(bool, &mut App)>,
    pub directories: AnyElement,
    pub directory: SharedString,
    pub refresh: Option<Button>,
    pub sessions: AnyElement,
    pub sessions_scroll: UniformListScrollHandle,
    pub empty: Option<SharedString>,
    pub footer: AnyElement,
}

impl HistoryPicker {
    #[must_use]
    pub fn render(self, cx: &App) -> Stateful<Div> {
        let on_layout = self.on_layout;
        picker_overlay(("agent-history-overlay", self.pane), cx)
            .p_2()
            .child(
                picker_modal_sized(("agent-history-modal", self.pane), 920.0, cx)
                    .map_element(|modal| modal.w_full().min_w_0().min_h_0())
                    .child(picker_header(cx).child(picker_search(&self.input, cx)))
                    .child(
                        h_flex()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .items_stretch()
                            .when(self.compact, zz_gpui::Styled::flex_col)
                            .on_prepaint(move |bounds, _, cx| {
                                on_layout(bounds.size.width < px(COMPACT_WIDTH), cx);
                            })
                            .child(self.directories)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .pt(px(CHROME_GAP))
                                    .child(
                                        h_flex()
                                            .debug_selector(|| "agent-history-header".into())
                                            .h(px(ROW_HEIGHT))
                                            .flex_none()
                                            .gap_2()
                                            .px_3()
                                            .text_size(crate::rems_from_px(11.0))
                                            .line_height(px(16.0))
                                            .text_color(cx.theme().foreground.muted())
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .whitespace_nowrap()
                                                    .child(self.directory),
                                            )
                                            .children(self.refresh.map(|refresh| {
                                                refresh.icon(IconName::Redo2).label("Refresh")
                                            })),
                                    )
                                    .child(
                                        picker_list()
                                            .list_inset(cx)
                                            .pb(px(CHROME_GAP))
                                            .pt_0()
                                            .pr_0()
                                            .child(
                                                v_flex()
                                                    .relative()
                                                    .debug_selector(|| "agent-history-list".into())
                                                    .flex_1()
                                                    .min_h_0()
                                                    .overflow_hidden()
                                                    .pr(GUTTER_WIDTH)
                                                    .child(self.sessions)
                                                    .vertical_scrollbar(&self.sessions_scroll),
                                            )
                                            .children(
                                                self.empty.map(|empty| picker_empty(empty, cx)),
                                            ),
                                    ),
                            ),
                    )
                    .child(self.footer),
            )
    }
}
