use ala::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollDelta,
    ScrollWheelEvent, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
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
