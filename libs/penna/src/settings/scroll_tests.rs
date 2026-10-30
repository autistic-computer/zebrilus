use ala::{
    AppContext as _, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, VisualTestContext, Window,
    div, point, px,
};

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn scroll(cx: &mut VisualTestContext, x: f32, y: f32, dx: f32, dy: f32) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        ..Default::default()
    });
    draw(cx);
}

fn row(selector: &'static str, height: f32) -> Div {
    div()
        .h(px(height))
        .flex_shrink_0()
        .debug_selector(move || selector.to_string())
}

struct WrappedTextTest;

impl Render for WrappedTextTest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().w(px(300.)).h(px(200.)).child(
            div().flex().flex_1().min_h_0().overflow_hidden().child(
                super::settings_scroll_column("wrapped-text-column")
                    .child(
                        div()
                            .text_size(crate::rems_from_px(10.))
                            .child(ala::SharedString::from("word ".repeat(60))),
                    )
                    .child(row("wrapped-text-tail", 20.)),
            ),
        )
    }
}

struct SettingsPageReplicaTest {
    pickers: Vec<ala::Entity<crate::color_picker::ColorPickerState>>,
    numbers: Vec<ala::Entity<crate::input::InputState>>,
}

impl Render for SettingsPageReplicaTest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().w(px(760.)).h(px(300.)).child(
            div().flex().flex_1().min_h_0().overflow_hidden().child(
                super::settings_scroll_column("replica-column")
                    .child(
                        div()
                            .text_size(crate::rems_from_px(11.))
                            .child("Changes are written to zz/config."),
                    )
                    .child(
                        super::SettingsStack::titled("Theme").children(self.pickers.iter().map(
                            |picker| {
                                super::SettingEntry::new("Background").control(
                                    crate::color_picker::ColorPicker::new(picker, ala::black()),
                                )
                            },
                        )),
                    )
                    .child(
                        super::SettingsStack::titled("Window")
                            .child(
                                super::SettingEntry::new("Window background opacity").control(
                                    div()
                                        .w(px(120.))
                                        .flex_none()
                                        .child(crate::input::NumberInput::new(&self.numbers[0])),
                                ),
                            )
                            .child(
                                super::SettingEntry::new("Window background blur").control(
                                    crate::switch::Switch::new("replica-blur").checked(true),
                                ),
                            ),
                    )
                    .child(
                        super::SettingsStack::titled("Interface").child(
                            super::SettingEntry::new("Widget corner radius").control(
                                div()
                                    .w(px(120.))
                                    .flex_none()
                                    .child(crate::input::NumberInput::new(&self.numbers[1])),
                            ),
                        ),
                    )
                    .child(row("replica-tail", 20.)),
            ),
        )
    }
}

#[ala::test]
fn settings_page_scroll_range_matches_content(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (_, cx) = cx.add_window_view(|window, cx| SettingsPageReplicaTest {
        pickers: (0..6)
            .map(|_| cx.new(|cx| crate::color_picker::ColorPickerState::new(None, window, cx)))
            .collect(),
        numbers: (0..2)
            .map(|_| cx.new(|cx| crate::input::InputState::new(window, cx)))
            .collect(),
    });
    let cx: &mut VisualTestContext = cx;
    draw(cx);

    scroll(cx, 300., 150., 0., -100_000.);
    let tail = cx.debug_bounds("replica-tail").unwrap();
    assert!(
        tail.bottom() >= px(250.) && tail.bottom() <= px(300.),
        "after scrolling to the end, the last row must sit at the viewport bottom, \
         not pages above it (tail bottom: {:?})",
        tail.bottom()
    );
}

#[ala::test]
fn scroll_range_measures_wrapped_text_at_layout_width(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (_, cx) = cx.add_window_view(|_, _| WrappedTextTest);
    let cx: &mut VisualTestContext = cx;
    draw(cx);

    let before = cx.debug_bounds("wrapped-text-tail").unwrap().origin.y;
    scroll(cx, 150., 100., 0., -10_000.);
    let after = cx.debug_bounds("wrapped-text-tail").unwrap().origin.y;
    assert_eq!(
        before, after,
        "content fits the viewport, so the wheel must not move it"
    );
}

struct LargeTitleTest {
    fold: super::SettingsFold,
    virtual_rows: bool,
}

impl Render for LargeTitleTest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let page = if self.virtual_rows {
            super::settings_virtual_column("large-title-virtual", 30, |_, _, _| {
                row("large-title-row", 40.).into_any_element()
            })
            .large_title("Terminal", &self.fold)
            .into_any_element()
        } else {
            super::settings_scroll_column("large-title-scroll")
                .children((0..30).map(|_| row("large-title-row", 40.)))
                .large_title("Terminal", &self.fold)
                .into_any_element()
        };
        div()
            .flex()
            .flex_col_reverse()
            .w(px(390.))
            .h(px(400.))
            .child(div().flex().flex_1().min_h_0().child(page))
            .child(super::settings_title_bar("Terminal", &self.fold, div()))
    }
}

fn large_title_folds(cx: &mut TestAppContext, virtual_rows: bool) {
    cx.update(crate::init);
    let fold = super::SettingsFold::default();
    let (_, cx) = cx.add_window_view(|_, _| LargeTitleTest {
        fold: fold.clone(),
        virtual_rows,
    });
    let cx: &mut VisualTestContext = cx;
    draw(cx);

    let bar = cx.debug_bounds("folding-title-bar").unwrap();
    let title = cx.debug_bounds("large-title").unwrap();
    assert!(title.top() >= bar.bottom());
    assert!(fold.scrolled() < 1.0);

    scroll(cx, 200., 300., 0., -10_000.);
    assert!(
        fold.scrolled() > 60.0,
        "the page scrolled its large title away, so the bar must fold (scrolled: {})",
        fold.scrolled()
    );
    assert!(
        cx.debug_bounds("large-title")
            .is_none_or(|title| title.bottom() <= bar.bottom())
    );
}

#[ala::test]
fn a_scrolled_page_folds_its_large_title_into_the_bar(cx: &mut TestAppContext) {
    large_title_folds(cx, false);
}

#[ala::test]
fn a_scrolled_virtual_page_folds_its_large_title_into_the_bar(cx: &mut TestAppContext) {
    large_title_folds(cx, true);
}
