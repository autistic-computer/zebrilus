use std::{rc::Rc, time::Duration};

use ala::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ClickEvent, ElementId,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Ground, Kind, PaintState, Size, StyledExt as _, Surface, SurfaceRole, h_flex,
    input::field_chrome,
};

const INSET: f32 = 2.0;
const SEGMENT_PADDING: f32 = 10.0;
const SLIDE: Duration = Duration::from_millis(220);

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    segments: Vec<SharedString>,
    selected: usize,
    on: Option<Ground>,
    on_select: Option<SelectHandler>,
}

#[derive(Default)]
struct Slide {
    segments: Vec<Bounds<Pixels>>,
    shown: Option<usize>,
    from: Option<Bounds<Pixels>>,
    generation: u64,
}

impl SegmentedControl {
    #[must_use]
    pub fn new(
        id: impl Into<ElementId>,
        segments: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self {
            id: id.into(),
            segments: segments.into_iter().map(Into::into).collect(),
            selected: 0,
            on: None,
            on_select: None,
        }
    }

    #[must_use]
    pub const fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    #[must_use]
    pub const fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    #[must_use]
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

fn lerp(from: Pixels, to: Pixels, delta: f32) -> Pixels {
    from + (to - from) * delta
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let track = Surface::new(Kind::Field.role()).on_maybe(self.on);
        let layer = cx.theme().layer(track, PaintState::Rest);
        let ground = layer.ground;
        let border = layer.border_width.max(px(1.0));
        let height = Size::Medium.control_px();
        let inner = px(height - INSET * 2.0) - border * 2.0;
        let radius = (layer.radius.min(px(height / 2.0)) - border - px(INSET)).max(px(0.0));
        let pill = Surface::new(SurfaceRole::QUIET_BUTTON).on(ground);
        let picked = cx.theme().layer(pill, PaintState::Current);
        let rest = cx.theme().layer(pill, PaintState::Rest).text;
        let hover = cx.theme().layer(pill, PaintState::Hover).text;
        let coarse = crate::touch::CoarsePointer::get(cx);
        let slop = crate::touch::control_slop(Size::Medium);

        let selected = self.selected.min(self.segments.len().saturating_sub(1));
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| Slide::default());
        let (to, from, generation) = state.update(cx, |slide, _| {
            let to = slide.segments.get(selected).copied();
            if to.is_some() && slide.shown != Some(selected) {
                slide.from = slide
                    .shown
                    .and_then(|shown| slide.segments.get(shown).copied());
                slide.shown = Some(selected);
                slide.generation += 1;
            }
            (to, slide.from, slide.generation)
        });
        let pill = to.map(|to| {
            let pill = div()
                .absolute()
                .top_0()
                .h(inner)
                .layer(&picked)
                .rounded(radius);
            match from {
                Some(from) if from != to => pill
                    .with_animation(
                        ("slide", generation),
                        Animation::new(SLIDE).with_easing(ala::ease_out_quint()),
                        move |pill, delta| {
                            pill.left(lerp(from.origin.x, to.origin.x, delta)).w(lerp(
                                from.size.width,
                                to.size.width,
                                delta,
                            ))
                        },
                    )
                    .into_any_element(),
                _ => pill.left(to.origin.x).w(to.size.width).into_any_element(),
            }
        });

        let skip = usize::from(pill.is_some());
        let on_select = self.on_select;
        let measured = state.clone();
        let segments = self
            .segments
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let picked_text = picked.text;
                let click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>> =
                    on_select.clone().map(|on_select| {
                        Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            on_select(index, window, cx);
                        }) as Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>
                    });
                div()
                    .id(("segment", index))
                    .role(ala::Role::Button)
                    .aria_label(label.clone())
                    .aria_selected(index == selected)
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .h(inner)
                    .px(px(SEGMENT_PADDING))
                    .text_size(Size::Medium.control_text())
                    .whitespace_nowrap()
                    .map(|segment| {
                        if index == selected {
                            segment.text_color(picked_text)
                        } else {
                            segment
                                .text_color(rest)
                                .hover(move |style| style.text_color(hover))
                        }
                    })
                    .child(label)
                    .when_some(click, |segment, click| {
                        let tap = Rc::clone(&click);
                        segment
                            .on_click(move |event, window, cx| click(event, window, cx))
                            .when(coarse && slop > 0.0, |segment| {
                                segment.child(
                                    crate::touch::hit_area("touch", 0.0, slop)
                                        .on_click(move |event, window, cx| tap(event, window, cx)),
                                )
                            })
                    })
                    .into_any_element()
            })
            .collect::<Vec<AnyElement>>();

        field_chrome(
            h_flex().id(self.id).flex_none().p(px(INSET)),
            track,
            PaintState::Rest,
            true,
            false,
            cx,
        )
        .child(
            h_flex()
                .relative()
                .children(pill)
                .children(segments)
                .on_children_prepainted(move |bounds, window, cx| {
                    let Some(first) = bounds.get(skip).copied() else {
                        return;
                    };
                    let segments = bounds[skip..]
                        .iter()
                        .map(|bounds| Bounds::new(bounds.origin - first.origin, bounds.size))
                        .collect::<Vec<_>>();
                    let changed = measured.update(cx, |slide, _| {
                        let changed = slide.segments != segments;
                        slide.segments = segments;
                        changed
                    });
                    if changed {
                        window.request_animation_frame();
                    }
                }),
        )
    }
}
