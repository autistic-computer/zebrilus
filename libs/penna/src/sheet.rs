use std::rc::Rc;

use ala::{
    Animation, AnimationExt as _, AnyElement, App, Context, DispatchPhase, Div, ElementId, Global,
    HitboxBehavior, IntoElement, MouseButton, ParentElement as _, Pixels, Point, RenderOnce,
    ScrollHandle, ScrollWheelEvent, SharedString, Stateful, Styled, TouchPhase, Window, anchored,
    canvas, deferred, div, ease_out_quint, prelude::*, px, relative,
};
use web_time::{Duration, Instant};

use crate::dismissal::{Dismissal, Overdrag, Tick, coast, coasting};
use crate::{
    ActiveTheme as _, Emphasis, Icon, IconName, Layer, PaintState, Shape, StyledExt as _, Surface,
    SurfaceRole, Tone,
    button::Button,
    foundation::{muted_on, row_radius, row_surface},
    h_flex, roles,
};

const ENTER: Duration = Duration::from_millis(240);
const GRABBER_WIDTH: f32 = 36.0;
const GRABBER_HEIGHT: f32 = 5.0;
const HEADER_HEIGHT: f32 = 60.0;
const PADDING_X: f32 = 20.0;
const CLOSE: f32 = 36.0;
const OPTION_HEIGHT: f32 = 52.0;
const DETAIL_HEIGHT: f32 = 64.0;
const TILE: f32 = 40.0;
const TILE_ROW_HEIGHT: f32 = 56.0;
const TILE_ROUNDING: f32 = 0.28;
const MAX_HEIGHT: f32 = 0.85;
const FLOAT_MARGIN: f32 = 8.0;
const RISE: f32 = 0.3;
const FLING_HEIGHTS: f32 = 2.0;
const COMMIT_FRACTION: f32 = 0.48;
const SHADOW_ROOM: f32 = 32.0;

type Dismiss = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Default)]
struct ClosingSheets(Vec<ElementId>);

impl Global for ClosingSheets {}

pub fn close_sheet(id: impl Into<ElementId>, cx: &mut App) {
    let id = id.into();
    let closing = &mut cx.default_global::<ClosingSheets>().0;
    if !closing.contains(&id) {
        closing.push(id);
    }
    cx.refresh_windows();
}

fn take_close(id: &ElementId, cx: &mut App) -> bool {
    let Some(closing) = cx.try_global::<ClosingSheets>() else {
        return false;
    };
    if !closing.0.contains(id) {
        return false;
    }
    cx.global_mut::<ClosingSheets>()
        .0
        .retain(|closing| closing != id);
    true
}

#[derive(IntoElement)]
pub struct BottomSheet {
    id: ElementId,
    title: SharedString,
    actions: Vec<AnyElement>,
    content: AnyElement,
    bottom_inset: Pixels,
    on_dismiss: Dismiss,
    content_scroll: Option<ScrollHandle>,
    surface_role: Option<SurfaceRole>,
    heading: Option<AnyElement>,
    full: Option<Pixels>,
}

pub fn bottom_sheet(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    actions: impl IntoIterator<Item = AnyElement>,
    content: impl IntoElement,
    bottom_inset: Pixels,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> BottomSheet {
    BottomSheet {
        id: id.into(),
        title: title.into(),
        actions: actions.into_iter().collect(),
        content: content.into_any_element(),
        bottom_inset,
        on_dismiss: Rc::new(on_dismiss),
        content_scroll: None,
        surface_role: None,
        heading: None,
        full: None,
    }
}

impl BottomSheet {
    #[must_use]
    pub fn content_scroll(mut self, handle: ScrollHandle) -> Self {
        self.content_scroll = Some(handle);
        self
    }

    #[must_use]
    pub fn surface_role(mut self, role: SurfaceRole) -> Self {
        self.surface_role = Some(role);
        self
    }

    #[must_use]
    pub fn heading(mut self, heading: impl IntoElement) -> Self {
        self.heading = Some(heading.into_any_element());
        self
    }

    #[must_use]
    pub fn full(mut self, top: Pixels) -> Self {
        self.full = Some(top);
        self
    }
}

#[must_use]
pub fn sheet_inset(window: &Window) -> Pixels {
    (window.visual_viewport_bounds().bottom() - window.fully_visible_bounds().bottom()).max(px(0.0))
}

pub fn floating_sheet(sheet: impl IntoElement, window: &Window) -> AnyElement {
    let area = window.visual_viewport_bounds();
    deferred(
        anchored().position(area.origin).child(
            div()
                .relative()
                .w(area.size.width)
                .h(area.size.height)
                .child(sheet),
        ),
    )
    .with_priority(2)
    .into_any_element()
}

pub fn sheet_close(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .compact()
        .tab_stop(false)
        .size(px(CLOSE))
        .rounded_full()
        .child(Icon::new(IconName::Xmark).size(px(16.0)))
}

pub fn sheet_option(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    cx: &App,
) -> Stateful<Div> {
    option_row(
        id,
        SurfaceRole::MENU_ITEM,
        Lead::Check(checked),
        label.into(),
        cx,
    )
}

pub fn sheet_action(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    option_row(
        id,
        SurfaceRole::MENU_ITEM,
        Lead::Tile(Box::new(Icon::new(icon))),
        label.into(),
        cx,
    )
}

pub fn sheet_row(
    id: impl Into<ElementId>,
    slot: Option<Icon>,
    label: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    option_row(
        id,
        roles::PHONE_ROW,
        slot.map_or(Lead::Empty, |icon| Lead::Tile(Box::new(icon))),
        label,
        cx,
    )
}

pub fn sheet_detail_row(
    id: impl Into<ElementId>,
    role: SurfaceRole,
    slot: Option<Icon>,
    label: impl Into<SharedString>,
    detail: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let layer = cx.theme().layer(role, PaintState::Rest);
    let muted = muted_on(layer.ground, cx);
    row_shell(id, role, px(DETAIL_HEIGHT), cx)
        .children(slot.map(|icon| icon_tile(icon, TILE, &layer, cx)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(16.0))
                        .line_height(px(21.0))
                        .child(label.into()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .line_height(px(17.0))
                        .text_color(muted)
                        .child(detail),
                ),
        )
}

enum Lead {
    Empty,
    Check(bool),
    Tile(Box<Icon>),
}

fn option_row(
    id: impl Into<ElementId>,
    role: SurfaceRole,
    lead: Lead,
    label: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let layer = cx.theme().layer(role, PaintState::Rest);
    let height = if matches!(lead, Lead::Tile(_)) {
        TILE_ROW_HEIGHT
    } else {
        OPTION_HEIGHT
    };
    let lead = match lead {
        Lead::Empty => None,
        Lead::Check(checked) => Some(
            div()
                .flex()
                .flex_none()
                .w(px(20.0))
                .children(checked.then(|| Icon::new(IconName::Check).size(px(18.0))))
                .into_any_element(),
        ),
        Lead::Tile(icon) => Some(icon_tile(*icon, TILE, &layer, cx).into_any_element()),
    };
    row_shell(id, role, px(height), cx).children(lead).child(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(px(16.0))
            .child(label),
    )
}

fn row_shell(
    id: impl Into<ElementId>,
    role: SurfaceRole,
    height: Pixels,
    cx: &App,
) -> Stateful<Div> {
    let id = id.into();
    let layer = cx.theme().layer(role, PaintState::Rest);
    row_surface(
        h_flex().id(id.clone()).flex_none().h(height),
        role,
        PaintState::Rest,
        height,
        cx,
    )
    .relative()
    .w_full()
    .gap(px(12.0))
    .px(px(12.0))
    .cursor_pointer()
    .child(crate::touch::instant_press_highlight(
        id,
        cx.theme().layer(role, PaintState::Active).fill,
        crate::touch::PressCorners::Row(row_radius(&layer, height)),
    ))
}

fn icon_tile(icon: Icon, size: f32, layer: &Layer, cx: &App) -> Div {
    let theme = cx.theme();
    let role = SurfaceRole::ICON_TILE;
    let tile = theme.layer(Surface::new(role).on(layer.ground), PaintState::Rest);
    let radius = match theme.style(role).shape {
        Shape::Square => px(0.0),
        Shape::Round => px(size * TILE_ROUNDING),
        Shape::Pill => px(size / 2.0),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .layer(&tile)
        .rounded(radius)
        .child(icon.size(px(size / 2.0)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Idle,
    Pending(Point<Pixels>),
    Content(Point<Pixels>),
    Sheet(Point<Pixels>),
}

struct SheetState {
    dismissal: Dismissal,
    scroll: ScrollHandle,
    content_scroll: Option<ScrollHandle>,
    height: f32,
    extent: f32,
    gesture: Gesture,
}

impl SheetState {
    fn content(&self) -> &ScrollHandle {
        self.content_scroll.as_ref().unwrap_or(&self.scroll)
    }

    fn at_top(&self) -> bool {
        self.content().offset().y >= px(-0.5)
    }

    fn claim(&mut self, start: Point<Pixels>, delta: f32, now: Instant) {
        self.dismissal.grab(self.extent);
        self.dismissal.drag(delta, now);
        self.gesture = Gesture::Sheet(start);
    }

    fn begin(&mut self, start: Point<Pixels>, delta: f32, now: Instant) -> bool {
        if start.y < self.content().bounds().top() || delta > 0.0 && self.at_top() {
            self.claim(start, delta, now);
            true
        } else {
            self.gesture = Gesture::Content(start);
            false
        }
    }

    fn pan(
        &mut self,
        event: &ScrollWheelEvent,
        delta: f32,
        hovered: bool,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> bool {
        let start = event.position;
        let claimed = match (event.touch_phase, self.gesture) {
            (TouchPhase::Started, _) => {
                self.gesture = Gesture::Idle;
                if !hovered {
                    false
                } else if delta == 0.0 {
                    self.gesture = Gesture::Pending(start);
                    false
                } else {
                    self.begin(start, delta, now)
                }
            }
            (TouchPhase::Moved, Gesture::Pending(origin)) if origin == start && delta != 0.0 => {
                self.begin(start, delta, now)
            }
            (TouchPhase::Moved, Gesture::Sheet(origin)) if origin == start => {
                self.dismissal.drag(delta, now);
                true
            }
            (TouchPhase::Moved, Gesture::Content(origin))
                if origin == start && delta > 0.0 && self.at_top() =>
            {
                self.claim(start, delta, now);
                true
            }
            (TouchPhase::Ended, Gesture::Sheet(origin)) if origin == start => {
                self.gesture = Gesture::Idle;
                self.dismissal.drag(delta, now);
                self.dismissal.release(
                    FLING_HEIGHTS * self.height,
                    COMMIT_FRACTION * self.height / self.extent.max(1.0),
                    now,
                );
                coast(start, cx);
                true
            }
            (TouchPhase::Cancelled, Gesture::Sheet(origin)) if origin == start => {
                self.gesture = Gesture::Idle;
                self.dismissal.cancel(now);
                true
            }
            (TouchPhase::Ended | TouchPhase::Cancelled, _) => {
                self.gesture = Gesture::Idle;
                false
            }
            _ => false,
        };
        if claimed {
            cx.notify();
        }
        claimed
    }
}

impl RenderOnce for BottomSheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| {
            take_close(&id, cx);
            SheetState {
                dismissal: Dismissal::new(Overdrag::Band),
                scroll: ScrollHandle::new(),
                content_scroll: None,
                height: 0.0,
                extent: 0.0,
                gesture: Gesture::Idle,
            }
        });
        let now = cx.background_executor().now();
        let content_scroll = self.content_scroll;
        let reduce_motion = cx.reduce_motion();
        let closing = take_close(&self.id, cx);
        let tick = state.update(cx, |state, _| {
            state.content_scroll = content_scroll;
            if closing {
                state.dismissal.leave(state.extent, now);
            }
            if reduce_motion {
                state.dismissal.finish();
            }
            state.dismissal.tick(now)
        });
        match tick {
            Tick::Moving => window.request_animation_frame(),
            Tick::Dismissed => {
                let on_dismiss = Rc::clone(&self.on_dismiss);
                window.defer(cx, move |window, cx| on_dismiss(window, cx));
            }
            Tick::Still => {}
        }
        let (offset, fade, scroll) = {
            let state = state.read(cx);
            (
                px(state.dismissal.offset()),
                1.0 - state.dismissal.progress(),
                state.scroll.clone(),
            )
        };
        let theme = cx.theme();
        let enter = ENTER;
        let margin = px(FLOAT_MARGIN);
        let rest = self.bottom_inset.max(margin);
        let full = self.full;
        let sheet = self.surface_role.unwrap_or(SurfaceRole::DRAWER);
        let ground = theme.ground_of(sheet);
        let grabber = Layer {
            fill: theme
                .paint_on(ground, Tone::Neutral, Emphasis::Outline)
                .border,
            glass: None,
            ..theme.layer(
                Surface::new(SurfaceRole::GRABBER).on(ground),
                PaintState::Rest,
            )
        };
        let header = Surface::new(roles::PHONE_BAR).on(ground);
        let close = state.clone();
        let scrim = div()
            .id("bottom-sheet-scrim")
            .debug_selector(|| "bottom-sheet-scrim".to_owned())
            .absolute()
            .inset_0()
            .bg(theme.scrim)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_click(move |_, _, cx| {
                let now = cx.background_executor().now();
                close.update(cx, |state, cx| {
                    state.dismissal.leave(state.extent, now);
                    cx.notify();
                });
            })
            .with_animation(
                "bottom-sheet-scrim-enter",
                Animation::new(enter).with_easing(ease_out_quint()),
                move |scrim, delta| scrim.opacity(delta * fade),
            );
        let listener = canvas(
            {
                let state = state.clone();
                move |bounds, window, cx| {
                    state.update(cx, |state, _| {
                        state.height = f32::from(bounds.size.height);
                        state.extent = state.height + f32::from(rest) + SHADOW_ROOM;
                    });
                    window.insert_hitbox(bounds, HitboxBehavior::Normal)
                }
            },
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    if coasting(event, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    let hovered = hitbox.should_handle_scroll(window);
                    let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
                    let now = cx.background_executor().now();
                    if state.update(cx, |state, cx| state.pan(event, delta, hovered, now, cx)) {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .inset_0();
        let panel = div()
            .id("bottom-sheet-panel")
            .debug_selector(|| "bottom-sheet-panel".to_owned())
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .map(|panel| match full {
                Some(_) => panel.top_0(),
                None => panel.max_h(relative(MAX_HEIGHT)),
            })
            .flex()
            .flex_col()
            .occlude()
            .surface(sheet, cx)
            .font_family(theme.font_family.clone())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(listener)
            .child(
                div().flex().flex_none().justify_center().pt(px(6.0)).child(
                    div()
                        .w(px(GRABBER_WIDTH))
                        .h(px(GRABBER_HEIGHT))
                        .layer(&grabber),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(HEADER_HEIGHT))
                    .pl(px(PADDING_X))
                    .pr(px(14.0))
                    .surface(header, cx)
                    .child(self.heading.unwrap_or_else(|| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(24.0))
                            .line_height(px(30.0))
                            .font_bold()
                            .child(self.title)
                            .into_any_element()
                    }))
                    .children(self.actions),
            )
            .child(match full {
                Some(_) => div()
                    .id("bottom-sheet-content")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(self.content),
                None => div()
                    .id("bottom-sheet-content")
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w_full()
                            .child(self.content),
                    ),
            })
            .with_animation(
                "bottom-sheet-panel-enter",
                Animation::new(enter).with_easing(ease_out_quint()),
                move |panel, delta| {
                    let rise = RISE * (1.0 - delta);
                    let panel = panel.bottom(relative(-rise));
                    if full.is_some() {
                        panel.top(relative(rise))
                    } else {
                        panel
                    }
                },
            );
        div()
            .id(self.id)
            .absolute()
            .inset_0()
            .occlude()
            .child(scrim)
            .child(
                div()
                    .absolute()
                    .left(margin)
                    .right(margin)
                    .top(full.unwrap_or_default() + offset)
                    .bottom(rest - offset)
                    .child(panel),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use ala::{
        Bounds, Context, Modifiers, Render, ScrollDelta, TestAppContext, VisualTestContext, point,
    };

    use super::*;

    struct Host {
        dismissed: Rc<Cell<usize>>,
        body: Pixels,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let dismissed = Rc::clone(&self.dismissed);
            div().size_full().child(bottom_sheet(
                "sheet",
                "Panes",
                Vec::new(),
                div()
                    .debug_selector(|| "sheet-body".to_owned())
                    .h(self.body)
                    .w_full(),
                px(20.0),
                move |_, _| dismissed.set(dismissed.get() + 1),
            ))
        }
    }

    fn host(body: f32, cx: &mut TestAppContext) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let dismissed = Rc::new(Cell::new(0));
        let count = Rc::clone(&dismissed);
        let (_, cx) = cx.add_window_view(move |_, _| Host {
            dismissed: count,
            body: px(body),
        });
        redraw(cx);
        (dismissed, cx)
    }

    fn redraw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn panel(cx: &mut VisualTestContext) -> Bounds<Pixels> {
        cx.debug_bounds("bottom-sheet-panel").expect("panel")
    }

    fn pan(cx: &mut VisualTestContext, at: Point<Pixels>, steps: &[(u64, TouchPhase, f32)]) {
        for (wait, phase, dy) in steps {
            cx.executor().advance_clock(Duration::from_millis(*wait));
            cx.simulate_event(ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.0), px(*dy))),
                modifiers: Modifiers::none(),
                touch_phase: *phase,
            });
            redraw(cx);
        }
    }

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..120 {
            cx.executor().advance_clock(Duration::from_millis(16));
            redraw(cx);
        }
    }

    #[ala::test]
    fn the_scrim_dismisses_and_the_panel_does_not(cx: &mut TestAppContext) {
        let (dismissed, cx) = host(120.0, cx);
        let panel = panel(cx);
        let scrim = cx.debug_bounds("bottom-sheet-scrim").expect("scrim");
        assert!(panel.top() > scrim.top());
        assert_eq!(scrim.bottom() - panel.bottom(), px(20.0));
        assert_eq!(panel.left() - scrim.left(), px(FLOAT_MARGIN));
        assert_eq!(scrim.right() - panel.right(), px(FLOAT_MARGIN));
        let body = cx.debug_bounds("sheet-body").expect("body");
        assert_eq!(body.size.height, px(120.0));

        cx.simulate_click(panel.center(), Modifiers::none());
        assert_eq!(dismissed.get(), 0);

        cx.simulate_click(
            point(scrim.center().x, scrim.top() + px(10.0)),
            Modifiers::none(),
        );
        assert_eq!(dismissed.get(), 1);
    }

    #[ala::test]
    fn a_closed_sheet_slides_away_before_it_dismisses(cx: &mut TestAppContext) {
        let (dismissed, cx) = host(120.0, cx);
        cx.update(|_, cx| cx.set_reduce_motion(false));
        let rest = panel(cx);
        cx.update(|_, cx| close_sheet("sheet", cx));
        redraw(cx);
        cx.executor().advance_clock(Duration::from_millis(48));
        redraw(cx);
        assert!(panel(cx).top() > rest.top());
        assert_eq!(dismissed.get(), 0);

        settle(cx);
        assert_eq!(dismissed.get(), 1);
    }

    #[ala::test]
    fn a_tall_sheet_stops_at_most_of_the_window(cx: &mut TestAppContext) {
        let (_, cx) = host(5000.0, cx);
        let panel = panel(cx);
        let scrim = cx.debug_bounds("bottom-sheet-scrim").expect("scrim");
        let room = scrim.size.height - px(20.0);
        assert!(panel.size.height <= room * MAX_HEIGHT + px(0.5));
        assert!(panel.size.height >= room * MAX_HEIGHT - px(0.5));
        let body = cx.debug_bounds("sheet-body").expect("body");
        assert_eq!(body.size.height, px(5000.0));
    }

    #[ala::test]
    fn a_short_drag_follows_the_finger_and_springs_back(cx: &mut TestAppContext) {
        let (dismissed, cx) = host(300.0, cx);
        let rest = panel(cx);
        let grip = point(rest.center().x, rest.top() + px(20.0));
        pan(
            cx,
            grip,
            &[
                (0, TouchPhase::Started, 30.0),
                (150, TouchPhase::Moved, 30.0),
            ],
        );
        assert_eq!(panel(cx).top(), rest.top() + px(60.0));
        pan(cx, grip, &[(300, TouchPhase::Ended, 0.0)]);
        settle(cx);
        assert_eq!(panel(cx).top(), rest.top());
        assert_eq!(dismissed.get(), 0);
    }

    #[ala::test]
    fn a_drag_that_catches_a_fling_still_moves_the_sheet(cx: &mut TestAppContext) {
        let (_, cx) = host(300.0, cx);
        let rest = panel(cx);
        let grip = point(rest.center().x, rest.top() + px(20.0));
        pan(
            cx,
            grip,
            &[
                (0, TouchPhase::Started, 0.0),
                (16, TouchPhase::Moved, 30.0),
                (16, TouchPhase::Moved, 30.0),
            ],
        );
        assert_eq!(panel(cx).top(), rest.top() + px(60.0));
    }

    #[ala::test]
    fn a_long_drag_or_a_flick_dismisses(cx: &mut TestAppContext) {
        let (dismissed, cx) = host(300.0, cx);
        let rest = panel(cx);
        let grip = point(rest.center().x, rest.top() + px(20.0));
        let half = f32::from(rest.size.height) / 2.0;
        pan(
            cx,
            grip,
            &[
                (0, TouchPhase::Started, half / 2.0 + 10.0),
                (150, TouchPhase::Moved, half / 2.0 + 10.0),
                (300, TouchPhase::Ended, 0.0),
            ],
        );
        settle(cx);
        assert_eq!(dismissed.get(), 1);

        pan(
            cx,
            grip,
            &[
                (500, TouchPhase::Started, 12.0),
                (8, TouchPhase::Moved, 12.0),
                (8, TouchPhase::Ended, 0.0),
            ],
        );
        settle(cx);
        assert_eq!(dismissed.get(), 2);
    }

    #[ala::test]
    fn dragging_up_resists(cx: &mut TestAppContext) {
        let (_, cx) = host(300.0, cx);
        let rest = panel(cx);
        let grip = point(rest.center().x, rest.top() + px(20.0));
        let drag = 80.0;
        pan(cx, grip, &[(0, TouchPhase::Started, -drag)]);
        let lifted = rest.top() - panel(cx).top();
        assert!(lifted > px(0.0) && lifted < px(drag * 0.6), "{lifted:?}");
        pan(cx, grip, &[(300, TouchPhase::Ended, 0.0)]);
        settle(cx);
        assert_eq!(panel(cx).top(), rest.top());
    }

    #[ala::test]
    fn content_scrolls_first_and_hands_over_at_its_top(cx: &mut TestAppContext) {
        let (dismissed, cx) = host(5000.0, cx);
        let rest = panel(cx);
        let body = point(rest.center().x, rest.center().y);
        let top = cx.debug_bounds("sheet-body").expect("body").top();
        pan(
            cx,
            body,
            &[
                (0, TouchPhase::Started, -50.0),
                (150, TouchPhase::Ended, 0.0),
            ],
        );
        assert_eq!(panel(cx).top(), rest.top());
        let content = cx.debug_bounds("sheet-body").expect("body");
        assert_eq!(content.top(), top - px(50.0));

        pan(
            cx,
            body,
            &[
                (500, TouchPhase::Started, 40.0),
                (150, TouchPhase::Moved, 30.0),
            ],
        );
        assert_eq!(panel(cx).top(), rest.top());
        assert_eq!(cx.debug_bounds("sheet-body").expect("body").top(), top);
        pan(cx, body, &[(150, TouchPhase::Moved, 30.0)]);
        assert_eq!(panel(cx).top(), rest.top() + px(30.0));
        pan(cx, body, &[(300, TouchPhase::Ended, 0.0)]);
        settle(cx);
        assert_eq!(panel(cx).top(), rest.top());
        assert_eq!(dismissed.get(), 0);
    }
}
