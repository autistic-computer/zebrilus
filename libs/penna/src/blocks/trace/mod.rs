use ala::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Icon, IconName, Kind, Sizable as _,
    Space, StyledExt as _, Surface, TextStyle, Tone, components::icon::cap_center_drop,
    components::separator::Separator, components::spinner::Spinner, foundation::RuleLook,
    foundation::motion::Disclosure as DisclosureChevron, foundation::roles, h_flex, v_flex,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceStatus {
    Running,
    #[default]
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct TraceStep {
    pub tool: SharedString,
    pub target: SharedString,
    pub status: TraceStatus,
    pub duration_ms: Option<u64>,
    pub detail: Option<SharedString>,
    pub default_open: bool,
}

impl TraceStep {
    #[must_use]
    pub fn new(tool: impl Into<SharedString>, target: impl Into<SharedString>) -> Self {
        Self {
            tool: tool.into(),
            target: target.into(),
            status: TraceStatus::Done,
            duration_ms: None,
            detail: None,
            default_open: false,
        }
    }

    #[must_use]
    pub const fn status(mut self, status: TraceStatus) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub const fn duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub const fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceSummary {
    pub total: usize,
    pub running: usize,
    pub done: usize,
    pub errors: usize,
    pub total_ms: u64,
}

#[derive(IntoElement)]
pub struct Trace {
    id: ElementId,
    title: Option<SharedString>,
    steps: Vec<TraceStep>,
    style: StyleRefinement,
}

impl Trace {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, steps: Vec<TraceStep>) -> Self {
        Self {
            id: id.into(),
            title: None,
            steps,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn summary(&self) -> TraceSummary {
        let mut out = TraceSummary {
            total: self.steps.len(),
            ..TraceSummary::default()
        };
        for step in &self.steps {
            match step.status {
                TraceStatus::Running => out.running += 1,
                TraceStatus::Done => out.done += 1,
                TraceStatus::Error => out.errors += 1,
            }
            if let Some(ms) = step.duration_ms {
                out.total_ms = out.total_ms.saturating_add(ms);
            }
        }
        out
    }
}

impl Styled for Trace {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn format_duration(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.2} s", ms as f64 / 1000.0)
    } else {
        format!("{ms} ms")
    }
}

impl RenderOnce for Trace {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let summary = self.summary();
        let theme = cx.theme();
        let surface = Surface::new(Kind::Card.role());
        let code_surface = Surface::new(roles::CARD_CODE);
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let success = theme.tone(Tone::Success);
        let danger = theme.tone(Tone::Danger);
        let mono = theme.mono_font_family.clone();
        let drop = cap_center_drop(px(TextStyle::Small.design_px()), px(12.0), cx);

        let mut host = v_flex()
            .author_id(self.id.clone())
            .role(Role::Region)
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .surface(surface, cx)
            .refine_style(&self.style);

        if let Some(title) = self.title {
            let meta = if summary.total_ms > 0 {
                format!(
                    "{} steps · {}",
                    summary.total,
                    format_duration(summary.total_ms)
                )
            } else {
                format!("{} steps", summary.total)
            };
            host = host
                .child(
                    h_flex()
                        .w_full()
                        .h(px(32.0))
                        .px(Space::S12)
                        .items_center()
                        .justify_between()
                        .gap(Space::S8)
                        .child(
                            div()
                                .type_style_strong(TextStyle::Small, cx)
                                .text_color(fg)
                                .child(title),
                        )
                        .child(
                            div()
                                .font_family(mono.clone())
                                .type_style(TextStyle::Caption, cx)
                                .text_color(muted)
                                .child(meta),
                        ),
                )
                .child(Separator::horizontal().look(RuleLook::Bar));
        }

        let mut list = v_flex().w_full().min_w_0();
        for (index, step) in self.steps.into_iter().enumerate() {
            if index > 0 {
                list = list.child(Separator::horizontal().look(RuleLook::List));
            }
            let default_open = step.default_open;
            let open_state =
                window.use_keyed_state((self.id.clone(), format!("step-{index}")), cx, |_, _| {
                    default_open
                });
            let is_open = *open_state.read(cx);
            let has_detail = step.detail.is_some();

            let status_el = match step.status {
                TraceStatus::Running => Spinner::new().xsmall().into_any_element(),
                TraceStatus::Done => Icon::new(IconName::CircleCheck)
                    .size(px(13.0))
                    .text_color(success)
                    .into_any_element(),
                TraceStatus::Error => Icon::new(IconName::CircleX)
                    .size(px(13.0))
                    .text_color(danger)
                    .into_any_element(),
            };

            let toggle = open_state.clone();
            let row =
                h_flex()
                    .id((self.id.clone(), format!("row-{index}")))
                    .w_full()
                    .min_w_0()
                    .h(px(32.0))
                    .px(Space::S12)
                    .gap(Space::S8)
                    .items_center()
                    .text_color(muted)
                    .when(has_detail, |el| {
                        el.role(Role::Button)
                            .aria_expanded(is_open)
                            .cursor_pointer()
                            .hover(move |s| s.text_color(fg))
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |open, cx| {
                                    *open = !*open;
                                    cx.notify();
                                });
                            })
                    })
                    .child(div().flex_none().relative().top(drop).child(status_el))
                    .child(
                        div()
                            .flex_none()
                            .font_family(mono.clone())
                            .type_style_strong(TextStyle::Small, cx)
                            .text_color(fg)
                            .child(step.tool),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .type_style(TextStyle::Small, cx)
                            .child(step.target),
                    )
                    .when_some(step.duration_ms, |el, ms| {
                        el.child(
                            div()
                                .flex_none()
                                .font_family(mono.clone())
                                .type_style(TextStyle::Caption, cx)
                                .text_color(muted)
                                .child(format_duration(ms)),
                        )
                    })
                    .when(has_detail, |el| {
                        el.child(div().flex_none().relative().top(drop).child(
                            DisclosureChevron::new(
                                (self.id.clone(), format!("chev-{index}")),
                                is_open,
                                px(12.0),
                            ),
                        ))
                    });

            list = list.child(row);
            if let Some(detail) = step.detail
                && is_open
            {
                list = list.child(
                    div().w_full().px(Space::S12).pb(Space::S10).child(
                        div()
                            .w_full()
                            .p(Space::S8)
                            .surface(code_surface, cx)
                            .font_family(mono.clone())
                            .type_style(TextStyle::Caption, cx)
                            .text_color(muted)
                            .child(detail),
                    ),
                );
            }
        }

        host.child(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_summary_counts_statuses_and_sums_durations() {
        let trace = Trace::new(
            "t",
            vec![
                TraceStep::new("read_file", "src/lib.rs")
                    .status(TraceStatus::Done)
                    .duration_ms(14),
                TraceStep::new("run_command", "cargo test")
                    .status(TraceStatus::Running)
                    .duration_ms(1240),
                TraceStep::new("edit_file", "docs/site.json")
                    .status(TraceStatus::Error)
                    .duration_ms(6),
            ],
        );
        assert_eq!(
            trace.summary(),
            TraceSummary {
                total: 3,
                running: 1,
                done: 1,
                errors: 1,
                total_ms: 1260,
            }
        );
        assert_eq!(format_duration(142), "142 ms");
        assert_eq!(format_duration(1240), "1.24 s");
    }
}
