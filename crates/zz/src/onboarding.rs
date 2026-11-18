use std::path::{Path, PathBuf};

use ala::{
    AnyElement, Context, Div, Entity, EventEmitter, FocusHandle, IntoElement, KeyDownEvent, Render,
    SharedString, Window, div, prelude::*,
};
use penna::{
    ActiveTheme as _, Sizable as _,
    components::{
        button::{Button, ButtonLook, ButtonVariants as _},
        switch::Switch,
    },
};
use zz_client::providers::home_relative;
use zz_ui::{
    onboarding::{Step, onboarding_page, onboarding_row},
    settings::providers::PROVIDERS,
    shell::window_drag_handle,
};

use crate::{
    config::{agent_setup, import_prompt, settings},
    mux::{client::MuxClient, hosts::HostId},
};

pub(crate) struct Finished;

#[derive(Clone, Copy)]
enum Donor {
    Tmux,
    Ghostty,
}

impl Donor {
    const fn name(self) -> &'static str {
        match self {
            Self::Tmux => "tmux",
            Self::Ghostty => "Ghostty",
        }
    }
}

struct Source {
    path: Option<PathBuf>,
    import: bool,
}

impl Source {
    const fn new(path: Option<PathBuf>) -> Self {
        Self {
            import: path.is_some(),
            path,
        }
    }

    fn chosen(&self) -> Option<&Path> {
        self.path.as_deref().filter(|_| self.import)
    }
}

pub(crate) struct Onboarding {
    step: Step,
    tmux: Source,
    ghostty: Source,
    hook: bool,
    mux: Entity<MuxClient>,
    focus: FocusHandle,
    focused: bool,
}

impl EventEmitter<Finished> for Onboarding {}

impl Onboarding {
    pub(crate) fn new(mux: Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        cx.observe(&mux, |_, _, cx| cx.notify()).detach();
        Self {
            step: Step::Welcome,
            tmux: Source::new(zz_daemon_client::discover_tmux_config()),
            ghostty: Source::new(zz_terminal::discover_ghostty_config()),
            hook: true,
            mux,
            focus: cx.focus_handle(),
            focused: false,
        }
    }

    const fn source(&self, donor: Donor) -> &Source {
        match donor {
            Donor::Tmux => &self.tmux,
            Donor::Ghostty => &self.ghostty,
        }
    }

    const fn source_mut(&mut self, donor: Donor) -> &mut Source {
        match donor {
            Donor::Tmux => &mut self.tmux,
            Donor::Ghostty => &mut self.ghostty,
        }
    }

    fn importing(&self) -> bool {
        self.tmux.chosen().is_some() || self.ghostty.chosen().is_some()
    }

    fn agents(&self, cx: &Context<Self>) -> Option<Vec<(&'static str, Option<SharedString>)>> {
        let mux = self.mux.read(cx);
        let report = mux
            .host_providers(HostId::LOCAL)
            .filter(|report| !(report.checking && report.providers.is_empty()))?;
        Some(
            PROVIDERS
                .into_iter()
                .filter_map(|provider| report.provider(provider))
                .filter(|status| status.installed())
                .map(|status| {
                    (
                        status.provider.label(),
                        status.version.clone().map(SharedString::from),
                    )
                })
                .collect(),
        )
    }

    fn choose(&mut self, donor: Donor, window: &mut Window, cx: &mut Context<Self>) {
        let selected = cx.prompt_for_paths(ala::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = selected.await.ok()?.ok()??.into_iter().next()?;
            this.update(cx, |this, cx| {
                *this.source_mut(donor) = Source::new(Some(path));
                cx.notify();
            })
            .ok()
        })
        .detach();
    }

    fn import(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.ghostty.chosen() {
            settings::run_import_from(path, cx);
        }
        if let Some(path) = self.tmux.chosen() {
            import_prompt::run_tmux_import(path.to_owned(), window, cx);
        }
    }

    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.step == Step::Import {
            self.import(window, cx);
        }
        self.go(self.step.next(), window, cx);
    }

    fn go(&mut self, step: Option<Step>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(step) = step {
            self.step = step;
            cx.notify();
            return;
        }
        if self.hook && self.agents(cx).is_none_or(|agents| !agents.is_empty()) {
            agent_setup::set_up_agents(window, cx);
        }
        import_prompt::mark_prompted();
        cx.emit(Finished);
    }

    fn donor_row(&self, donor: Donor, cx: &Context<Self>) -> Div {
        let source = self.source(donor);
        let detail = source
            .path
            .as_ref()
            .map(|path| home_relative(&path.display().to_string()).into());
        let control: AnyElement = if source.path.is_some() {
            Switch::new(("onboarding-import", donor as usize))
                .checked(source.import)
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    this.source_mut(donor).import = *checked;
                    cx.notify();
                }))
                .into_any_element()
        } else {
            Button::new(("onboarding-choose", donor as usize))
                .label("Choose…")
                .small()
                .on_click(cx.listener(move |this, _, window, cx| this.choose(donor, window, cx)))
                .into_any_element()
        };
        onboarding_row(donor.name(), detail, Some(control))
    }

    fn rows(&self, cx: &mut Context<Self>) -> Vec<Div> {
        match self.step {
            Step::Welcome => Vec::new(),
            Step::Import => vec![
                self.donor_row(Donor::Tmux, cx),
                self.donor_row(Donor::Ghostty, cx),
            ],
            Step::Agents => {
                if self.mux.read(cx).providers_unrequested() {
                    let mux = self.mux.clone();
                    cx.defer(move |cx| mux.update(cx, |mux, _| mux.request_providers(false)));
                }
                let mut rows = self
                    .agents(cx)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, version)| onboarding_row(name, version, None))
                    .collect::<Vec<_>>();
                if !rows.is_empty() {
                    let hook =
                        Switch::new("onboarding-hook")
                            .checked(self.hook)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.hook = *checked;
                                cx.notify();
                            }));
                    rows.push(onboarding_row(
                        "Install hook",
                        None,
                        Some(hook.into_any_element()),
                    ));
                }
                rows
            }
        }
    }

    fn buttons(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let importing = self.step == Step::Import && self.importing();
        let label = match self.step {
            Step::Import if importing => "Import",
            Step::Welcome | Step::Import => "Next",
            Step::Agents => "Start",
        };
        let primary = Button::new("onboarding-next")
            .label(label)
            .primary()
            .on_click(cx.listener(|this, _, window, cx| this.advance(window, cx)));
        let skip = importing.then(|| {
            Button::new("onboarding-skip")
                .label("Skip")
                .look(ButtonLook::Quiet)
                .small()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.go(this.step.next(), window, cx);
                }))
                .into_any_element()
        });
        skip.into_iter()
            .chain([primary.into_any_element()])
            .collect()
    }
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            self.focus.focus(window, cx);
        }
        let rows = self.rows(cx);
        let buttons = self.buttons(cx);
        let drag = div()
            .id("onboarding-drag-strip")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(cx.theme().title_bar_height());
        div()
            .id("onboarding")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && !event.is_held {
                    this.advance(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(onboarding_page(self.step, rows, buttons, cx))
            .child(window_drag_handle("onboarding-drag", drag, window, cx))
    }
}
