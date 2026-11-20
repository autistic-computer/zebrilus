use std::collections::BTreeSet;

use ala::{AnyElement, App, Context, Div, IntoElement as _, Pixels, Window, div, prelude::*, px};
use penna::{
    ActiveTheme as _, Disableable as _, IconName, Label, Sizable as _, Size, TextStyle, Tone,
    blocks::compact::compact_bar_button,
    components::{
        button::{Button, ButtonLook, ButtonVariants as _},
        input::Input,
        switch::Switch,
    },
};
use web_time::{Duration, Instant};
use zz_ui::onboarding::{
    PHONE_STEPS, PhonePage, PhoneStep, phone_onboarding_page, phone_onboarding_row,
};

use super::{AppShell, hosts::parsed_endpoint};

const TAILNET_WAIT: Duration = Duration::from_secs(4);
const STEP_MOTION: Duration = Duration::from_millis(340);
const STEP_SHIFT: f32 = 28.0;
const NOT_OURS: &str = "That code is not from zz.";

pub(super) struct PhoneOnboarding {
    step: PhoneStep,
    moved: Option<(PhoneStep, Instant)>,
    focus_field: bool,
    machine: Option<String>,
    connected_at: Option<Instant>,
    off: BTreeSet<String>,
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn place(step: PhoneStep) -> f32 {
    PHONE_STEPS
        .iter()
        .position(|at| *at == step)
        .unwrap_or_default() as f32
}

struct Shown {
    step: PhoneStep,
    fade: f32,
    shift: Pixels,
    at: f32,
}

impl AppShell {
    pub(super) fn start_onboarding(&mut self, cx: &App) {
        if !self.preferences.onboarded
            && self.hosts.saved.endpoints().is_empty()
            && self.connection.read(cx).endpoint().is_none()
        {
            self.onboarding = Some(PhoneOnboarding {
                step: PhoneStep::Welcome,
                moved: None,
                focus_field: false,
                machine: None,
                connected_at: None,
                off: BTreeSet::new(),
            });
        }
    }

    pub(super) fn onboarding_open(&self) -> bool {
        self.onboarding.is_some()
    }

    fn onboarding_go(&mut self, step: PhoneStep, cx: &mut Context<Self>) {
        if let Some(onboarding) = &mut self.onboarding
            && onboarding.step != step
        {
            onboarding.moved = Some((onboarding.step, Instant::now()));
            onboarding.focus_field = step == PhoneStep::Machine;
            onboarding.step = step;
            cx.notify();
        }
    }

    fn onboarding_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(onboarding) = &mut self.onboarding else {
            return;
        };
        match onboarding.step {
            PhoneStep::Welcome => {}
            PhoneStep::Machine => {
                self.hosts.error = None;
                window.dismiss_virtual_keyboard();
                self.onboarding_go(PhoneStep::Welcome, cx);
            }
            PhoneStep::Tailnet => {
                onboarding.machine = None;
                onboarding.connected_at = None;
                self.onboarding_go(PhoneStep::Machine, cx);
            }
        }
    }

    pub(super) fn onboarding_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.hosts.destination.read(cx).value().to_string();
        match parsed_endpoint(&value) {
            Ok(endpoint) => self.onboarding_dial(endpoint, window, cx),
            Err(error) => {
                self.hosts.error = Some(error.into());
                cx.notify();
            }
        }
    }

    pub(super) fn onboarding_open_host(
        &mut self,
        endpoint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typed = endpoint
            .split_once("://")
            .map_or(endpoint.as_str(), |(_, rest)| rest)
            .to_owned();
        self.hosts
            .destination
            .update(cx, |input, cx| input.set_value(typed, window, cx));
        self.onboarding_go(PhoneStep::Machine, cx);
        if let Some(onboarding) = &mut self.onboarding {
            onboarding.focus_field = false;
        }
        self.onboarding_dial(endpoint, window, cx);
    }

    fn onboarding_dial(&mut self, endpoint: String, window: &mut Window, cx: &mut Context<Self>) {
        self.hosts.error = None;
        window.dismiss_virtual_keyboard();
        self.hosts.saved.add(&endpoint);
        self.hosts.saved.mark_connected(&endpoint, true);
        self.hosts.saved.save();
        let here = {
            let connection = self.connection.read(cx);
            connection.connected && connection.endpoint() == Some(endpoint.as_str())
        };
        if !here {
            self.connection.update(cx, |connection, cx| {
                connection.connect_to(endpoint.clone(), cx);
            });
        }
        if let Some(onboarding) = &mut self.onboarding {
            onboarding.machine = Some(endpoint);
            onboarding.connected_at = None;
        }
        cx.notify();
    }

    fn onboarding_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.dismiss_virtual_keyboard();
        let (sender, receiver) = async_channel::bounded(1);
        ala_platform::mobile::scan_code(move |code| {
            let _ = sender.try_send(code);
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some(code)) = receiver.recv().await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                if let Some(endpoint) = super::host_from_url(&code) {
                    this.onboarding_open_host(endpoint, window, cx);
                } else {
                    this.hosts.error = Some(NOT_OURS.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn onboarding_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let off = self
            .onboarding
            .as_ref()
            .map(|onboarding| onboarding.off.clone())
            .unwrap_or_default();
        for host in self.found_machines(cx) {
            if host.machine.runs_zz && !off.contains(&host.endpoint) {
                self.hosts.saved.add(&host.endpoint);
                self.connect_host(host.endpoint, window, cx);
            }
        }
        self.finish_onboarding(window, cx);
    }

    fn finish_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.onboarding = None;
        self.hosts.error = None;
        self.preferences.onboarded = true;
        self.preferences.save();
        window.dismiss_virtual_keyboard();
        if self.connection.read(cx).connected {
            self.show_overview(cx);
        }
        cx.notify();
    }

    fn follow_first_machine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(endpoint) = self
            .onboarding
            .as_ref()
            .filter(|onboarding| onboarding.step == PhoneStep::Machine)
            .and_then(|onboarding| onboarding.machine.clone())
        else {
            return;
        };
        let (ours, connected, busy, looking, status) = {
            let connection = self.connection.read(cx);
            (
                connection.endpoint() == Some(endpoint.as_str()),
                connection.connected,
                connection.busy(),
                connection.looking_for_tailnet(),
                connection.status.clone(),
            )
        };
        if !ours {
            return;
        }
        if connected {
            let now = Instant::now();
            let since = self.onboarding.as_mut().map_or(now, |onboarding| {
                *onboarding.connected_at.get_or_insert(now)
            });
            if looking && now.saturating_duration_since(since) < TAILNET_WAIT {
                window.request_animation_frame();
                return;
            }
            if self.found_machines(cx).is_empty() {
                self.finish_onboarding(window, cx);
            } else {
                self.onboarding_go(PhoneStep::Tailnet, cx);
            }
        } else if !busy {
            if let Some(onboarding) = &mut self.onboarding {
                onboarding.machine = None;
            }
            self.hosts.error = Some(status.into());
            self.hosts.saved.remove(&endpoint);
            self.hosts.saved.save();
            cx.notify();
        }
    }

    fn onboarding_shown(&mut self, window: &mut Window, cx: &App) -> Shown {
        let Some(onboarding) = &mut self.onboarding else {
            return Shown {
                step: PhoneStep::Welcome,
                fade: 1.0,
                shift: px(0.0),
                at: 0.0,
            };
        };
        let step = onboarding.step;
        let settled = Shown {
            step,
            fade: 1.0,
            shift: px(0.0),
            at: place(step),
        };
        let Some((from, since)) = onboarding.moved else {
            return settled;
        };
        let progress = since.elapsed().as_secs_f32() / STEP_MOTION.as_secs_f32();
        if progress >= 1.0 || cx.reduce_motion() {
            onboarding.moved = None;
            return settled;
        }
        window.request_animation_frame();
        let way = (place(step) - place(from)).signum();
        let at = place(from) + (place(step) - place(from)) * ease_out(progress);
        if progress < 0.5 {
            let out = ease_out(progress * 2.0);
            Shown {
                step: from,
                fade: 1.0 - out,
                shift: px(-STEP_SHIFT * way * out),
                at,
            }
        } else {
            let into = ease_out((progress - 0.5) * 2.0);
            Shown {
                step,
                fade: into,
                shift: px(STEP_SHIFT * way * (1.0 - into)),
                at,
            }
        }
    }

    fn onboarding_field(&self, waiting: bool, cx: &mut Context<Self>) -> Div {
        let danger = cx.theme().paint(Tone::Danger, penna::Emphasis::Ghost).text;
        let scan = ala_platform::mobile::can_scan().then(|| {
            compact_bar_button("onboarding-scan", IconName::QrCode)
                .disabled(waiting)
                .on_click(cx.listener(|this, _, window, cx| this.onboarding_scan(window, cx)))
        });
        let field = Input::new(&self.hosts.destination)
            .id("onboarding-field")
            .with_size(Size::TouchLarge)
            .disabled(waiting);
        let field = match scan {
            Some(scan) => field.suffix(scan),
            None => field,
        };
        div().flex().flex_col().gap(px(8.0)).child(field).children(
            self.hosts
                .error
                .clone()
                .map(|error| Label::new(TextStyle::TouchSmall, error).text_color(danger)),
        )
    }

    fn onboarding_rows(&self, cx: &mut Context<Self>) -> Div {
        let off = self
            .onboarding
            .as_ref()
            .map(|onboarding| onboarding.off.clone())
            .unwrap_or_default();
        let rows = self
            .found_machines(cx)
            .into_iter()
            .enumerate()
            .map(|(index, host)| {
                let control = host.machine.runs_zz.then(|| {
                    let endpoint = host.endpoint.clone();
                    Switch::new(("onboarding-found", index))
                        .checked(!off.contains(&host.endpoint))
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            if let Some(onboarding) = &mut this.onboarding {
                                if *checked {
                                    onboarding.off.remove(&endpoint);
                                } else {
                                    onboarding.off.insert(endpoint.clone());
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element()
                });
                phone_onboarding_row(
                    host.machine.name.clone(),
                    zz_ui::compact::found_detail(&host.machine),
                    control,
                )
            });
        div().flex().flex_col().children(rows)
    }

    fn onboarding_button(&self, step: PhoneStep, waiting: bool, cx: &mut Context<Self>) -> Button {
        let button = match step {
            PhoneStep::Welcome => Button::new("onboarding-next")
                .label("Next")
                .on_click(cx.listener(|this, _, _, cx| this.onboarding_go(PhoneStep::Machine, cx))),
            PhoneStep::Machine => {
                let typed = self.hosts.destination.read(cx).value().to_string();
                Button::new("onboarding-connect")
                    .label(if waiting { "Connecting" } else { "Connect" })
                    .loading(waiting)
                    .disabled(waiting || parsed_endpoint(&typed).is_err())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.onboarding_connect(window, cx)),
                    )
            }
            PhoneStep::Tailnet => {
                let off = self
                    .onboarding
                    .as_ref()
                    .map(|onboarding| onboarding.off.clone())
                    .unwrap_or_default();
                let picked = self
                    .found_machines(cx)
                    .iter()
                    .filter(|host| host.machine.runs_zz && !off.contains(&host.endpoint))
                    .count();
                Button::new("onboarding-add")
                    .label(if picked == 0 {
                        "Done".to_owned()
                    } else {
                        format!("Add {picked}")
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.onboarding_add(window, cx)))
            }
        };
        button.primary().with_size(Size::TouchLarge).w_full()
    }

    pub(super) fn onboarding_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.follow_first_machine(window, cx);
        self.onboarding.as_ref()?;
        let Shown {
            step: shown,
            fade,
            shift,
            at,
        } = self.onboarding_shown(window, cx);
        let (step, waiting) = self
            .onboarding
            .as_ref()
            .map(|onboarding| (onboarding.step, onboarding.machine.is_some()))?;
        if shown == PhoneStep::Machine
            && let Some(onboarding) = &mut self.onboarding
            && std::mem::take(&mut onboarding.focus_field)
        {
            let input = self.hosts.destination.clone();
            cx.on_next_frame(window, move |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
                window.request_virtual_keyboard();
            });
        }
        let machine = self
            .connection
            .read(cx)
            .endpoint()
            .map_or_else(String::new, super::hosts::host_title);
        let body = match shown {
            PhoneStep::Welcome => None,
            PhoneStep::Machine => Some(self.onboarding_field(waiting, cx)),
            PhoneStep::Tailnet => Some(self.onboarding_rows(cx)),
        };
        let button = self.onboarding_button(step, waiting, cx);
        let back = (step != PhoneStep::Welcome).then(|| {
            compact_bar_button("onboarding-back", IconName::ChevronLeft)
                .disabled(waiting && step == PhoneStep::Machine)
                .on_click(cx.listener(|this, _, window, cx| this.onboarding_back(window, cx)))
                .into_any_element()
        });
        let skip = Button::new("onboarding-skip")
            .label("Skip")
            .look(ButtonLook::Quiet)
            .with_size(Size::Touch)
            .on_click(cx.listener(|this, _, window, cx| this.finish_onboarding(window, cx)))
            .into_any_element();
        let viewport = window.viewport_size().height;
        let keyboard = viewport - window.visual_viewport_bounds().bottom();
        let safe = viewport - window.fully_visible_bounds().bottom();
        let page = phone_onboarding_page(
            PhonePage {
                at,
                shown,
                title: shown.title(&machine),
                body,
                fade,
                shift,
                button: button.into_any_element(),
                back,
                skip: Some(skip),
                top: window.fully_visible_bounds().top(),
                bottom: keyboard.max(safe).max(px(0.0)),
            },
            cx,
        );
        Some(
            div()
                .id("phone-onboarding")
                .absolute()
                .inset_0()
                .occlude()
                .child(page)
                .into_any_element(),
        )
    }
}
