use std::rc::Rc;

use ala::{
    Animation, AnimationExt as _, AnyElement, App, Context, DispatchPhase, Div, ElementId, Global,
    HitboxBehavior, IntoElement, MouseButton, ParentElement as _, Pixels, Point, RenderOnce,
    ScrollHandle, ScrollWheelEvent, SharedString, Stateful, Styled, TextRun, TouchPhase, Window,
    anchored, canvas, deferred, div, ease_out_quint, prelude::*, px, relative,
};
use web_time::{Duration, Instant};

use crate::foundation::dismissal::{Dismissal, Overdrag, Tick, coast, coasting};
use crate::{
    ActiveTheme as _, Disableable as _, Emphasis, Ground, Icon, IconName, Layer, PaintState,
    Sizable as _, Size, Space, StyledExt as _, Surface, SurfaceRole, TextStyle, Tone,
    components::button::{Button, ButtonVariant, ButtonVariants as _},
    foundation::{SURFACE_ENTER_DURATION, muted_on, roles},
};

const ENTER: Duration = Duration::from_millis(240);
const GRABBER_WIDTH: f32 = 36.0;
const GRABBER_HEIGHT: f32 = 5.0;
const HEADER_HEIGHT: f32 = 44.0;
const HEADER_SIDE: f32 = 60.0;
const CLOSE: f32 = crate::blocks::touch::TOUCH_TARGET;
const DISC_GLYPH: f32 = 20.0;
const CONTENT_EDGE: f32 = 16.0;
const MODAL_PAD: f32 = 20.0;
const MODAL_GUTTER: f32 = 64.0;
const MODAL_MAX_WIDTH: f32 = 340.0;
const MODAL_TITLE: f32 = CLOSE;
const MODAL_TOP: f32 = 12.0;
const MODAL_TITLE_SIDE: f32 = 44.0;
const MODAL_CONTROL: f32 = 48.0;
const MODAL_RISE: f32 = 6.0;
const MODAL_SCRIM_LIGHT: f32 = 0.28;
const OPTION_HEIGHT: f32 = crate::blocks::touch::TOUCH_ROW_HEIGHT;
const TILE: f32 = 40.0;
const TILE_ROW_HEIGHT: f32 = 56.0;
const MAX_HEIGHT: f32 = 0.85;
const FLOAT_MARGIN: f32 = Space::S8.design_px();
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
    back: Option<AnyElement>,
    detail: Option<SharedString>,
    full: Option<Pixels>,
    cut_middle: bool,
    modal: bool,
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
        back: None,
        detail: None,
        full: None,
        cut_middle: false,
        modal: false,
    }
}

impl BottomSheet {
    #[must_use]
    pub fn content_scroll(mut self, handle: ScrollHandle) -> Self {
        self.content_scroll = Some(handle);
        self
    }

    #[must_use]
    pub fn back(mut self, back: impl IntoElement) -> Self {
        self.back = Some(back.into_any_element());
        self
    }

    #[must_use]
    pub fn modal(mut self) -> Self {
        self.modal = true;
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn cut_middle(mut self) -> Self {
        self.cut_middle = true;
        self
    }

    #[must_use]
    pub fn full(mut self, top: Pixels) -> Self {
        self.full = Some(top);
        self
    }
}

#[must_use]
pub fn sheet_ground(cx: &App) -> Ground {
    cx.theme().ground_of(Surface::new(SurfaceRole::DRAWER))
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

pub fn sheet_close(id: impl Into<ElementId>, cx: &App) -> Button {
    disc_button(id, IconName::Xmark, "Close", cx)
}

pub fn sheet_back(id: impl Into<ElementId>, cx: &App) -> Button {
    disc_button(id, IconName::ChevronLeft, "Back", cx)
}

fn disc_button(id: impl Into<ElementId>, icon: IconName, label: &'static str, cx: &App) -> Button {
    let ground = sheet_ground(cx);
    let paint = cx.theme().paint_on(ground, Tone::Neutral, Emphasis::Soft);
    Button::new(id)
        .text()
        .tab_stop(false)
        .w(px(CLOSE))
        .h(px(CLOSE))
        .tooltip(label)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(CLOSE))
                .rounded_full()
                .bg(paint.fill)
                .child(
                    Icon::new(icon)
                        .size(px(DISC_GLYPH))
                        .text_color(muted_on(ground, cx)),
                ),
        )
}

#[must_use]
pub fn sheet_groups() -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(Space::GAP_GROUP)
        .px(Space::S8)
}

#[must_use]
pub fn modal_groups() -> Div {
    sheet_groups().gap(Space::S8).pb(Space::S8)
}

#[must_use]
pub fn modal_body() -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(Space::S16)
        .px(px(MODAL_PAD))
        .pb(px(MODAL_PAD))
}

pub fn sheet_note(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .mt(-Space::S8.rems())
        .px(Space::S16)
        .pb(Space::S4)
        .type_style(TextStyle::TouchSmall, cx)
        .text_color(muted_on(sheet_ground(cx), cx))
        .child(text.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalTone {
    Danger,
    Accent,
}

type ModalHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct ModalAction {
    id: ElementId,
    label: SharedString,
    tone: ModalTone,
    disabled: bool,
    on_click: ModalHandler,
}

impl ModalAction {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        tone: ModalTone,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            tone,
            disabled: false,
            on_click: Rc::new(on_click),
        }
    }
}

impl crate::Disableable for ModalAction {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

const CANCEL: &str = "Cancel";

fn label_width(text: &str, window: &Window, cx: &App) -> Pixels {
    let style = TextStyle::TouchBody;
    let mut font = ala::font(cx.theme().font_family.clone());
    font.weight = style.strong_weight();
    let run = TextRun {
        len: text.len(),
        font,
        color: ala::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(
            SharedString::from(text.to_owned()),
            style.size().to_pixels(window.rem_size()),
            &[run],
            None,
        )
        .width
}

fn card_width(room: Pixels) -> Pixels {
    (room - px(MODAL_GUTTER))
        .min(px(MODAL_MAX_WIDTH))
        .max(px(0.0))
}

fn modal_width(window: &Window) -> Pixels {
    card_width(window.viewport_size().width)
}

#[must_use]
pub fn modal_buttons_stack(labels: [Pixels; 2], room: Pixels) -> bool {
    let each = (room - px(Space::S8.design_px())) / 2.0 - px(MODAL_CONTROL) / 5.0 * 2.0;
    labels.into_iter().any(|label| label > each)
}

pub fn modal_buttons(
    sheet: impl Into<ElementId>,
    action: ModalAction,
    window: &Window,
    cx: &App,
) -> Div {
    let sheet = sheet.into();
    let ground = sheet_ground(cx);
    let size = Size::Size(px(MODAL_CONTROL));
    let room = modal_width(window) - px(MODAL_PAD * 2.0);
    let stack = modal_buttons_stack(
        [
            label_width(CANCEL, window, cx),
            label_width(&action.label, window, cx),
        ],
        room,
    );
    let cancelled = sheet.clone();
    let cancel = Button::new("modal-cancel")
        .label(CANCEL)
        .with_size(size)
        .w_full()
        .on(ground)
        .on_click(move |_, _, cx| close_sheet(cancelled.clone(), cx));
    let tone = match action.tone {
        ModalTone::Danger => Tone::Danger,
        ModalTone::Accent => Tone::Accent,
    };
    let on_click = action.on_click;
    let act = Button::new(action.id)
        .with_variant(ButtonVariant::Paint(tone, Emphasis::Solid))
        .label(action.label)
        .with_size(size)
        .w_full()
        .on(ground)
        .disabled(action.disabled)
        .on_click(move |_, window, cx| {
            on_click(window, cx);
            close_sheet(sheet.clone(), cx);
        });
    if stack {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(Space::S8)
            .child(act)
            .child(cancel)
    } else {
        div()
            .flex()
            .w_full()
            .gap(Space::S8)
            .child(div().flex_1().min_w_0().child(cancel))
            .child(div().flex_1().min_w_0().child(act))
    }
}

#[must_use]
pub fn modal_scrim(cx: &App) -> ala::Hsla {
    let theme = cx.theme();
    if theme.mode.is_dark() {
        theme.scrim
    } else {
        theme.foreground.opacity(MODAL_SCRIM_LIGHT)
    }
}

fn title_text(title: SharedString, cut_middle: bool, style: TextStyle) -> AnyElement {
    if cut_middle {
        crate::blocks::touch::middle_line(title, style).into_any_element()
    } else {
        div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(title)
            .into_any_element()
    }
}

pub fn sheet_row(
    id: impl Into<ElementId>,
    slot: Option<Icon>,
    label: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    option_row(
        id,
        Surface::new(roles::PHONE_ROW),
        slot.map_or(Lead::Empty, |icon| Lead::Tile(Box::new(icon))),
        label,
        cx,
    )
}

enum Lead {
    Empty,
    Tile(Box<Icon>),
}

fn option_row(
    id: impl Into<ElementId>,
    surface: Surface,
    lead: Lead,
    label: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let layer = cx.theme().layer(surface, PaintState::Rest);
    let height = if matches!(lead, Lead::Tile(_)) {
        TILE_ROW_HEIGHT
    } else {
        OPTION_HEIGHT
    };
    let lead = match lead {
        Lead::Empty => None,
        Lead::Tile(icon) => Some(icon_tile(*icon, TILE, &layer, cx).into_any_element()),
    };
    row_shell(id, surface, px(height), cx).children(lead).child(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .type_style(crate::TextStyle::TouchBody, cx)
            .child(label),
    )
}

fn row_shell(
    id: impl Into<ElementId>,
    surface: Surface,
    height: Pixels,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let nest = crate::blocks::touch::Nest {
        outer: theme.phone_radius(),
        inset: theme.row_inset,
    };
    crate::blocks::touch::touch_row_frame(id, surface, height, Some(nest), PaintState::Rest, cx)
}

fn icon_tile(icon: Icon, size: f32, layer: &Layer, cx: &App) -> Div {
    let theme = cx.theme();
    let role = SurfaceRole::ICON_TILE;
    let tile = theme.layer(Surface::new(role).on(layer.ground), PaintState::Rest);
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .layer(&tile)
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
        let rem = cx.theme().font_size;
        let sheet = if self.modal {
            self.render_modal(window, cx)
        } else {
            self.render_sheet(window, cx)
        };
        crate::blocks::touch::at_rem(sheet, rem)
    }
}

struct ModalState {
    since: Instant,
    leaving: bool,
    dismissed: bool,
    bounds: Rc<std::cell::Cell<Option<ala::Bounds<Pixels>>>>,
}

impl BottomSheet {
    fn render_modal(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let id = self.id.clone();
        let now = cx.background_executor().now();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| {
            take_close(&id, cx);
            ModalState {
                since: cx.background_executor().now(),
                leaving: false,
                dismissed: false,
                bounds: Rc::default(),
            }
        });
        let closing = take_close(&self.id, cx);
        let reduce_motion = cx.reduce_motion();
        let (shown, moving, dismiss) = state.update(cx, |state, _| {
            if closing && !state.leaving {
                state.leaving = true;
                state.since = now;
            }
            let elapsed = now.saturating_duration_since(state.since).as_secs_f32()
                / SURFACE_ENTER_DURATION.as_secs_f32();
            let done = elapsed >= 1.0;
            let eased = ease_out_quint()(elapsed.clamp(0.0, 1.0));
            let shown = if state.leaving { 1.0 - eased } else { eased };
            let dismiss = state.leaving && done && !state.dismissed;
            if dismiss {
                state.dismissed = true;
            }
            (shown, !done, dismiss)
        });
        if moving {
            window.request_animation_frame();
        }
        if dismiss {
            let on_dismiss = Rc::clone(&self.on_dismiss);
            window.defer(cx, move |window, cx| on_dismiss(window, cx));
        }
        let theme = cx.theme();
        let measured = Rc::clone(&state.read(cx).bounds);
        let visible = window.fully_visible_bounds();
        let visual = window.visual_viewport_bounds();
        let (pad_top, pad_bottom, pad_left, pad_right) =
            measured
                .get()
                .map_or((px(0.0), px(0.0), px(0.0), px(0.0)), |own| {
                    (
                        (visible.top().max(visual.top()) - own.top()).max(px(0.0)),
                        (own.bottom() - visible.bottom().min(visual.bottom())).max(px(0.0)),
                        (visible.left() - own.left()).max(px(0.0)),
                        (own.right() - visible.right()).max(px(0.0)),
                    )
                });
        let drawer = Surface::new(SurfaceRole::DRAWER);
        let ground = theme.ground_of(drawer);
        let rise = if reduce_motion {
            px(0.0)
        } else {
            px(MODAL_RISE * (1.0 - shown))
        };
        let x = !self.actions.is_empty();
        let title_style = TextStyle::TouchBody;
        let slot = px(MODAL_TOP - MODAL_PAD);
        let rem = theme.font_size;
        let title_slack = (px(MODAL_TITLE) - title_style.line_height().to_pixels(rem)) / 2.0;
        let head = div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .min_h(px(MODAL_TITLE))
            .when(x, |head| head.px(px(MODAL_TITLE_SIDE)))
            .type_style(title_style, cx)
            .font_semibold()
            .text_color(theme.foreground)
            .child(title_text(self.title, self.cut_middle, title_style))
            .when(x, |head| {
                head.child(
                    div()
                        .absolute()
                        .top_0()
                        .right(slot)
                        .flex()
                        .children(self.actions),
                )
            });
        let heading = div()
            .flex()
            .flex_col()
            .px(px(MODAL_PAD))
            .child(head)
            .children(self.detail.map(|detail| {
                div()
                    .mt(Space::S4.rems().to_pixels(rem) - title_slack)
                    .w_full()
                    .text_center()
                    .type_style(TextStyle::TouchSmall, cx)
                    .text_color(muted_on(ground, cx))
                    .child(detail)
            }));
        let corner = theme.phone_radius();
        let card = div()
            .id("modal-card")
            .debug_selector(|| "modal-card".to_owned())
            .relative()
            .flex()
            .flex_col()
            .w(measured
                .get()
                .map_or_else(|| modal_width(window), |own| card_width(own.size.width)))
            .pt(px(MODAL_TOP))
            .surface(drawer, cx)
            .rounded(corner)
            .occlude()
            .font_family(theme.font_family.clone())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(heading)
            .child(div().flex_none().h(Space::S16))
            .child(self.content);
        let backdrop = div()
            .absolute()
            .inset_0()
            .rounded(corner)
            .bg(theme.background)
            .child(div().absolute().inset_0().rounded(corner).bg(theme.scrim));
        let card = div()
            .relative()
            .top(rise)
            .opacity(shown)
            .child(backdrop)
            .child(card);
        let leave = state.clone();
        let scrim = div()
            .id("modal-scrim")
            .debug_selector(|| "modal-scrim".to_owned())
            .absolute()
            .inset_0()
            .bg(modal_scrim(cx))
            .opacity(shown)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_click(move |_, _, cx| {
                let now = cx.background_executor().now();
                leave.update(cx, |state, cx| {
                    if !state.leaving {
                        state.leaving = true;
                        state.since = now;
                        cx.notify();
                    }
                });
            });
        div()
            .id(self.id)
            .absolute()
            .inset_0()
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                canvas(
                    move |bounds, window, _| {
                        if measured.get() != Some(bounds) {
                            measured.set(Some(bounds));
                            window.refresh();
                        }
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(scrim)
            .child(
                div()
                    .absolute()
                    .top(pad_top)
                    .bottom(pad_bottom)
                    .left(pad_left)
                    .right(pad_right)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(card),
            )
            .into_any_element()
    }

    fn render_sheet(self, window: &mut Window, cx: &mut App) -> AnyElement {
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
        let sheet = Surface::new(SurfaceRole::DRAWER);
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
        let slot = px(CONTENT_EDGE);
        let title_style = TextStyle::TouchSmall;
        let header = div()
            .debug_selector(|| "bottom-sheet-header".to_owned())
            .relative()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .mb(Space::S16)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(px(HEADER_SIDE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .type_style(title_style, cx)
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(title_text(self.title, self.cut_middle, title_style)),
            )
            .children(self.back.map(|back| {
                div()
                    .absolute()
                    .top_0()
                    .left(slot)
                    .h(px(HEADER_HEIGHT))
                    .flex()
                    .items_center()
                    .child(back)
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(slot)
                    .h(px(HEADER_HEIGHT))
                    .flex()
                    .items_center()
                    .children(self.actions),
            );
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
            .rounded(theme.phone_radius())
            .font_family(theme.font_family.clone())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(listener)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .justify_center()
                    .pt(Space::S6)
                    .child(
                        div()
                            .w(px(GRABBER_WIDTH))
                            .h(px(GRABBER_HEIGHT))
                            .layer(&grabber),
                    ),
            )
            .child(header)
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
                    .mb(Space::S8)
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
            .into_any_element()
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
        settle(cx);
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

    struct ModalHost {
        dismissed: Rc<Cell<usize>>,
    }

    impl Render for ModalHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let dismissed = Rc::clone(&self.dismissed);
            div().size_full().child(
                bottom_sheet(
                    "modal",
                    "Close this pane?",
                    Vec::new(),
                    modal_body().child(div().h(px(48.0))),
                    px(0.0),
                    move |_, _| dismissed.set(dismissed.get() + 1),
                )
                .modal(),
            )
        }
    }

    #[ala::test]
    fn a_tap_on_the_modal_scrim_cancels_and_a_tap_on_its_card_does_not(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let dismissed = Rc::new(Cell::new(0));
        let count = Rc::clone(&dismissed);
        let (_, cx) = cx.add_window_view(move |_, _| ModalHost { dismissed: count });
        settle(cx);
        let card = cx.debug_bounds("modal-card").expect("card");
        let scrim = cx.debug_bounds("modal-scrim").expect("scrim");
        assert_eq!(card.center().x, scrim.center().x);

        cx.simulate_click(card.center(), Modifiers::none());
        settle(cx);
        assert_eq!(dismissed.get(), 0);

        cx.simulate_click(
            point(scrim.left() + px(4.0), scrim.top() + px(4.0)),
            Modifiers::none(),
        );
        settle(cx);
        assert_eq!(dismissed.get(), 1);
    }
}
