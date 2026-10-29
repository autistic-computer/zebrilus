use ala::{
    Bounds, Context, IntoElement, Pixels, Render, TestAppContext, Window, canvas, div, prelude::*,
    px,
};
use std::{cell::Cell, rc::Rc};
use zz_ui::{
    ActiveTheme as _,
    pane::{PaneChrome, pane_surface},
    shell::{app_shell_surface, app_workspace_surface},
};

type PaintBounds = Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>;

struct ShellFixture {
    sidebar_width: Pixels,
    titlebar_height: Pixels,
    shadow: PaintBounds,
    content: PaintBounds,
}

impl Render for ShellFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shadow = self.shadow.clone();
        let content = self.content.clone();
        let pane = pane_surface(
            "test-pane",
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| {
                    content.set(Some((bounds, window.content_mask().bounds)));
                },
            )
            .size_full(),
            [],
            PaneChrome::new(px(10.).into(), true),
            cx,
        );
        let workspace = div().size_full().p(px(6.)).child(
            div()
                .relative()
                .size_full()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            shadow.set(Some((bounds, window.content_mask().bounds)));
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(pane),
        );
        app_shell_surface(
            "test-shell",
            cx.theme().background,
            div().flex_none().w(self.sidebar_width).h_full(),
            Some(div().flex_none().h(self.titlebar_height).into_any_element()),
            app_workspace_surface("test-workspace", workspace, [], cx),
            [],
        )
    }
}

#[ala::test]
fn pane_shadows_cross_chrome_boundaries_while_pane_content_stays_clipped(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    for (sidebar, titlebar) in [(80., 0.), (0., 40.), (80., 40.)] {
        let shadow = PaintBounds::default();
        let content = PaintBounds::default();
        let (_, cx) = cx.add_window_view({
            let shadow = shadow.clone();
            let content = content.clone();
            move |_, _| ShellFixture {
                sidebar_width: px(sidebar),
                titlebar_height: px(titlebar),
                shadow,
                content,
            }
        });
        cx.run_until_parked();
        let viewport = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.viewport_size()
        });
        let (bounds, mask) = shadow.get().expect("shadow painted");
        assert_eq!(mask, Bounds::new(ala::point(px(0.), px(0.)), viewport));
        if sidebar > 0. {
            assert!(mask.left() < bounds.left() - px(6.));
        }
        if titlebar > 0. {
            assert!(mask.top() < bounds.top() - px(6.));
        }
        let (bounds, mask) = content.get().expect("pane content painted");
        assert_eq!(mask, bounds);
    }
}
