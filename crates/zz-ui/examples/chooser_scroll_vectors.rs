use std::{error::Error, fs::File};

use ala::{
    Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, ScrollDelta,
    ScrollStrategy, ScrollWheelEvent, Styled as _, TestAppContext, TestDispatcher, TouchPhase,
    UniformListScrollHandle, Window, div, point, px, uniform_list,
};
use serde_json::{Value, json};

struct ListView {
    count: usize,
    viewport: f32,
    requested_line: f32,
    measured_line: f32,
    handle: UniformListScrollHandle,
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .line_height(px(self.requested_line))
            .child(
                uniform_list(
                    "chooser-scroll-source-list",
                    self.count,
                    cx.processor(|view, range: std::ops::Range<usize>, window, _| {
                        view.measured_line = f32::from(window.line_height());
                        range.map(|index| div().id(index).h(px(40.0))).collect()
                    }),
                )
                .track_scroll(&self.handle)
                .w(px(200.0))
                .h(px(self.viewport)),
            )
    }
}

fn positive_offset(handle: &UniformListScrollHandle) -> f32 {
    let value = f32::from(handle.0.borrow().base_handle.offset().y);
    if value == 0.0 { 0.0 } else { -value }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut records: Vec<Value> = Vec::new();
    let mut seed = 0;
    for count in [0_usize, 1, 10, 11, 47, 65_536] {
        for viewport in [40.0_f32, 80.0, 200.0, 400.5] {
            for scale in [1.0_f32, 1.25, 2.0] {
                seed += 1;
                let mut app = TestAppContext::build(TestDispatcher::new(seed), None);
                let (entity, cx) = app.add_window_view(|_, _| ListView {
                    count,
                    viewport,
                    requested_line: if count.is_multiple_of(2) { 19.5 } else { 24.0 },
                    measured_line: 19.5,
                    handle: UniformListScrollHandle::new(),
                });
                cx.simulate_scale_factor_change(scale);
                cx.run_until_parked();
                let (handle, actual_viewport) = entity.read_with(cx, |view, _| {
                    (
                        view.handle.clone(),
                        f32::from(view.handle.0.borrow().base_handle.bounds().size.height),
                    )
                });
                for fraction in [0.0_f32, 0.5, 1.0] {
                    for selected in [0_u32, 1, 5, 9, 10, 46, u32::MAX] {
                        let maximum = f32::from(handle.0.borrow().base_handle.max_offset().y);
                        entity.update(cx, |view, cx| {
                            view.handle
                                .0
                                .borrow()
                                .base_handle
                                .set_offset(point(px(0.0), px(-maximum * fraction)));
                            cx.notify();
                        });
                        cx.run_until_parked();
                        let before = positive_offset(&handle);
                        entity.update(cx, |view, cx| {
                            view.handle.scroll_to_item(
                                usize::try_from(selected).unwrap_or(usize::MAX),
                                ScrollStrategy::Center,
                            );
                            cx.notify();
                        });
                        cx.run_until_parked();
                        records.push(json!({"kind":0,"before":before.to_bits(),"row":40_f32.to_bits(),"viewport":actual_viewport.to_bits(),"count":count,"selected":selected,"after":positive_offset(&handle).to_bits(),"scale":scale}));
                    }
                }
                for fraction in [0.0_f32, 0.5, 1.0] {
                    for precise in [false, true] {
                        for (x, y) in [
                            (-2.0_f32, 0.0_f32),
                            (0.0, -1.25),
                            (3.0, 4.0),
                            (0.0, 50_000.0),
                        ] {
                            let maximum = f32::from(handle.0.borrow().base_handle.max_offset().y);
                            entity.update(cx, |view, cx| {
                                view.handle
                                    .0
                                    .borrow()
                                    .base_handle
                                    .set_offset(point(px(0.0), px(-maximum * fraction)));
                                cx.notify();
                            });
                            cx.run_until_parked();
                            let before = positive_offset(&handle);
                            let line = entity.read_with(cx, |view, _| view.measured_line);
                            let position = point(px(50.0), px(actual_viewport / 2.0));
                            cx.simulate_mouse_move(position, None, Modifiers::default());
                            cx.simulate_event(ScrollWheelEvent {
                                position,
                                delta: if precise {
                                    ScrollDelta::Pixels(point(px(x), px(y)))
                                } else {
                                    ScrollDelta::Lines(point(x, y))
                                },
                                modifiers: Modifiers::default(),
                                touch_phase: TouchPhase::Moved,
                            });
                            cx.run_until_parked();
                            records.push(json!({"kind":1,"before":before.to_bits(),"maximum":maximum.to_bits(),"line":line.to_bits(),"precise":precise,"x":x.to_bits(),"y":y.to_bits(),"after":positive_offset(&handle).to_bits(),"scale":scale}));
                        }
                    }
                }
            }
        }
    }
    let path = std::env::args()
        .nth(1)
        .ok_or("chooser scroll output path required")?;
    serde_json::to_writer(File::create(path)?, &records)?;
    Ok(())
}
