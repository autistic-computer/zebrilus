use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use zz_browser::{BrowserEvent, EditCommand, PointerEvent, PointerPhase, Viewport, WheelEvent};
use zz_gpui::{
    Bounds, Context, Corners, CursorStyle, Entity, EntityInputHandler, FocusHandle, KeyDownEvent,
    KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    RenderImage, ScrollWheelEvent, Size, Subscription, UTF16Selection, Window, div, prelude::*, px,
};
use zz_protocol::PaneId;
use zz_terminal::KeyAction as TerminalKeyAction;

use crate::{
    browser::{
        controller::{
            BrowserController, BrowserPaneFrameContent, BrowserSessionRequest, ControllerEvent,
            TabId,
        },
        element::{BrowserElement, BrowserSurfaceHost},
        view::{
            allows_text_input, browser_key, browser_modifiers, cursor_style, owns_pressed_button,
            pointer_button, pointer_button_bit, pointer_needs_clamping, rounded_coordinate,
            rounded_dimension,
        },
    },
    mux::prefix::terminal_key_input,
};

const INLINE_PANE: PaneId = PaneId(u64::MAX - 1);
const INLINE_PROFILE: &str = "artifacts";
const INLINE_BACKGROUND: u32 = 0xffff_ffff;
const REPLY_GAP: Duration = Duration::from_secs(1);

pub(crate) struct InlineBrowser {
    controller: Entity<BrowserController>,
    focus_handle: FocusHandle,
    live: Option<LivePage>,
    next_tab: u64,
    retired: Vec<Arc<RenderImage>>,
    _subscriptions: Vec<Subscription>,
}

struct LivePage {
    block: u64,
    url: String,
    reply: ReplyGate,
    tab: TabId,
    bounds: Option<Bounds<Pixels>>,
    viewport: Viewport,
    buttons: u8,
    image: Option<Arc<RenderImage>>,
    cursor: CursorStyle,
    marked_text: Option<String>,
}

struct ReplyGate {
    url: String,
    pane: Option<PaneId>,
    replied: Option<Instant>,
}

impl ReplyGate {
    fn take(&mut self, token: &str, now: Instant) -> Option<PaneId> {
        let pane = self.pane?;
        let (live, _) = zz_browser::parse_artifact_url(&self.url)?;
        if live != token
            || self
                .replied
                .is_some_and(|replied| now.saturating_duration_since(replied) < REPLY_GAP)
        {
            return None;
        }
        self.replied = Some(now);
        Some(pane)
    }
}

impl InlineBrowser {
    pub(crate) fn new(
        controller: Entity<BrowserController>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.subscribe(&controller, |inline, _, event: &ControllerEvent, cx| {
                inline.on_controller_event(event, cx);
            }),
            cx.on_focus_out(&focus_handle, window, |inline, _, window, cx| {
                inline.deactivate(window, cx);
            }),
        ];
        Self {
            controller,
            focus_handle,
            live: None,
            next_tab: 0,
            retired: Vec::new(),
            _subscriptions: subscriptions,
        }
    }

    pub(crate) fn is_live(&self, block: u64) -> bool {
        self.live.as_ref().is_some_and(|live| live.block == block)
    }

    pub(crate) fn holds_focus(&self, window: &Window) -> bool {
        self.live.is_some() && self.focus_handle.is_focused(window)
    }

    pub(crate) fn activate(
        &mut self,
        block: u64,
        html: &str,
        size: Size<Pixels>,
        reply_to: Option<PaneId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_live(cx);
        let url = match zz_browser::host_local_page(html, reply_to.is_some()) {
            Ok(url) => url,
            Err(error) => {
                log::warn!("could not host a live preview: {error}");
                return;
            }
        };
        self.next_tab += 1;
        let tab = TabId(self.next_tab);
        let viewport = Viewport {
            width: rounded_dimension(size.width),
            height: rounded_dimension(size.height),
            scale_factor: window.scale_factor(),
            window_zoom: window.zoom(),
            screen_x: 0,
            screen_y: 0,
            visible: true,
        }
        .sanitized();
        let request =
            BrowserSessionRequest::new(url.clone(), INLINE_PROFILE.to_owned(), viewport, 1.0, None)
                .readback_only()
                .with_background(INLINE_BACKGROUND);
        self.controller.update(cx, |controller, cx| {
            controller.set_active_tab(INLINE_PANE, tab, cx);
            controller.request_browser(INLINE_PANE, tab, request, cx);
            controller.set_focus(INLINE_PANE, true);
        });
        self.live = Some(LivePage {
            block,
            reply: ReplyGate {
                url: url.clone(),
                pane: reply_to,
                replied: None,
            },
            url,
            tab,
            bounds: None,
            viewport,
            buttons: 0,
            image: None,
            cursor: CursorStyle::Arrow,
            marked_text: None,
        });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_live(cx) {
            window.refresh();
        }
    }

    pub(crate) fn take_reply(&mut self, token: &str, now: Instant) -> Option<PaneId> {
        self.live.as_mut()?.reply.take(token, now)
    }

    fn close_live(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.take() else {
            return false;
        };
        self.controller.update(cx, |controller, _| {
            controller.close_pane(INLINE_PANE);
        });
        zz_browser::drop_local_page(&live.url);
        self.retired.extend(live.image);
        cx.notify();
        true
    }

    fn on_controller_event(&mut self, event: &ControllerEvent, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match event {
            ControllerEvent::Browser { pane, tab, event }
                if *pane == INLINE_PANE && *tab == live.tab =>
            {
                match event {
                    BrowserEvent::FrameReady { .. } => {
                        let frame = self.controller.read(cx).latest_frame(INLINE_PANE, live.tab);
                        if let Some(frame) = frame
                            && let BrowserPaneFrameContent::OwnedBgra(image) = frame.content
                        {
                            if let Some(previous) = live.image.replace(image) {
                                self.retired.push(previous);
                            }
                            cx.notify();
                        }
                    }
                    BrowserEvent::CursorChanged { cursor, .. } => {
                        live.cursor = cursor_style(*cursor);
                        cx.notify();
                    }
                    BrowserEvent::RenderProcessTerminated { .. } => {
                        self.close_live(cx);
                    }
                    _ => {}
                }
            }
            ControllerEvent::TabClosed { pane, tab }
                if *pane == INLINE_PANE && *tab == live.tab =>
            {
                self.close_live(cx);
            }
            ControllerEvent::BrowserFailed { pane, message } if *pane == INLINE_PANE => {
                log::warn!("live preview failed: {message}");
                self.close_live(cx);
            }
            _ => {}
        }
    }

    fn local_point(&self, point: Point<Pixels>, clamp: bool) -> Option<(i32, i32)> {
        let bounds = self.live.as_ref()?.bounds?;
        if !clamp && !bounds.contains(&point) {
            return None;
        }
        Some((
            rounded_coordinate(point.x.clamp(bounds.origin.x, bounds.right()) - bounds.origin.x),
            rounded_coordinate(point.y.clamp(bounds.origin.y, bounds.bottom()) - bounds.origin.y),
        ))
    }

    fn send_pointer(
        &mut self,
        position: Point<Pixels>,
        phase: PointerPhase,
        button: Option<MouseButton>,
        click_count: usize,
        modifiers: zz_gpui::Modifiers,
        pressed: Option<MouseButton>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.live.as_ref().map(|live| live.tab) else {
            return;
        };
        let Some((x, y)) = self.local_point(position, pointer_needs_clamping(phase, pressed))
        else {
            return;
        };
        let event = PointerEvent {
            x,
            y,
            phase,
            button: button.and_then(pointer_button),
            click_count: i32::try_from(click_count).unwrap_or(i32::MAX),
            modifiers: browser_modifiers(modifiers, pressed, false),
        };
        self.controller.update(cx, |controller, _| {
            controller.send_pointer(INLINE_PANE, tab, event);
        });
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        if let Some(live) = self.live.as_mut() {
            live.buttons |= pointer_button_bit(event.button);
        }
        self.send_pointer(
            event.position,
            PointerPhase::Down,
            Some(event.button),
            event.click_count,
            event.modifiers,
            Some(event.button),
            cx,
        );
        cx.stop_propagation();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let held = self.live.as_mut().is_some_and(|live| {
            let held = live.buttons & pointer_button_bit(event.button) != 0;
            live.buttons &= !pointer_button_bit(event.button);
            held
        });
        if held {
            self.send_pointer(
                event.position,
                PointerPhase::Up,
                Some(event.button),
                event.click_count,
                event.modifiers,
                None,
                cx,
            );
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let buttons = self.live.as_ref().map_or(0, |live| live.buttons);
        if !owns_pressed_button(buttons, event.pressed_button) {
            return;
        }
        self.send_pointer(
            event.position,
            PointerPhase::Move,
            None,
            0,
            event.modifiers,
            event.pressed_button,
            cx,
        );
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.live.as_ref().map(|live| live.tab) else {
            return;
        };
        let Some((x, y)) = self.local_point(event.position, false) else {
            return;
        };
        let delta = event.delta.pixel_delta(px(20.0));
        let wheel = WheelEvent {
            x,
            y,
            delta_x: rounded_coordinate(delta.x),
            delta_y: rounded_coordinate(delta.y),
            precise: event.delta.precise(),
            modifiers: browser_modifiers(event.modifiers, None, false),
        };
        self.controller.update(cx, |controller, cx| {
            controller.send_wheel(INLINE_PANE, tab, wheel, cx);
        });
        cx.stop_propagation();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.live.as_ref().map(|live| live.tab) else {
            return;
        };
        let keystroke = &event.keystroke;
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.deactivate(window, cx);
            cx.stop_propagation();
            return;
        }
        let action = if event.is_held {
            TerminalKeyAction::Repeat
        } else {
            TerminalKeyAction::Press
        };
        let input = zz_browser::terminal_key_input(&terminal_key_input(keystroke, action));
        self.controller.update(cx, |controller, cx| {
            controller.send_key(INLINE_PANE, tab, input, cx);
        });
        if !allows_text_input(browser_key(&keystroke.key), keystroke.modifiers) {
            cx.stop_propagation();
        }
    }

    fn on_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.live.as_ref().map(|live| live.tab) else {
            return;
        };
        let input = zz_browser::terminal_key_input(&terminal_key_input(
            &event.keystroke,
            TerminalKeyAction::Release,
        ));
        self.controller.update(cx, |controller, cx| {
            controller.send_key(INLINE_PANE, tab, input, cx);
        });
        cx.stop_propagation();
    }

    fn edit(&mut self, command: EditCommand, cx: &mut Context<Self>) {
        let Some(tab) = self.live.as_ref().map(|live| live.tab) else {
            return;
        };
        self.controller.update(cx, |controller, cx| {
            controller.edit(INLINE_PANE, tab, command, cx);
        });
    }

    fn release_retired(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.live.as_ref().map(|live| live.tab);
        let mut reclaimed = Vec::new();
        for image in self.retired.drain(..) {
            if let Err(error) = window.drop_image(image.clone()) {
                log::warn!("failed to release a live preview frame: {error}");
            }
            if let Ok(image) = Arc::try_unwrap(image) {
                reclaimed.extend(
                    image
                        .into_frames()
                        .into_iter()
                        .map(|frame| frame.into_buffer().into_raw()),
                );
            }
        }
        if let Some(tab) = tab
            && !reclaimed.is_empty()
        {
            let controller = self.controller.read(cx);
            for bgra in reclaimed {
                controller.recycle_frame(INLINE_PANE, tab, bgra);
            }
        }
    }
}

impl Render for InlineBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.release_retired(window, cx);
        let Some(live) = self.live.as_ref() else {
            return div().id("inline-browser");
        };
        let image = live.image.clone();
        let cursor = live.cursor;
        div()
            .id("inline-browser")
            .size_full()
            .overflow_hidden()
            .cursor(cursor)
            .key_context("Browser")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|inline, _: &crate::menus::Undo, _, cx| {
                inline.edit(EditCommand::Undo, cx);
            }))
            .on_action(cx.listener(|inline, _: &crate::menus::Redo, _, cx| {
                inline.edit(EditCommand::Redo, cx);
            }))
            .on_action(cx.listener(|inline, _: &crate::menus::Cut, _, cx| {
                inline.edit(EditCommand::Cut, cx);
            }))
            .on_action(cx.listener(|inline, _: &crate::menus::Copy, _, cx| {
                inline.edit(EditCommand::Copy, cx);
            }))
            .on_action(cx.listener(|inline, _: &crate::menus::Paste, _, cx| {
                inline.edit(EditCommand::Paste, cx);
            }))
            .on_action(cx.listener(|inline, _: &crate::menus::SelectAll, _, cx| {
                inline.edit(EditCommand::SelectAll, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .child(BrowserElement::new(cx.entity(), image, Corners::default()))
    }
}

impl BrowserSurfaceHost for InlineBrowser {
    fn surface_bounds(&mut self, bounds: Bounds<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        live.bounds = Some(bounds);
        let window_bounds = window.bounds();
        let next = Viewport {
            width: rounded_dimension(bounds.size.width),
            height: rounded_dimension(bounds.size.height),
            scale_factor: window.scale_factor(),
            window_zoom: window.zoom(),
            screen_x: rounded_coordinate(window_bounds.origin.x + bounds.origin.x * window.zoom()),
            screen_y: rounded_coordinate(window_bounds.origin.y + bounds.origin.y * window.zoom()),
            visible: true,
        }
        .sanitized();
        if next != live.viewport {
            live.viewport = next;
            self.controller.update(cx, |controller, cx| {
                controller.set_viewport(INLINE_PANE, next, cx);
            });
        }
    }

    fn surface_focus(&self) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for InlineBrowser {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        actual_range.replace(0..0);
        Some(String::new())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.live
            .as_ref()?
            .marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        live.marked_text = None;
        let tab = live.tab;
        self.controller.update(cx, |controller, cx| {
            controller.finish_composition(INLINE_PANE, tab, cx);
        });
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let was_composing = live.marked_text.take().is_some();
        let tab = live.tab;
        self.controller.update(cx, |controller, cx| {
            if was_composing {
                controller.commit_composition(INLINE_PANE, tab, text, cx);
            } else if !text.is_empty() {
                controller.send_text(INLINE_PANE, tab, text, cx);
            }
        });
        window.invalidate_character_coordinates();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        live.marked_text = (!text.is_empty()).then(|| text.to_owned());
        let tab = live.tab;
        let selection = selected.unwrap_or_else(|| {
            let end = text.encode_utf16().count();
            end..end
        });
        self.controller.update(cx, |controller, cx| {
            if text.is_empty() {
                controller.cancel_composition(INLINE_PANE, tab, cx);
            } else {
                controller.set_composition(INLINE_PANE, tab, text, selection, cx);
            }
        });
        window.invalidate_character_coordinates();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.live.as_ref()?.bounds?;
        Some(Bounds::new(
            bounds.origin,
            zz_gpui::size(px(1.0), px(18.0).min(bounds.size.height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_page_replies_to_its_agent_pane_at_most_once_a_second() {
        let now = Instant::now();
        let mut gate = ReplyGate {
            url: "zz://abc123/".to_owned(),
            pane: Some(PaneId(4)),
            replied: None,
        };
        assert_eq!(gate.take("other", now), None);
        assert_eq!(gate.take("abc123", now), Some(PaneId(4)));
        assert_eq!(gate.take("abc123", now + Duration::from_millis(500)), None);
        assert_eq!(gate.take("abc123", now + REPLY_GAP), Some(PaneId(4)));
        gate.pane = None;
        assert_eq!(gate.take("abc123", now + REPLY_GAP * 3), None);
    }
}
