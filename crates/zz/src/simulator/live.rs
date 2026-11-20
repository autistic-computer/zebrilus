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
use zz_protocol::{
    CommandInvocation, GuiResponse, PaneId, SimulatorButton, SimulatorCommand, SimulatorDescriptor,
};
use zz_simulator::{
    Button, Device, Error, Input, KeyDirection, SHIFT, Screen, Simulator, TouchPhase, Turn,
    char_usage, key_usage,
};
use zz_ui::pane::{PaneKindButton, PaneTitleRow, pane_picker_note_row, pane_picker_surface};

use super::PHONE;
use crate::mux::client::MuxClient;

const HOUSE: IconName = IconName::Asset("icons/house.svg");
const LOCK_ICON: IconName = IconName::Asset("icons/lock.svg");
const HOME: &str = "simulator-home";
const LOCK: &str = "simulator-lock";
const KEYBOARD: &str = "simulator-keyboard";
const ROTATE: &str = "simulator-rotate";
const ROTATE_ICON: IconName = IconName::Asset("icons/rotate.svg");
const SCREENSHOT: &str = "simulator-screenshot";
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
const TAP_HOLD: Duration = Duration::from_millis(60);
const SWIPE_STEPS: u32 = 20;
const SWIPE_STEP: Duration = Duration::from_millis(15);
const KEY_GAP: Duration = Duration::from_millis(15);

struct Live {
    name: SharedString,
    screen: Screen,
    input: Option<Input>,
    buffer: CVPixelBuffer,
    surface_changes: u64,
    tablet: bool,
    turns: u8,
    keyboard: bool,
    simulator: Simulator,
}

enum State {
    Devices(Vec<Device>),
    Waiting(SharedString),
    Failed(SharedString),
    Live(Box<Live>),
}

pub(crate) struct SimulatorView {
    pane: PaneId,
    device: String,
    mux: Entity<MuxClient>,
    focus: FocusHandle,
    state: State,
    devices: Vec<Device>,
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
            devices: Vec::new(),
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
        if let State::Live(live) = &mut self.state {
            live.screen.set_paused(!visible);
            if visible {
                if let Ok(buffer) = live.screen.pixel_buffer() {
                    live.buffer = buffer;
                }
                live.surface_changes = live.screen.surface_changes();
                live.turns = live.screen.turns();
            }
            cx.notify();
        }
    }

    fn show(&mut self, mut live: Live) {
        live.screen.set_paused(!self.visible);
        self.state = State::Live(Box::new(live));
    }

    pub(crate) fn focus(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub(crate) fn title(&self) -> SharedString {
        match &self.state {
            State::Live(live) => live.name.clone(),
            State::Waiting(name) => name.clone(),
            State::Devices(_) | State::Failed(_) => TITLE.into(),
        }
    }

    pub(crate) fn title_menu(&self) -> Vec<PaneTitleRow> {
        if self.device.is_empty() || matches!(self.state, State::Devices(_)) {
            return Vec::new();
        }
        self.devices
            .iter()
            .map(|device| PaneTitleRow {
                label: device.name.clone().into(),
                detail: device.system.clone().into(),
                current: device.udid.eq_ignore_ascii_case(&self.device),
            })
            .collect()
    }

    pub(crate) fn title_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(device) = self.devices.get(index) {
            let udid = device.udid.clone();
            self.pick(udid, cx);
        }
    }

    pub(crate) fn header_buttons(&self) -> Vec<PaneKindButton> {
        match &self.state {
            State::Devices(_) | State::Waiting(_) | State::Failed(_) => Vec::new(),
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
                    key: ROTATE,
                    icon: ROTATE_ICON,
                    label: "Rotate",
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
            ],
        }
    }

    pub(crate) fn header_action(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            HOME => self.button(Button::Home),
            LOCK => self.button(Button::Lock),
            ROTATE => {
                if let State::Live(live) = &self.state {
                    let _ = live.simulator.rotate(Turn::Left);
                }
            }
            KEYBOARD => {
                if let State::Live(live) = &mut self.state
                    && live.simulator.set_hardware_keyboard(live.keyboard).is_ok()
                {
                    live.keyboard = !live.keyboard;
                }
            }
            SCREENSHOT => self.screenshot(cx),
            _ => {}
        }
    }

    pub(crate) fn command(&mut self, command: SimulatorCommand, cx: &mut Context<Self>) {
        let request_id = command.request_id();
        let State::Live(live) = &self.state else {
            self.answer(
                request_id,
                Err(format!("{} is not showing a booted simulator", self.pane)),
                cx,
            );
            return;
        };
        let upright = live.upright_size();
        match command {
            SimulatorCommand::Screenshot { .. } => {
                let sent = self
                    .png()
                    .and_then(|png| self.mux.read(cx).send_screenshot(request_id, &png));
                if let Err(message) = sent {
                    self.answer(request_id, Err(message), cx);
                }
            }
            SimulatorCommand::Tap { x, y, .. } => match normalize((x, y), upright) {
                Ok(point) => self.play(request_id, vec![point], cx),
                Err(message) => self.answer(request_id, Err(message), cx),
            },
            SimulatorCommand::Swipe { from, to, .. } => {
                match (normalize(from, upright), normalize(to, upright)) {
                    (Ok(from), Ok(to)) => {
                        let path = (0..=SWIPE_STEPS)
                            .map(|step| {
                                let t = f64::from(step) / f64::from(SWIPE_STEPS);
                                (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
                            })
                            .collect();
                        self.play(request_id, path, cx);
                    }
                    (Err(message), _) | (_, Err(message)) => {
                        self.answer(request_id, Err(message), cx);
                    }
                }
            }
            SimulatorCommand::Type { text, .. } => {
                let keys = text.chars().map(|character| {
                    char_usage(character).ok_or_else(|| format!("cannot type {character:?}"))
                });
                match keys.collect::<Result<Vec<_>, _>>() {
                    Ok(keys) => self.type_keys(request_id, keys, cx),
                    Err(message) => self.answer(request_id, Err(message), cx),
                }
            }
            SimulatorCommand::Press { button, .. } => {
                self.button(match button {
                    SimulatorButton::Home => Button::Home,
                    SimulatorButton::Lock => Button::Lock,
                });
                self.answer(request_id, Ok(String::new()), cx);
            }
        }
    }

    fn answer(&self, request_id: u64, result: Result<String, String>, cx: &ala::App) {
        self.mux.read(cx).respond_to_request(match result {
            Ok(output) => GuiResponse::Success { request_id, output },
            Err(message) => GuiResponse::Error {
                request_id,
                message,
            },
        });
    }

    fn play(&self, request_id: u64, path: Vec<(f64, f64)>, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            let last = path.len().saturating_sub(1);
            for (index, point) in path.into_iter().enumerate() {
                let phase = match index {
                    0 => TouchPhase::Down,
                    _ => TouchPhase::Move,
                };
                view.update(cx, |view, _| view.touch(phase, point)).ok();
                if index == last {
                    cx.background_executor()
                        .timer(if last == 0 { TAP_HOLD } else { SWIPE_STEP })
                        .await;
                    view.update(cx, |view, cx| {
                        view.touch(TouchPhase::Up, point);
                        view.answer(request_id, Ok(String::new()), cx);
                    })
                    .ok();
                } else {
                    cx.background_executor().timer(SWIPE_STEP).await;
                }
            }
        })
        .detach();
    }

    fn type_keys(&self, request_id: u64, keys: Vec<(u32, bool)>, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            for (usage, shift) in keys {
                view.update(cx, |view, _| {
                    if let Some(input) = view.input() {
                        if shift {
                            let _ = input.key(SHIFT, KeyDirection::Down);
                        }
                        let _ = input.key(usage, KeyDirection::Down);
                        let _ = input.key(usage, KeyDirection::Up);
                        if shift {
                            let _ = input.key(SHIFT, KeyDirection::Up);
                        }
                    }
                })
                .ok();
                cx.background_executor().timer(KEY_GAP).await;
            }
            view.update(cx, |view, cx| {
                view.answer(request_id, Ok(String::new()), cx);
            })
            .ok();
        })
        .detach();
    }

    fn png(&self) -> Result<Vec<u8>, String> {
        let State::Live(live) = &self.state else {
            return Err(format!("{} is not showing a booted simulator", self.pane));
        };
        let (width, height, pixels) = live.screen.read_rgba().map_err(|error| error.to_string())?;
        let picture = image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| "the simulator screen has no pixels".to_owned())?;
        let mut png = Vec::new();
        upright(picture, live.turns)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(png)
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
        let listed = zz_simulator::devices();
        self.devices = listed.as_ref().map(Clone::clone).unwrap_or_default();
        if self.device.is_empty() {
            self.state = match listed {
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
            turns: screen.turns(),
            input: simulator.input_unverified().ok(),
            keyboard: simulator.set_hardware_keyboard(true).is_err(),
            screen,
            buffer,
            simulator,
        })
    }

    fn wait(&mut self, cx: &mut Context<Self>) {
        let name = self
            .devices
            .iter()
            .find(|device| device.udid.eq_ignore_ascii_case(&self.device))
            .map_or_else(|| TITLE.into(), |device| device.name.clone().into());
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
        live.turns = live.screen.turns();
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
        if let Ok(png) = self.png() {
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
        let State::Live(live) = &self.state else {
            return;
        };
        if let Some(input) = &live.input {
            let (x, y) = unturn(point, live.turns);
            let _ = input.touch(phase, x, y);
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
        let turns = live.turns;
        let (width, height) = if turns % 2 == 1 {
            (surface.height, surface.width)
        } else {
            (surface.width, surface.height)
        };
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
                                DevicePixels(i32::try_from(width).unwrap_or(i32::MAX)),
                                DevicePixels(i32::try_from(height).unwrap_or(i32::MAX)),
                            ),
                        );
                        slot.set(Some(fit));
                        fit
                    },
                    move |_, fit, window, _| {
                        let radii = Corners::all(fit.size.width.min(fit.size.height) * corner);
                        window.paint_surface_turned(fit, radii, buffer, turns);
                        window
                            .paint_quad(outline(fit, edge, BorderStyle::Solid).corner_radii(radii));
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

impl Live {
    fn upright_size(&self) -> (u32, u32) {
        let surface = self.screen.surface();
        let width = u32::try_from(surface.width).unwrap_or(u32::MAX);
        let height = u32::try_from(surface.height).unwrap_or(u32::MAX);
        if self.turns % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        }
    }
}

fn normalize((x, y): (u32, u32), (width, height): (u32, u32)) -> Result<(f64, f64), String> {
    if x >= width || y >= height {
        return Err(format!(
            "{x},{y} is outside the {width}x{height} screen of capture-simulator"
        ));
    }
    Ok((
        (f64::from(x) + 0.5) / f64::from(width),
        (f64::from(y) + 0.5) / f64::from(height),
    ))
}

fn unturn((u, v): (f64, f64), turns: u8) -> (f64, f64) {
    match turns {
        1 => (v, 1.0 - u),
        2 => (1.0 - u, 1.0 - v),
        3 => (1.0 - v, u),
        _ => (u, v),
    }
}

fn upright(picture: image::RgbaImage, turns: u8) -> image::RgbaImage {
    match turns {
        1 => image::imageops::rotate90(&picture),
        2 => image::imageops::rotate180(&picture),
        3 => image::imageops::rotate270(&picture),
        _ => picture,
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
        }
    }
}
