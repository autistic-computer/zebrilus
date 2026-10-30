use ala::{
    AnyElement, App, Bounds, Div, Element, ElementId, Global, GlobalElementId, InspectorElementId,
    InteractiveElement as _, IntoElement, LayoutId, Pixels, Stateful, Styled as _, Window, div,
};

use crate::{Size, rems_from_px};

mod arrow_pad;
mod dots;
mod key_press;
mod key_row;
mod lift;
mod pager;
mod popover_key;
mod press;
mod sticky;
mod swipe_back;
mod which_key_list;

pub use arrow_pad::{ArrowDirection, ArrowPad, ArrowPadEvent, arrow_pad_bubble, resolve_direction};
pub use dots::{PageDot, page_dots, page_dots_in};
pub use key_row::{
    KEY_PILL_HEIGHT, KEY_ROW_HEIGHT, KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, KeyRow, KeyRowKey,
    ToolKeys,
};
pub use lift::{Lift, LiftEvent, LiftResponse};
pub use pager::{Pager, PagerEvent, PagerLayout, PagerResponse};
pub use popover_key::{
    PopoverKey, PopoverKeyEvent, PopoverKeyItem, PopoverKeyTap, alt_chords, control_chords,
};
pub use press::{
    PressCorners, PressFeedback, instant_press_highlight, press_feedback, press_highlight,
};
pub use sticky::{StickyModifier, StickyModifiers};
pub use swipe_back::{SwipeBack, claim_back_swipe, swipe_back, yield_back_swipe};
pub use which_key_list::WhichKeyList;

pub const TOUCH_TARGET: f32 = 44.0;

#[must_use]
pub fn concentric(radius: Pixels, gap: Pixels) -> Pixels {
    if radius >= ala::FULL_CORNER_RADIUS {
        ala::FULL_CORNER_RADIUS
    } else {
        (radius - gap).max(ala::px(0.0))
    }
}

pub const TOUCH_TYPE_SCALE: f32 = 15.0 / 13.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoarsePointer(pub bool);

impl Global for CoarsePointer {}

impl CoarsePointer {
    #[must_use]
    pub fn get(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|pointer| pointer.0)
    }

    pub fn set(coarse: bool, cx: &mut App) {
        if Self::get(cx) != coarse {
            cx.set_global(Self(coarse));
        }
    }
}

pub fn touch_scale(child: impl IntoElement, cx: &App) -> AnyElement {
    if CoarsePointer::get(cx) {
        TouchScale {
            child: child.into_any_element(),
            rem_size: None,
        }
        .into_any_element()
    } else {
        child.into_any_element()
    }
}

struct TouchScale {
    child: AnyElement,
    rem_size: Option<Pixels>,
}

impl IntoElement for TouchScale {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TouchScale {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let rem_size = *self
            .rem_size
            .get_or_insert(window.rem_size() * TOUCH_TYPE_SCALE);
        let child = &mut self.child;
        let layout_id =
            window.with_rem_size(Some(rem_size), |window| child.request_layout(window, cx));
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let child = &mut self.child;
        window.with_rem_size(self.rem_size, |window| child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let child = &mut self.child;
        window.with_rem_size(self.rem_size, |window| child.paint(window, cx));
    }
}

#[must_use]
pub fn control_slop(size: Size) -> f32 {
    match size {
        Size::Size(_) => 0.0,
        size => (TOUCH_TARGET - size.control_px()) / 2.0,
    }
}

#[must_use]
pub fn hit_area(id: impl Into<ElementId>, slop_x: f32, slop_y: f32) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top(rems_from_px(-slop_y))
        .bottom(rems_from_px(-slop_y))
        .left(rems_from_px(-slop_x))
        .right(rems_from_px(-slop_x))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use ala::{
        Context, IntoElement, Modifiers, ParentElement as _, Render,
        StatefulInteractiveElement as _, TestAppContext, Window, point, px,
    };

    use super::*;
    use crate::{
        IconName,
        button::{Button, ButtonVariants as _},
    };

    struct Host {
        clicks: Rc<Cell<[usize; 3]>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let count = |index: usize, clicks: &Rc<Cell<[usize; 3]>>| {
                let clicks = Rc::clone(clicks);
                move |_: &ala::ClickEvent, _: &mut Window, _: &mut App| {
                    let mut all = clicks.get();
                    all[index] += 1;
                    clicks.set(all);
                }
            };
            div()
                .size_full()
                .id("row")
                .on_click(count(2, &self.clicks))
                .child(
                    div()
                        .absolute()
                        .top(px(100.0))
                        .left(px(100.0))
                        .flex()
                        .gap(px(4.0))
                        .child(
                            Button::compact_icon("first", IconName::Plus)
                                .debug_selector(|| "first".to_owned())
                                .hit_slop(2.0, 10.0)
                                .on_click(count(0, &self.clicks)),
                        )
                        .child(
                            Button::compact_icon("second", IconName::Xmark)
                                .ghost()
                                .debug_selector(|| "second".to_owned())
                                .hit_slop(2.0, 10.0)
                                .on_click(count(1, &self.clicks)),
                        ),
                )
        }
    }

    #[ala::test]
    fn coarse_buttons_take_taps_beside_the_glyph(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let clicks = Rc::new(Cell::new([0; 3]));
        let shared = Rc::clone(&clicks);
        let (_, cx) = cx.add_window_view(move |_, _| Host { clicks: shared });
        let tap = |cx: &mut ala::VisualTestContext, at| {
            cx.simulate_click(at, Modifiers::none());
            cx.run_until_parked();
        };
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let first = cx.debug_bounds("first").expect("first");
        assert_eq!(first.size.height, px(24.0));
        let above = point(first.center().x, first.top() - px(8.0));
        tap(cx, above);
        assert_eq!(clicks.get(), [0, 0, 1]);

        cx.update(|window, cx| {
            CoarsePointer::set(true, cx);
            window.refresh();
            _ = window.draw(cx);
        });
        let first = cx.debug_bounds("first").expect("first");
        let second = cx.debug_bounds("second").expect("second");
        assert_eq!(first.size.height, px(24.0));
        assert_eq!(first.left(), px(100.0));
        assert_eq!(first.top(), px(100.0));
        assert_eq!(second.left() - first.right(), px(4.0));
        tap(cx, point(first.center().x, first.top() - px(8.0)));
        tap(cx, point(first.center().x, first.bottom() + px(8.0)));
        tap(cx, first.center());
        assert_eq!(clicks.get(), [3, 0, 1]);
        tap(cx, point(first.right() + px(1.0), first.center().y));
        tap(cx, point(second.left() - px(1.0), first.center().y));
        assert_eq!(clicks.get(), [4, 1, 1]);
        tap(cx, point(first.center().x, first.top() - px(12.0)));
        assert_eq!(clicks.get(), [4, 1, 2]);
    }

    #[test]
    fn inner_corners_share_the_outer_center() {
        assert_eq!(concentric(ala::px(28.0), ala::px(8.0)), ala::px(20.0));
        assert_eq!(concentric(ala::px(4.0), ala::px(8.0)), ala::px(0.0));
        assert_eq!(
            concentric(ala::FULL_CORNER_RADIUS, ala::px(8.0)),
            ala::FULL_CORNER_RADIUS
        );
    }
}
