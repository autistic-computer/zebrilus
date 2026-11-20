use std::{cell::Cell, io::Cursor, rc::Rc, time::Duration};

use ala::{
    AnyElement, AppContext as _, BorderStyle, Bounds, ClipboardItem, Context, Corners,
    DevicePixels, Entity, FocusHandle, Image, ImageFormat, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement as _, Pixels, Point, Render, ScrollWheelEvent, SharedString, Styled as _, Task,
    Window, canvas, div, outline, px, size,
};
use core_video::pixel_buffer::CVPixelBuffer;
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Label, TextStyle, components::spinner::Spinner,
};
use zz_protocol::{CommandInvocation, PaneId, SimulatorDescriptor};
use zz_simulator::{
    Button, Device, Error, Input, KeyDirection, SHIFT, Screen, Simulator, TouchPhase, key_usage,
};
use zz_ui::pane::{PaneKindButton, pane_picker_note_row, pane_picker_surface};

use super::PHONE;
use crate::mux::client::MuxClient;

const HOUSE: IconName = IconName::Asset("icons/house.svg");
const LOCK_ICON: IconName = IconName::Asset("icons/lock.svg");
const HOME: &str = "simulator-home";
const LOCK: &str = "simulator-lock";
const KEYBOARD: &str = "simulator-keyboard";
const SCREENSHOT: &str = "simulator-screenshot";
const DEVICES: &str = "simulator-devices";
const TITLE: &str = "Simulator";
const INSET: f32 = 6.0;
const TILED_INSET: f32 = 16.0;
const TILED_TOP: f32 = 4.0;
const BOOTING_SIZE: (i32, i32) = (1206, 2622);
const PHONE_CORNER: f32 = 0.146;
const TABLET_CORNER: f32 = 0.035;
const TABLET_ASPECT: f32 = 0.6;
const RETRY: Duration = Duration::from_millis(500);
const ATTEMPTS: u32 = 120;
const WHEEL_IDLE: Duration = Duration::from_millis(120);
const WHEEL_LINE: f32 = 20.0;

struct Live {
    name: SharedString,
    screen: Screen,
    input: Option<Input>,
    buffer: CVPixelBuffer,
    surface_changes: u64,
    tablet: bool,
    keyboard: bool,
    simulator: Simulator,
}

enum State {
    Devices(Vec<Device>),
    Waiting(SharedString),
    Failed(SharedString),
    Live(Box<Live>),
    Paused(SharedString),
}

pub(crate) struct SimulatorView {
    pane: PaneId,
    device: String,
    mux: Entity<MuxClient>,
    focus: FocusHandle,
    state: State,
    selected: usize,
    screen_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    floating: bool,
    visible: bool,
    touching: bool,
    wheel: Option<(f64, f64)>,
    wheel_epoch: u64,
    frames: Option<Task<()>>,
    opening: Option<Task<()>>,
    wheel_end: Option<Task<()>>,
}

impl SimulatorView {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &SimulatorDescriptor,
        mux: Entity<MuxClient>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            pane,
            device: descriptor.device.clone(),
            mux,
            focus: cx.focus_handle(),
            state: State::Devices(Vec::new()),
            selected: 0,
            screen_bounds: Rc::new(Cell::new(None)),
            floating: true,
            visible: true,
            touching: false,
            wheel: None,
            wheel_epoch: 0,
            frames: None,
            opening: None,
            wheel_end: None,
        };
        view.open(cx);
        view
    }

    pub(crate) fn synchronize_descriptor(
        &mut self,
        descriptor: &SimulatorDescriptor,
        cx: &mut Context<Self>,
    ) -> bool {
        if descriptor.device == self.device {
            return false;
        }
        self.device.clone_from(&descriptor.device);
        self.open(cx);
        true
    }

    pub(crate) fn set_floating(&mut self, floating: bool, cx: &mut Context<Self>) {
        if self.floating != floating {
            self.floating = floating;
            cx.notify();
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            if matches!(self.state, State::Paused(_)) {
                self.open(cx);
            }
        } else if let State::Live(live) = &self.state {
            self.state = State::Paused(live.name.clone());
            self.frames = None;
            self.wheel_end = None;
            self.touching = false;
            self.wheel = None;
            cx.notify();
        }
    }

    fn show(&mut self, live: Live) {
        self.state = if self.visible {
            State::Live(Box::new(live))
        } else {
            self.frames = None;
            State::Paused(live.name)
        };
    }

    pub(crate) fn focus(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub(crate) fn title(&self) -> SharedString {
        match &self.state {
            State::Live(live) => live.name.clone(),
            State::Waiting(name) | State::Paused(name) => name.clone(),
            State::Devices(_) | State::Failed(_) => TITLE.into(),
        }
    }

    pub(crate) fn header_buttons(&self) -> Vec<PaneKindButton> {
        let devices = PaneKindButton {
            key: DEVICES,
            icon: PHONE,
            label: "Devices",
        };
        match &self.state {
            State::Devices(_) | State::Paused(_) => Vec::new(),
            State::Waiting(_) | State::Failed(_) => vec![devices],
            State::Live(_) => vec![
                PaneKindButton {
                    key: HOME,
                    icon: HOUSE,
                    label: "Home",
                },
                PaneKindButton {
                    key: LOCK,
                    icon: LOCK_ICON,
                    label: "Lock",
                },
                PaneKindButton {
                    key: KEYBOARD,
                    icon: IconName::Keyboard,
                    label: "Keyboard",
                },
                PaneKindButton {
                    key: SCREENSHOT,
                    icon: IconName::Camera,
                    label: "Screenshot",
                },
                devices,
            ],
        }
    }

    pub(crate) fn header_action(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            HOME => self.button(Button::Home),
            LOCK => self.button(Button::Lock),
            KEYBOARD => {
                if let State::Live(live) = &mut self.state
                    && live.simulator.set_hardware_keyboard(live.keyboard).is_ok()
                {
                    live.keyboard = !live.keyboard;
                }
            }
            SCREENSHOT => self.screenshot(cx),
            DEVICES => self.pick(String::new(), cx),
            _ => {}
        }
    }

    fn button(&self, button: Button) {
        if let Some(input) = self.input() {
            let _ = input.button(button, KeyDirection::Down);
            let _ = input.button(button, KeyDirection::Up);
        }
    }

    fn input(&self) -> Option<&Input> {
        match &self.state {
            State::Live(live) => live.input.as_ref(),
            _ => None,
        }
    }

    fn pick(&mut self, device: String, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(CommandInvocation::new(
            "set-simulator-device",
            ["-t".to_owned(), self.pane.to_string(), device.clone()],
        ));
        self.device = device;
        self.open(cx);
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        self.frames = None;
        self.opening = None;
        self.wheel_end = None;
        self.touching = false;
        self.wheel = None;
        self.screen_bounds.set(None);
        if self.device.is_empty() {
            self.state = match zz_simulator::devices() {
                Ok(devices) if devices.is_empty() => State::Failed("No simulators".into()),
                Ok(devices) => State::Devices(devices),
                Err(error) => State::Failed(describe(&error)),
            };
            self.selected = 0;
        } else {
            match self.attach(cx) {
                Ok(live) => self.show(live),
                Err(Error::NotBooted { .. } | Error::NoDisplay) => self.wait(cx),
                Err(error) => self.state = State::Failed(describe(&error)),
            }
        }
        cx.notify();
    }

    fn attach(&mut self, cx: &mut Context<Self>) -> Result<Live, Error> {
        let simulator = Simulator::attach(&self.device)?;
        let (wake, woken) = async_channel::bounded(1);
        let screen = simulator.screen_with(move || {
            let _ = wake.try_send(());
        })?;
        let buffer = screen.pixel_buffer()?;
        let surface = screen.surface();
        self.frames = Some(cx.spawn(async move |view, cx| {
            while woken.recv().await.is_ok() {
                if view.update(cx, SimulatorView::frame).is_err() {
                    break;
                }
            }
        }));
        Ok(Live {
            name: simulator.name().into(),
            surface_changes: screen.surface_changes(),
            tablet: surface.width as f32 / surface.height.max(1) as f32 > TABLET_ASPECT
                && surface.height as f32 / surface.width.max(1) as f32 > TABLET_ASPECT,
            input: simulator.input_unverified().ok(),
            keyboard: simulator.set_hardware_keyboard(true).is_err(),
            screen,
            buffer,
            simulator,
        })
    }

    fn wait(&mut self, cx: &mut Context<Self>) {
        let name = zz_simulator::devices()
            .ok()
            .and_then(|devices| {
                devices
                    .into_iter()
                    .find(|device| device.udid.eq_ignore_ascii_case(&self.device))
            })
            .map_or_else(|| TITLE.into(), |device| device.name.into());
        self.state = State::Waiting(name);
        let device = self.device.clone();
        self.opening = Some(cx.spawn(async move |view, cx| {
            let booted = cx
                .background_spawn(async move {
                    zz_simulator::boot(&device).map_err(|error| describe(&error))
                })
                .await;
            if let Err(message) = booted {
                view.update(cx, |view, cx| {
                    view.state = State::Failed(message);
                    cx.notify();
                })
                .ok();
                return;
            }
            for _ in 0..ATTEMPTS {
                cx.background_executor().timer(RETRY).await;
                let attached = view.update(cx, |view, cx| match view.attach(cx) {
                    Ok(live) => {
                        view.show(live);
                        cx.notify();
                        true
                    }
                    Err(Error::NotBooted { .. } | Error::NoDisplay) => false,
                    Err(error) => {
                        view.state = State::Failed(describe(&error));
                        cx.notify();
                        true
                    }
                });
                if !matches!(attached, Ok(false)) {
                    return;
                }
            }
            view.update(cx, |view, cx| {
                view.state = State::Failed("The simulator did not finish booting".into());
                cx.notify();
            })
            .ok();
        }));
    }

    fn frame(&mut self, cx: &mut Context<Self>) {
        let State::Live(live) = &mut self.state else {
            return;
        };
        let changes = live.screen.surface_changes();
        if changes != live.surface_changes {
            live.surface_changes = changes;
            if live.screen.refresh().is_ok()
                && let Ok(buffer) = live.screen.pixel_buffer()
            {
                live.buffer = buffer;
            }
        }
        cx.notify();
    }

    fn screenshot(&self, cx: &mut Context<Self>) {
        let State::Live(live) = &self.state else {
            return;
        };
        let Ok((width, height, pixels)) = live.screen.read_rgba() else {
            return;
        };
        let Some(picture) = image::RgbaImage::from_raw(width, height, pixels) else {
            return;
        };
        let mut png = Vec::new();
        if picture
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .is_ok()
        {
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                ImageFormat::Png,
                png,
            )));
        }
    }

    fn point(&self, position: Point<Pixels>, clamp: bool) -> Option<(f64, f64)> {
        let bounds = self.screen_bounds.get()?;
        let x = f64::from((position.x - bounds.origin.x) / bounds.size.width);
        let y = f64::from((position.y - bounds.origin.y) / bounds.size.height);
        if clamp {
            return Some((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
        }
        ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then_some((x, y))
    }

    fn touch(&self, phase: TouchPhase, point: (f64, f64)) {
        if let Some(input) = self.input() {
            let _ = input.touch(phase, point.0, point.1);
        }
    }

    fn end_wheel(&mut self) {
        if let Some(point) = self.wheel.take() {
            self.touch(TouchPhase::Up, point);
        }
        self.wheel_end = None;
    }

    fn press(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.end_wheel();
        if let Some(point) = self.point(event.position, false) {
            self.touching = true;
            self.touch(TouchPhase::Down, point);
        }
    }

    fn drag(&mut self, event: &MouseMoveEvent, _: &mut Window, _: &mut Context<Self>) {
        if self.touching
            && let Some(point) = self.point(event.position, true)
        {
            self.touch(TouchPhase::Move, point);
        }
    }

    fn release(&mut self, event: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        if self.touching
            && let Some(point) = self.point(event.position, true)
        {
            self.touch(TouchPhase::Up, point);
        }
        self.touching = false;
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.touching {
            return;
        }
        let Some(bounds) = self.screen_bounds.get() else {
            return;
        };
        let delta = event.delta.pixel_delta(px(WHEEL_LINE));
        let step = (
            f64::from(delta.x / bounds.size.width),
            f64::from(delta.y / bounds.size.height),
        );
        let point = if let Some(point) = self.wheel {
            point
        } else {
            let Some(start) = self.point(event.position, false) else {
                return;
            };
            self.touch(TouchPhase::Down, start);
            start
        };
        let point = (
            (point.0 + step.0).clamp(0.0, 1.0),
            (point.1 + step.1).clamp(0.0, 1.0),
        );
        self.touch(TouchPhase::Move, point);
        self.wheel = Some(point);
        self.wheel_epoch += 1;
        let epoch = self.wheel_epoch;
        self.wheel_end = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(WHEEL_IDLE).await;
            view.update(cx, |view, _| {
                if view.wheel_epoch == epoch {
                    view.end_wheel();
                }
            })
            .ok();
        }));
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt || modifiers.function {
            return;
        }
        let (Some(input), Some(usage)) = (self.input(), key_usage(&keystroke.key)) else {
            return;
        };
        if modifiers.shift {
            let _ = input.key(SHIFT, KeyDirection::Down);
        }
        let _ = input.key(usage, KeyDirection::Down);
        let _ = input.key(usage, KeyDirection::Up);
        if modifiers.shift {
            let _ = input.key(SHIFT, KeyDirection::Up);
        }
        cx.stop_propagation();
    }

    fn list_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let State::Devices(devices) = &self.state else {
            return;
        };
        let modifiers = event.keystroke.modifiers;
        if devices.is_empty() || modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let count = devices.len();
        match event.keystroke.key.as_str() {
            "j" | "down" => self.selected = (self.selected + 1) % count,
            "k" | "up" => self.selected = (self.selected + count - 1) % count,
            "enter" => {
                let udid = devices[self.selected.min(count - 1)].udid.clone();
                self.pick(udid, cx);
            }
            _ => return,
        }
        cx.notify();
        cx.stop_propagation();
    }

    fn devices(&self, devices: &[Device], cx: &mut Context<Self>) -> AnyElement {
        let rows = devices
            .iter()
            .enumerate()
            .map(|(index, device)| {
                let hovered = cx.entity();
                let picked = cx.entity();
                let udid = device.udid.clone();
                pane_picker_note_row(
                    ("simulator-device", index),
                    device.name.clone(),
                    PHONE,
                    Some(device.system.clone().into()),
                    self.selected == index,
                    device.booted,
                    cx,
                )
                .on_mouse_move(move |_, _, cx| {
                    hovered.update(cx, |view, cx| {
                        if view.selected != index {
                            view.selected = index;
                            cx.notify();
                        }
                    });
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    picked.update(cx, |view, cx| view.pick(udid.clone(), cx));
                    cx.stop_propagation();
                })
                .into_any_element()
            })
            .collect::<Vec<_>>();
        pane_picker_surface(
            ("simulator-devices", self.pane.0),
            Corners::default(),
            rows,
            cx,
        )
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::list_key))
        .into_any_element()
    }

    fn note(&self, text: SharedString, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.focus)
            .child(Label::new(TextStyle::Caption, text).text_color(cx.theme().foreground.muted()))
            .into_any_element()
    }

    fn stage(&self) -> ala::Div {
        let stage = div().size_full();
        if self.floating {
            stage.px(px(INSET)).pb(px(INSET))
        } else {
            stage
                .pt(px(TILED_TOP))
                .px(px(TILED_INSET))
                .pb(px(TILED_INSET))
        }
    }

    fn booting(&self, cx: &mut Context<Self>) -> AnyElement {
        let edge = cx.theme().border();
        self.stage()
            .relative()
            .track_focus(&self.focus)
            .child(
                canvas(
                    |bounds, _, _| {
                        ObjectFit::Contain.get_bounds(
                            bounds,
                            size(DevicePixels(BOOTING_SIZE.0), DevicePixels(BOOTING_SIZE.1)),
                        )
                    },
                    move |_, fit, window, _| {
                        let radii = Corners::all(fit.size.width * PHONE_CORNER);
                        window
                            .paint_quad(outline(fit, edge, BorderStyle::Solid).corner_radii(radii));
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Spinner::new()
                            .id(("simulator-booting", self.pane.0))
                            .color(cx.theme().foreground.muted()),
                    ),
            )
            .into_any_element()
    }

    fn screen(&self, live: &Live, cx: &mut Context<Self>) -> AnyElement {
        let buffer = live.buffer.clone();
        let surface = live.screen.surface();
        let corner = if live.tablet {
            TABLET_CORNER
        } else {
            PHONE_CORNER
        };
        let slot = Rc::clone(&self.screen_bounds);
        let edge = cx.theme().border();
        self.stage()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press))
            .on_mouse_move(cx.listener(Self::drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::release))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::release))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_key_down(cx.listener(Self::key))
            .child(
                canvas(
                    move |bounds, _, _| {
                        let fit = ObjectFit::Contain.get_bounds(
                            bounds,
                            size(
                                DevicePixels(i32::try_from(surface.width).unwrap_or(i32::MAX)),
                                DevicePixels(i32::try_from(surface.height).unwrap_or(i32::MAX)),
                            ),
                        );
                        slot.set(Some(fit));
                        fit
                    },
                    move |_, fit, window, _| {
                        let radii = Corners::all(fit.size.width * corner);
                        window.paint_surface(fit, radii, buffer);
                        window
                            .paint_quad(outline(fit, edge, BorderStyle::Solid).corner_radii(radii));
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

fn describe(error: &Error) -> SharedString {
    match error {
        Error::DeveloperDir | Error::MissingClass(_) => "Xcode is not installed".into(),
        error => error.to_string().into(),
    }
}

impl Render for SimulatorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            State::Devices(devices) => self.devices(devices, cx),
            State::Waiting(_) => self.booting(cx),
            State::Failed(message) => self.note(message.clone(), cx),
            State::Live(live) => self.screen(live, cx),
            State::Paused(_) => self.stage().track_focus(&self.focus).into_any_element(),
        }
    }
}
