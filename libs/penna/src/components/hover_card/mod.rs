use ala::{
    Anchor, AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, Role, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, anchored,
    deferred, div, prelude::FluentBuilder as _, px,
};
use web_time::{Duration, Instant};

use crate::{Space, StyledExt as _, Surface, SurfaceRole, foundation::motion::Presence, v_flex};

pub const HOVER_CARD_OPEN_DELAY: Duration = Duration::from_millis(240);
pub const HOVER_CARD_CLOSE_DELAY: Duration = Duration::from_millis(140);

#[derive(Default)]
struct HoverCardState {
    trigger_hovered: bool,
    card_hovered: bool,
    changed_at: Option<Instant>,
    presence: Presence,
}

impl HoverCardState {
    fn sync(&mut self, now: Instant) -> bool {
        let wants_open = self.trigger_hovered || self.card_hovered;
        if wants_open == self.presence.is_open() {
            self.changed_at = None;
            return false;
        }
        let delay = if wants_open {
            HOVER_CARD_OPEN_DELAY
        } else {
            HOVER_CARD_CLOSE_DELAY
        };
        let since = *self.changed_at.get_or_insert(now);
        if now.saturating_duration_since(since) >= delay {
            self.presence.set(wants_open, now);
            self.changed_at = None;
            false
        } else {
            true
        }
    }
}

#[derive(IntoElement)]
pub struct HoverCard {
    id: ElementId,
    anchor: Anchor,
    trigger: Option<AnyElement>,
    content: Option<AnyElement>,
    style: StyleRefinement,
}

impl HoverCard {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            anchor: Anchor::TopLeft,
            trigger: None,
            content: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub const fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    #[must_use]
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    #[must_use]
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl Styled for HoverCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for HoverCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| HoverCardState::default());
        let now = cx.background_executor().now();
        let pending = state.update(cx, |state, _| state.sync(now));
        if pending {
            window.request_animation_frame();
        }
        let shown = state.read(cx).presence.shown(window, cx);
        let surface = Surface::new(SurfaceRole::POPOVER);
        let trigger_state = state.clone();
        let card_state = state;

        div()
            .id(self.id.clone())
            .relative()
            .on_hover(move |hovered, window, cx| {
                trigger_state.update(cx, |state, cx| {
                    state.trigger_hovered = *hovered;
                    state.changed_at = Some(cx.background_executor().now());
                    cx.notify();
                });
                window.request_animation_frame();
            })
            .children(self.trigger)
            .when_some(shown.zip(self.content), |host, (alpha, content)| {
                host.child(
                    deferred(
                        anchored()
                            .anchor(self.anchor)
                            .snap_to_window_with_margin(px(8.0))
                            .child(
                                v_flex()
                                    .id((self.id.clone(), "card"))
                                    .role(Role::Tooltip)
                                    .mt(Space::S6)
                                    .p(Space::INSET_CARD)
                                    .gap(Space::S6)
                                    .max_w(px(300.0))
                                    .opacity(alpha)
                                    .surface(surface, cx)
                                    .refine_style(&self.style)
                                    .on_hover(move |hovered, window, cx| {
                                        card_state.update(cx, |state, cx| {
                                            state.card_hovered = *hovered;
                                            state.changed_at = Some(cx.background_executor().now());
                                            cx.notify();
                                        });
                                        window.request_animation_frame();
                                    })
                                    .child(content),
                            ),
                    )
                    .with_priority(1),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_card_waits_for_open_and_close_delays_and_stays_open_while_card_hovered() {
        let start = Instant::now();
        let mut state = HoverCardState {
            trigger_hovered: true,
            ..HoverCardState::default()
        };
        assert!(state.sync(start));
        assert!(!state.presence.is_open());
        assert!(!state.sync(start + HOVER_CARD_OPEN_DELAY));
        assert!(state.presence.is_open());

        state.trigger_hovered = false;
        state.card_hovered = true;
        assert!(!state.sync(start + Duration::from_millis(500)));
        assert!(state.presence.is_open());

        state.card_hovered = false;
        assert!(state.sync(start + Duration::from_millis(600)));
        assert!(state.presence.is_open());
        assert!(!state.sync(start + Duration::from_millis(600) + HOVER_CARD_CLOSE_DELAY));
        assert!(!state.presence.is_open());
    }
}
