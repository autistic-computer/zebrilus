use std::rc::Rc;

use ala::{
    AnyElement, App, Context, ElementId, EntityId, IntoElement, Pixels, Point, Render,
    SharedString, Window, div, prelude::*, px,
};
use penna::{Chip, ChipLook, Label, Tab, TabLook, TabStrip, TextStyle};

use crate::{
    ActiveTheme as _, Emphasis, Ground, IconName, PaintState, Surface, Tone,
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
pub type TabMove = Rc<dyn Fn(u64, usize, &mut Window, &mut App)>;

pub struct TabFaces {
    pub label: SharedString,
    pub lead: Pixels,
    pub full: Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>,
    pub icon: TabFace,
}

pub struct TabClose {
    pub id: SharedString,
    pub tooltip: &'static str,
    pub action: Option<StatusAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStripId {
    view: EntityId,
    look: TabLook,
}

impl TabStripId {
    #[must_use]
    pub const fn new(view: EntityId, look: TabLook) -> Self {
        Self { view, look }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabDrag {
    pub strip: TabStripId,
    pub id: u64,
}

pub struct TabReorder {
    pub strip: TabStripId,
    pub id: u64,
    pub index: usize,
    pub on_move: TabMove,
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
#[allow(clippy::too_many_arguments)]
pub fn tab_faces(
    tab: Tab,
    look: TabLook,
    on: Ground,
    state: WorkspaceStatusWindowState,
    faces: TabFaces,
    close: Option<TabClose>,
    reorder: Option<TabReorder>,
    cx: &App,
) -> Tab {
    let TabFaces {
        label,
        lead,
        full,
        icon,
    } = faces;
    let compact = compact_below(lead, close.is_some());
    let group = close
        .as_ref()
        .map(|close| SharedString::from(format!("{}-tab", close.id)));
    let hover = group.clone();
    let face = Rc::clone(&icon);
    let tab = tab
        .px_0()
        .gap_0()
        .when_some(group, ala::InteractiveElement::group)
        .child(crate::pane::by_width(move |width, window, cx| {
            if width < compact {
                compact_face(close, &face, look, on, state, window, cx)
            } else {
                full_face(full, close, hover, look, on, state, window, cx)
            }
        }));
    match reorder {
        Some(reorder) => reorderable(tab, reorder, label, icon, on, cx),
        None => tab,
    }
}

fn reorderable(
    tab: Tab,
    reorder: TabReorder,
    label: SharedString,
    icon: TabFace,
    on: Ground,
    cx: &App,
) -> Tab {
    let TabReorder {
        strip,
        id,
        index,
        on_move,
    } = reorder;
    let highlight = cx
        .theme()
        .paint_on(TabStrip::ground(on, 2, cx), Tone::Accent, Emphasis::Soft)
        .fill;
    let lands = move |drag: &TabDrag| drag.strip == strip && drag.id != id;
    tab.on_drag(TabDrag { strip, id }, move |_, grab, _, cx| {
        let label = label.clone();
        let icon = Rc::clone(&icon);
        cx.new(|_| TabDragPreview { label, icon, grab })
    })
    .drag_over::<TabDrag>(move |style, drag, _, _| {
        if lands(drag) {
            style.bg(highlight)
        } else {
            style
        }
    })
    .on_drop(move |drag: &TabDrag, window, cx| {
        if lands(drag) {
            on_move(drag.id, index, window, cx);
        }
    })
}

struct TabDragPreview {
    label: SharedString,
    icon: TabFace,
    grab: Point<Pixels>,
}

impl Render for TabDragPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = cx.theme().ground_of(ChipLook::Drag);
        div()
            .pl(self.grab.x + px(12.0))
            .pt(self.grab.y + px(12.0))
            .child(
                Chip::empty("tab-drag-chip")
                    .look(ChipLook::Drag)
                    .min_w_auto()
                    .h_auto()
                    .gap(TAB_GAP)
                    .max_w(super::WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH)
                    .px(px(10.0))
                    .py(px(6.0))
                    .overflow_hidden()
                    .child((self.icon)(window, cx))
                    .child(
                        Label::new(TextStyle::Small, self.label.clone())
                            .on(ground)
                            .truncate()
                            .min_w_0(),
                    ),
            )
    }
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
