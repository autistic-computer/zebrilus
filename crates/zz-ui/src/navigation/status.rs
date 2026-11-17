use std::rc::Rc;

mod pane_deck;
pub(crate) use pane_deck::front_pane;
pub use pane_deck::{StatusPaneEntry, status_pane_deck};

use ala::{AnyElement, App, ElementId, IntoElement, MouseButton, SharedString, Window, prelude::*};

use crate::{
    Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonLook, ButtonVariants as _},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
};

use super::{
    WorkspaceStatusWindowState,
    tab::{TabClose, TabReorder},
    workspace_status_window,
};

pub use zz_client::{MAX_VISIBLE_WINDOWS, visible_window_range};

pub type StatusAction = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct StatusWindowActions {
    pub select: StatusAction,
    pub close: Option<StatusAction>,
    pub rename: Option<(SharedString, StatusAction)>,
    pub reorder: Option<TabReorder>,
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
    mut actions: StatusWindowActions,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let tooltip = name.clone();
    let close = TabClose {
        id: format!("status-window-close-{id:?}").into(),
        tooltip: "Close window",
        action: actions.close.clone(),
    };
    let reorder = actions.reorder.take();
    let item = workspace_status_window(id, name, tooltip, state, panes, Some(close), reorder, cx)
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
    tab_overflow(id, "All windows", IconName::AppWindow, windows, connected)
}

pub fn tab_overflow(
    id: impl Into<ElementId>,
    tooltip: &'static str,
    icon: IconName,
    entries: Vec<StatusWindowEntry>,
    connected: bool,
) -> AnyElement {
    Button::new(id)
        .ghost()
        .small()
        .compact()
        .icon(IconName::Ellipsis)
        .look(ButtonLook::Chrome)
        .tooltip(tooltip)
        .disabled(!connected)
        .dropdown_menu(move |menu, _, _| {
            entries.iter().fold(menu, |menu, entry| {
                let select = entry.select.clone();
                menu.item(
                    PopupMenuItem::new(entry.label.clone())
                        .icon(if entry.active {
                            IconName::Check
                        } else {
                            icon.clone()
                        })
                        .on_click(move |_, window, cx| select(window, cx)),
                )
            })
        })
        .into_any_element()
}
