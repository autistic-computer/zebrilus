use std::collections::BTreeSet;

use ala::{AnyElement, App, Context, IntoElement as _, Window, div, prelude::*, px};
use penna::{
    ActiveTheme as _, Disableable as _, Label, Sizable as _, Size, TextStyle, Tone,
    components::{
        button::{Button, ButtonLook, ButtonVariants as _},
        input::Input,
        switch::Switch,
    },
};
use web_time::{Duration, Instant};
use zz_ui::onboarding::{PhonePage, PhoneStep, phone_onboarding_page, phone_onboarding_row};

use super::{AppShell, hosts::parsed_endpoint};

const TAILNET_WAIT: Duration = Duration::from_secs(4);
const FIELD_CORNER: f32 = 18.0;

pub(super) struct PhoneOnboarding {
    step: PhoneStep,
    machine: Option<String>,
    connected_at: Option<Instant>,
    off: BTreeSet<String>,
}

impl AppShell {
    pub(super) fn start_onboarding(&mut self, cx: &App) {
        if !self.preferences.onboarded
            && self.hosts.saved.endpoints().is_empty()
            && self.connection.read(cx).endpoint().is_none()
        {
            self.onboarding = Some(PhoneOnboarding {
                step: PhoneStep::Welcome,
                machine: None,
                connected_at: None,
                off: BTreeSet::new(),
            });
        }
    }

    pub(super) fn onboarding_open(&self) -> bool {
        self.onboarding.is_some()
    }

    pub(super) fn onboarding_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.hosts.destination.read(cx).value().to_string();
        let endpoint = match parsed_endpoint(&value) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                self.hosts.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        self.hosts.error = None;
        window.dismiss_virtual_keyboard();
        self.hosts.saved.add(&endpoint);
        self.hosts.saved.mark_connected(&endpoint, true);
        self.hosts.saved.save();
        self.connection.update(cx, |connection, cx| {
            connection.connect_to(endpoint.clone(), cx);
        });
        if let Some(onboarding) = &mut self.onboarding {
            onboarding.machine = Some(endpoint);
            onboarding.connected_at = None;
        }
        cx.notify();
    }

    fn onboarding_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(onboarding) = &mut self.onboarding else {
            return;
        };
        onboarding.step = PhoneStep::Machine;
        let input = self.hosts.destination.clone();
        cx.on_next_frame(window, move |_, window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
            window.request_virtual_keyboard();
        });
        cx.notify();
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
            } else if let Some(onboarding) = &mut self.onboarding {
                onboarding.step = PhoneStep::Tailnet;
                cx.notify();
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

    pub(super) fn onboarding_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.follow_first_machine(window, cx);
        let onboarding = self.onboarding.as_ref()?;
        let step = onboarding.step;
        let waiting = onboarding.machine.is_some();
        let machine = self
            .connection
            .read(cx)
            .endpoint()
            .map_or_else(String::new, super::hosts::host_title);
        let (body, button) = match step {
            PhoneStep::Welcome => (
                None,
                Button::new("onboarding-next")
                    .primary()
                    .label("Next")
                    .with_size(Size::TouchLarge)
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.onboarding_next(window, cx))),
            ),
            PhoneStep::Machine => {
                let typed = self.hosts.destination.read(cx).value().to_string();
                let danger = cx.theme().paint(Tone::Danger, penna::Emphasis::Ghost).text;
                let body =
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            Input::new(&self.hosts.destination)
                                .id("onboarding-field")
                                .with_size(Size::Touch)
                                .rounded(px(FIELD_CORNER))
                                .disabled(waiting),
                        )
                        .children(self.hosts.error.clone().map(|error| {
                            Label::new(TextStyle::TouchSmall, error).text_color(danger)
                        }));
                let button = Button::new("onboarding-connect")
                    .primary()
                    .label(if waiting { "Connecting" } else { "Connect" })
                    .with_size(Size::TouchLarge)
                    .w_full()
                    .loading(waiting)
                    .disabled(waiting || parsed_endpoint(&typed).is_err())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.onboarding_connect(window, cx)),
                    );
                (Some(body), button)
            }
            PhoneStep::Tailnet => {
                let found = self.found_machines(cx);
                let picked = found
                    .iter()
                    .filter(|host| host.machine.runs_zz && !onboarding.off.contains(&host.endpoint))
                    .count();
                let rows = found.into_iter().enumerate().map(|(index, host)| {
                    let control = host.machine.runs_zz.then(|| {
                        let endpoint = host.endpoint.clone();
                        Switch::new(("onboarding-found", index))
                            .checked(!onboarding.off.contains(&host.endpoint))
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
                let body = div().flex().flex_col().children(rows);
                let button = Button::new("onboarding-add")
                    .primary()
                    .label(if picked == 0 {
                        "Done".to_owned()
                    } else {
                        format!("Add {picked}")
                    })
                    .with_size(Size::TouchLarge)
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.onboarding_add(window, cx)));
                (Some(body), button)
            }
        };
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
                step,
                title: step.title(&machine),
                body,
                button: button.into_any_element(),
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
