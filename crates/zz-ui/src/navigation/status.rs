use std::rc::Rc;

mod pane_deck;
pub use pane_deck::{StatusPaneEntry, status_pane_deck};

use ala::{
    AnyElement, App, ElementId, IntoElement, MouseButton, SharedString, Window, div, prelude::*,
};

use crate::{
    ActiveTheme as _, Disableable as _, IconName, PaintState, Sizable as _,
    button::{Button, ButtonLook, ButtonVariants as _},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
};
use penna::TabLook;

use super::{WorkspaceStatusWindowState, workspace_row_action, workspace_status_window};

pub use zz_client::{MAX_VISIBLE_WINDOWS, visible_window_range};

const CLOSE_SIDE: f32 =
    super::WORKSPACE_STATUS_TAB_HEIGHT.as_f32() - 2.0 * crate::button::INSIDE_GAP.as_f32();
const CLOSE_SLOP: f32 = (crate::touch::TOUCH_TARGET - CLOSE_SIDE) / 2.0;
pub type StatusAction = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct StatusWindowActions {
    pub select: StatusAction,
    pub close: Option<StatusAction>,
    pub rename: Option<(SharedString, StatusAction)>,
}

pub struct StatusWindowEntry {
    pub label: SharedString,
    pub active: bool,
    pub select: StatusAction,
}

pub fn status_window(
    id: impl Into<ElementId>,
    name: SharedString,
    state: WorkspaceStatusWindowState,
    panes: Vec<StatusPaneEntry>,
    actions: StatusWindowActions,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let group: SharedString = format!("status-window-{id:?}").into();
    let close_button = actions.close.clone();
    let tooltip = name.clone();
    let tab = penna::Surface::from(TabLook::Window)
        .on(super::workspace_status_tab_ground(state.alone, cx));
    let ground = cx.theme().ground_at(tab, state.paint_state());
    let hovered = cx.theme().ground_at(tab, PaintState::Hover);
    let hovers = state.connected && !state.active;
    let deck = status_pane_deck(
        ElementId::Name(format!("status-window-panes-{id:?}").into()),
        panes,
        state.connected,
        ground,
        cx,
    );
    let item = workspace_status_window(id.clone(), name, tooltip, state, deck, cx)
        .group(group.clone())
        .pr(crate::button::INSIDE_GAP)
        .child(
            div()
                .flex_none()
                .invisible()
                .group_hover(group, ala::Styled::visible)
                .child(
                    workspace_row_action(
                        Button::new(ElementId::Name(
                            format!("status-window-close-{id:?}").into(),
                        ))
                        .icon(IconName::Xmark)
                        .inside(tab, super::workspace_status_tab_height(state.alone))
                        .hit_slop(CLOSE_SLOP, CLOSE_SLOP)
                        .tooltip("Close window"),
                        if hovers { hovered } else { ground },
                        !state.connected || close_button.is_none(),
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        if let Some(close) = &close_button {
                            close(window, cx);
                        }
                    }),
                ),
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if state.connected {
                (actions.select)(window, cx);
            }
        });
    if !state.connected || actions.rename.is_none() && actions.close.is_none() {
        return item.into_any_element();
    }
    item.context_menu(move |mut menu, _, _| {
        if let Some((label, rename)) = &actions.rename {
            let rename = rename.clone();
            menu = menu.item(
                PopupMenuItem::new(label.clone()).on_click(move |_, window, cx| rename(window, cx)),
            );
        }
        if let Some(close) = &actions.close {
            let close = close.clone();
            menu = menu.item(
                PopupMenuItem::new("Close Window")
                    .icon(IconName::Xmark)
                    .on_click(move |_, window, cx| close(window, cx)),
            );
        }
        menu
    })
    .into_any_element()
}

pub fn status_window_overflow(
    id: impl Into<ElementId>,
    windows: Vec<StatusWindowEntry>,
    connected: bool,
    _: &App,
) -> AnyElement {
    Button::new(id)
        .ghost()
        .small()
        .compact()
        .icon(IconName::Ellipsis)
        .look(ButtonLook::Chrome)
        .tooltip("All windows")
        .disabled(!connected)
        .dropdown_menu(move |menu, _, _| {
            windows.iter().fold(menu, |menu, entry| {
                let select = entry.select.clone();
                menu.item(
                    PopupMenuItem::new(entry.label.clone())
                        .icon(if entry.active {
                            IconName::Check
                        } else {
                            IconName::AppWindow
                        })
                        .on_click(move |_, window, cx| select(window, cx)),
                )
            })
        })
        .into_any_element()
}
