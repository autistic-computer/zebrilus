use std::sync::Arc;

use ala::{App, ElementId, SharedString, px};
use zz_protocol::PaneKindSnapshot;

use crate::{
    Ground, IconName,
    navigation::{
        WORKSPACE_STATUS_PILL_HEIGHT,
        pane_stack::{PaneStack, StackSize, StackTile, stack_radius},
    },
};

use super::StatusAction;

#[derive(Clone)]
pub struct StatusPaneEntry {
    pub label: SharedString,
    pub detail: SharedString,
    pub icon: IconName,
    pub favicon: Option<Arc<[u8]>>,
    pub active: bool,
    pub select: StatusAction,
}

impl StatusPaneEntry {
    pub fn from_pane(pane: &zz_client::StatusBarPane, select: StatusAction) -> Self {
        let icon = crate::pane::pane_kind_icon(&pane.kind);
        let detail = match &pane.kind {
            PaneKindSnapshot::Terminal => format!("Terminal · {}", pane.id),
            PaneKindSnapshot::Browser(browser) => format!(
                "Browser · {} · {} tab{} · {}\n{}",
                pane.id,
                browser.tabs.len(),
                if browser.tabs.len() == 1 { "" } else { "s" },
                browser.profile,
                browser.url(),
            ),
            PaneKindSnapshot::Editor(editor) => format!(
                "Editor · {}\n{}",
                pane.id,
                editor.path.as_deref().unwrap_or(&editor.cwd),
            ),
            PaneKindSnapshot::Picker => format!("New pane · {}", pane.id),
        };
        Self {
            label: pane.label.clone().into(),
            detail: detail.into(),
            icon,
            favicon: None,
            active: pane.active,
            select,
        }
    }
}

pub fn status_pane_deck(
    id: impl Into<ElementId>,
    panes: Vec<StatusPaneEntry>,
    connected: bool,
    ground: Ground,
    cx: &App,
) -> Option<PaneStack> {
    (!panes.is_empty()).then(|| {
        let size = StackSize::Bar;
        PaneStack::new(
            id,
            focused_first(panes)
                .into_iter()
                .map(|pane| StackTile {
                    tooltip: format!(
                        "{}\n{}{}",
                        pane.label,
                        pane.detail,
                        if pane.active { "\nFocused pane" } else { "" }
                    )
                    .into(),
                    icon: pane.icon,
                    favicon: pane.favicon,
                    program: None,
                    label: pane.label,
                    select: connected.then_some(pane.select),
                })
                .collect(),
            size,
            ground,
            stack_radius(px(size.tile()), WORKSPACE_STATUS_PILL_HEIGHT, cx),
        )
    })
}

fn focused_first(mut panes: Vec<StatusPaneEntry>) -> Vec<StatusPaneEntry> {
    if let Some(active) = panes.iter().position(|pane| pane.active) {
        let focused = panes.remove(active);
        panes.insert(0, focused);
    }
    panes
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{
        Context, IntoElement, Modifiers, MouseButton, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, Window, div, prelude::*,
    };
    use std::{cell::Cell, rc::Rc};

    use crate::ActiveTheme as _;

    struct DeckTest {
        selected: Rc<Cell<Option<usize>>>,
        window_selected: Rc<Cell<bool>>,
        connected: bool,
    }

    fn entry(index: usize, active: bool, selected: Rc<Cell<Option<usize>>>) -> StatusPaneEntry {
        StatusPaneEntry {
            label: format!("Pane {index}").into(),
            detail: "Terminal".into(),
            icon: IconName::SquareTerminal,
            favicon: None,
            active,
            select: Rc::new(move |_, _| selected.set(Some(index))),
        }
    }

    impl Render for DeckTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let window_selected = self.window_selected.clone();
            let connected = self.connected;
            div()
                .id("test-window")
                .flex()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, _| {
                    if connected {
                        window_selected.set(true);
                    }
                })
                .child(
                    div()
                        .h(px(30.0))
                        .debug_selector(|| "deck".into())
                        .children(status_pane_deck(
                            "test-deck",
                            (0..6)
                                .map(|index| entry(index, index == 1, self.selected.clone()))
                                .collect(),
                            self.connected,
                            cx.theme().ground(),
                            cx,
                        )),
                )
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[ala::test]
    fn pointing_opens_every_pane_and_a_click_selects_one(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let selected = Rc::new(Cell::new(None));
        let window_selected = Rc::new(Cell::new(false));
        let (view, cx) = cx.add_window_view({
            let selected = selected.clone();
            let window_selected = window_selected.clone();
            move |_, _| DeckTest {
                selected,
                window_selected,
                connected: true,
            }
        });
        draw(cx);
        assert!(cx.debug_bounds("pane-stack-tile-0").is_none());
        let deck = cx.debug_bounds("deck").unwrap();
        cx.simulate_mouse_move(
            deck.origin + ala::point(px(4.0), px(4.0)),
            None,
            Modifiers::none(),
        );
        draw(cx);
        assert!(cx.debug_bounds("pane-stack-tile-5").is_some());
        let third = cx.debug_bounds("pane-stack-tile-2").unwrap();
        cx.simulate_click(third.center(), Modifiers::none());
        draw(cx);
        assert_eq!(selected.get(), Some(2));
        assert!(!window_selected.get());

        selected.set(None);
        view.update(cx, |view, cx| {
            view.connected = false;
            cx.notify();
        });
        draw(cx);
        let deck = cx.debug_bounds("deck").unwrap();
        cx.simulate_mouse_move(
            deck.origin + ala::point(px(4.0), px(4.0)),
            None,
            Modifiers::none(),
        );
        draw(cx);
        let third = cx.debug_bounds("pane-stack-tile-2").unwrap();
        cx.simulate_click(third.center(), Modifiers::none());
        draw(cx);
        assert_eq!(selected.get(), None);
        assert!(!window_selected.get());
    }

    #[test]
    fn the_focused_pane_leads_and_the_rest_keep_their_order() {
        let selected = Rc::new(Cell::new(None));
        let order = |active: usize| {
            focused_first(
                (0..4)
                    .map(|index| entry(index, index == active, selected.clone()))
                    .collect(),
            )
            .into_iter()
            .map(|pane| pane.label.to_string())
            .collect::<Vec<_>>()
        };
        assert_eq!(order(0), ["Pane 0", "Pane 1", "Pane 2", "Pane 3"]);
        assert_eq!(order(2), ["Pane 2", "Pane 0", "Pane 1", "Pane 3"]);
        assert_eq!(order(9), ["Pane 0", "Pane 1", "Pane 2", "Pane 3"]);
    }
}
