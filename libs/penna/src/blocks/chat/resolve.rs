use std::{sync::LazyLock, time::Duration};

use ala::{
    AnyElement, App, Bounds, CustomShader, Div, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Styled as _, Window,
    ease_out_quint, px,
};

use crate::foundation::motion::Instant;

const SPAN: Duration = Duration::from_millis(160);
const STAGGER: Duration = Duration::from_millis(18);
const STAGGER_BUDGET: Duration = Duration::from_millis(400);
const BLUR: f32 = 5.0;
const PAD: f32 = 8.0;

static SHADER: LazyLock<CustomShader> =
    LazyLock::new(|| CustomShader::new(include_str!("resolve.wgsl")));

fn stagger(count: usize) -> Duration {
    let count = u32::try_from(count.max(1)).unwrap_or(u32::MAX);
    STAGGER.min(STAGGER_BUDGET / count)
}

fn resolved_after(count: usize) -> Duration {
    let rest = u32::try_from(count.saturating_sub(1)).unwrap_or(u32::MAX);
    stagger(count) * rest + SPAN
}

pub(super) struct Resolve {
    label: SharedString,
    count: usize,
    child: Option<Div>,
}

impl Resolve {
    pub(super) fn new(label: SharedString, shown: &str, child: Div) -> Self {
        Self {
            label,
            count: shown.chars().count(),
            child: Some(child),
        }
    }
}

struct Changed {
    label: SharedString,
    at: Instant,
}

pub(super) struct Running {
    elapsed: Duration,
    blurs: bool,
}

impl IntoElement for Resolve {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Resolve {
    type RequestLayoutState = (AnyElement, Option<Running>);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("resolve".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let now = cx.background_executor().now();
        let label = self.label.clone();
        let changed = window.with_element_state(
            id.expect("resolve has an id"),
            |state: Option<Changed>, _| {
                let state = state
                    .filter(|state| state.label == label)
                    .unwrap_or(Changed { label, at: now });
                (state.at, state)
            },
        );
        let elapsed = ala::animation_time(now.saturating_duration_since(changed));
        let total = resolved_after(self.count);
        let running = (!cx.reduce_motion() && elapsed < total).then_some(Running {
            elapsed,
            blurs: !cfg!(target_os = "windows"),
        });
        let child = self.child.take().expect("resolve lays out once");
        let mut child = match &running {
            Some(running) if !running.blurs => child
                .opacity(ease_out_quint()(
                    elapsed.as_secs_f32() / total.as_secs_f32(),
                ))
                .into_any_element(),
            _ => child.into_any_element(),
        };
        (child.request_layout(window, cx), (child, running))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (child, _): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (child, running): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(running) = running else {
            child.paint(window, cx);
            return;
        };
        window.request_animation_frame();
        if !running.blurs {
            child.paint(window, cx);
            return;
        }
        let scale = window.scale_factor();
        let uniforms: Vec<u8> = [
            f32::from(bounds.left()) * scale,
            f32::from(bounds.size.width) * scale,
            f32::from(u16::try_from(self.count).unwrap_or(u16::MAX)),
            running.elapsed.as_secs_f32() * 1000.0,
            stagger(self.count).as_secs_f32() * 1000.0,
            SPAN.as_secs_f32() * 1000.0,
            BLUR * scale,
            0.0,
        ]
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
        window.paint_shader_layer(bounds.dilate(px(PAD)), &SHADER, uniforms, |window| {
            child.paint(window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, resolved_after, stagger};

    #[test]
    fn the_stagger_shrinks_so_a_long_label_still_resolves_within_560ms() {
        assert_eq!(stagger(10), Duration::from_millis(18));
        assert_eq!(resolved_after(10), Duration::from_millis(322));
        assert_eq!(stagger(40), Duration::from_millis(10));
        assert_eq!(resolved_after(40), Duration::from_millis(550));
    }
}
