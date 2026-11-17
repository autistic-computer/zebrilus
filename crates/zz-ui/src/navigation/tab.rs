use std::rc::Rc;

use ala::{
    AnyElement, App, ElementId, IntoElement, Pixels, SharedString, Window, div, prelude::*, px,
};
use penna::{Tab, TabLook};

use crate::{
    ActiveTheme as _, Ground, IconName, PaintState, Surface,
    button::{Button, INSIDE_GAP},
};

use super::{
    WORKSPACE_STATUS_TAB_HEIGHT, WorkspaceStatusWindowState, status::StatusAction,
    workspace_row_action, workspace_status_tab_ground, workspace_status_tab_height,
};

pub const TAB_INSET: Pixels = px(9.0);
pub const TAB_GAP: Pixels = px(5.0);
pub const TAB_ICON: Pixels = px(14.0);
const CLOSE_SIDE: Pixels = px(WORKSPACE_STATUS_TAB_HEIGHT.as_f32() - 2.0 * INSIDE_GAP.as_f32());
const CLOSE_SLOP: f32 = (crate::touch::TOUCH_TARGET - CLOSE_SIDE.as_f32()) / 2.0;

pub type TabFace = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub struct TabFaces {
    pub lead: Pixels,
    pub full: Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>,
    pub icon: TabFace,
}

pub struct TabClose {
    pub id: SharedString,
    pub tooltip: &'static str,
    pub action: Option<StatusAction>,
}

#[must_use]
pub fn compact_below(lead: Pixels, closable: bool) -> Pixels {
    let trailing = if closable {
        TAB_GAP + CLOSE_SIDE + INSIDE_GAP
    } else {
        TAB_INSET
    };
    TAB_INSET + lead + TAB_GAP + crate::LABEL_FADE + trailing
}

#[must_use]
pub fn tab_faces(
    tab: Tab,
    look: TabLook,
    on: Ground,
    state: WorkspaceStatusWindowState,
    faces: TabFaces,
    close: Option<TabClose>,
) -> Tab {
    let TabFaces { lead, full, icon } = faces;
    let compact = compact_below(lead, close.is_some());
    let group = close
        .as_ref()
        .map(|close| SharedString::from(format!("{}-tab", close.id)));
    let hover = group.clone();
    tab.px_0()
        .gap_0()
        .when_some(group, ala::InteractiveElement::group)
        .child(crate::pane::by_width(move |width, window, cx| {
            if width < compact {
                compact_face(close, &icon, look, on, state, window, cx)
            } else {
                full_face(full, close, hover, look, on, state, window, cx)
            }
        }))
}

fn compact_face(
    close: Option<TabClose>,
    icon: &TabFace,
    look: TabLook,
    on: Ground,
    state: WorkspaceStatusWindowState,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let face = match close.filter(|_| state.active) {
        Some(close) => close_button(close, look, on, state, cx).into_any_element(),
        None => icon(window, cx),
    };
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(face)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn full_face(
    full: Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>,
    close: Option<TabClose>,
    group: Option<SharedString>,
    look: TabLook,
    on: Ground,
    state: WorkspaceStatusWindowState,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let closable = close.is_some();
    let close = close.zip(group).map(|(close, group)| {
        div()
            .flex_none()
            .invisible()
            .group_hover(group, ala::Styled::visible)
            .child(close_button(close, look, on, state, cx))
    });
    div()
        .size_full()
        .flex()
        .items_center()
        .gap(TAB_GAP)
        .pl(TAB_INSET)
        .pr(if closable { INSIDE_GAP } else { TAB_INSET })
        .child(full(window, cx))
        .children(close)
        .into_any_element()
}

fn close_button(
    close: TabClose,
    look: TabLook,
    on: Ground,
    state: WorkspaceStatusWindowState,
    cx: &App,
) -> Button {
    let TabClose {
        id,
        tooltip,
        action,
    } = close;
    let surface = Surface::from(look).on(workspace_status_tab_ground(on, state.alone, cx));
    let ground = if state.connected && !state.active {
        cx.theme().ground_at(surface, PaintState::Hover)
    } else {
        cx.theme().ground_at(surface, state.paint_state())
    };
    let disabled = !state.connected || action.is_none();
    let selector = id.clone();
    workspace_row_action(
        Button::new(ElementId::Name(id))
            .icon(IconName::Xmark)
            .inside(surface, workspace_status_tab_height(state.alone))
            .hit_slop(CLOSE_SLOP, CLOSE_SLOP)
            .tooltip(tooltip),
        ground,
        disabled,
        cx,
    )
    .debug_selector(move || selector.to_string())
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        if let Some(action) = &action {
            action(window, cx);
        }
    })
}
