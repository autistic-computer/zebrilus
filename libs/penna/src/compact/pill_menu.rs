use std::rc::Rc;

use ala::{
    App, Bounds, Div, ElementId, FULL_CORNER_RADIUS, IntoElement, MouseButton, ParentElement as _,
    Pixels, SharedString, Styled as _, Window, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, Layer, PaintState, StyledExt as _,
    Surface, Tone, rems_from_px, roles,
};

const CHIP_ICON: f32 = 18.0;
const STACK_GAP: f32 = 12.0;
const SPACINGS: [Spacing; 4] = [
    Spacing {
        chip: 44.0,
        gap: 8.0,
        group: 16.0,
    },
    Spacing {
        chip: 44.0,
        gap: 4.0,
        group: 10.0,
    },
    Spacing {
        chip: 40.0,
        gap: 4.0,
        group: 8.0,
    },
    Spacing {
        chip: 36.0,
        gap: 2.0,
        group: 6.0,
    },
];
const CLOSE_ICON: f32 = 22.0;
const RISE: f32 = 14.0;
const STAGGER: f32 = 0.06;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PillMenuLook {
    #[default]
    Rest,
    Quiet,
    Danger,
    Primary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PillMenuItem {
    pub id: SharedString,
    pub label: SharedString,
    pub icon: IconName,
    pub look: PillMenuLook,
}

impl PillMenuItem {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon,
            look: PillMenuLook::Rest,
        }
    }

    #[must_use]
    pub fn look(mut self, look: PillMenuLook) -> Self {
        self.look = look;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Spacing {
    chip: f32,
    gap: f32,
    group: f32,
}

impl Spacing {
    fn fitting(chips: usize, groups: usize, room: Pixels) -> Self {
        let gaps = chips.saturating_sub(groups) as f32;
        let breaks = groups.saturating_sub(1) as f32;
        let height = |spacing: &Self| {
            px(chips as f32 * spacing.chip + gaps * spacing.gap + breaks * spacing.group)
        };
        let tightest = SPACINGS[SPACINGS.len() - 1];
        SPACINGS
            .into_iter()
            .find(|spacing| height(spacing) <= room)
            .unwrap_or(tightest)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillMenuAlign {
    Center,
    Start,
}

#[allow(clippy::too_many_arguments)]
pub fn compact_pill_menu(
    groups: Vec<Vec<PillMenuItem>>,
    close: Bounds<Pixels>,
    top: Pixels,
    align: PillMenuAlign,
    progress: f32,
    on_pick: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let progress = progress.clamp(0.0, 1.0);
    let theme = cx.theme();
    let on_pick: Rc<dyn Fn(&SharedString, &mut Window, &mut App)> = Rc::new(on_pick);
    let on_close: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(on_close);
    let groups: Vec<_> = groups
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect();
    let count = groups.iter().map(Vec::len).sum::<usize>();
    let room = (close.top() - top - px(STACK_GAP)).max(px(0.0));
    let spacing = Spacing::fitting(count, groups.len(), room);
    let span = 1.0 + STAGGER * count.saturating_sub(1) as f32;
    let mut above = count;
    let groups: Vec<Div> = groups
        .into_iter()
        .map(|group| {
            let chips: Vec<_> = group
                .into_iter()
                .map(|item| {
                    above -= 1;
                    let shown = (progress * span - STAGGER * above as f32).clamp(0.0, 1.0);
                    chip(item, shown, spacing.chip, Rc::clone(&on_pick), cx)
                })
                .collect();
            aligned(div().flex().flex_col().gap(px(spacing.gap)), align).children(chips)
        })
        .collect();
    let column = match align {
        PillMenuAlign::Center => div().left_0().w(close.center().x * 2.0),
        PillMenuAlign::Start => div().left(close.left()).right_0(),
    };
    let column = aligned(
        column
            .debug_selector(|| "pill-menu-stack".to_owned())
            .absolute()
            .top(top)
            .h(room)
            .flex()
            .flex_col()
            .justify_end()
            .gap(px(spacing.group)),
        align,
    )
    .children(groups);
    let button = Surface::new(roles::PHONE_PILL).prominent(true);
    let button_layer = Layer {
        radius: FULL_CORNER_RADIUS,
        ..theme.layer(button, PaintState::Rest)
    };
    let pressed = theme.layer(button, PaintState::Active).fill;
    let diameter = close.size.height.min(close.size.width);
    let center = close.center();
    let dismiss = Rc::clone(&on_close);
    let shut = on_close;
    div()
        .debug_selector(|| "pill-menu".to_owned())
        .absolute()
        .inset_0()
        .font_family(theme.font_family.clone())
        .child(
            div()
                .id("pill-menu-scrim")
                .debug_selector(|| "pill-menu-scrim".to_owned())
                .absolute()
                .inset_0()
                .occlude()
                .bg(theme.scrim)
                .opacity(progress)
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(move |_, window, cx| dismiss(window, cx)),
        )
        .child(column)
        .child(
            div()
                .absolute()
                .left(center.x - diameter / 2.0)
                .top(center.y - diameter / 2.0)
                .size(diameter)
                .rounded_full()
                .bg(theme.background.opaque())
                .opacity(progress),
        )
        .child(
            div()
                .id("pill-menu-close")
                .debug_selector(|| "pill-menu-close".to_owned())
                .absolute()
                .left(center.x - diameter / 2.0)
                .top(center.y - diameter / 2.0)
                .size(diameter)
                .flex()
                .items_center()
                .justify_center()
                .layer(&button_layer)
                .text_color(button_layer.text)
                .opacity(progress)
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .child(crate::touch::press_highlight(
                    "pill-menu-close-press",
                    pressed,
                    FULL_CORNER_RADIUS,
                ))
                .child(Icon::new(IconName::Xmark).size(rems_from_px(CLOSE_ICON)))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    shut(window, cx);
                }),
        )
}

fn aligned(column: Div, align: PillMenuAlign) -> Div {
    match align {
        PillMenuAlign::Center => column.items_center(),
        PillMenuAlign::Start => column.items_start(),
    }
}

fn chip(
    item: PillMenuItem,
    shown: f32,
    height: f32,
    on_pick: Rc<dyn Fn(&SharedString, &mut Window, &mut App)>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let surface = match item.look {
        PillMenuLook::Primary => Surface::new(roles::PHONE_PILL)
            .tone(Tone::Primary)
            .prominent(true),
        _ => Surface::new(roles::PHONE_PILL),
    };
    let layer = Layer {
        radius: FULL_CORNER_RADIUS,
        ..theme.layer(surface, PaintState::Rest)
    };
    let pressed = theme.layer(surface, PaintState::Active).fill;
    let ink = match item.look {
        PillMenuLook::Danger => {
            theme
                .paint_on(layer.ground, Tone::Danger, Emphasis::Ghost)
                .text
        }
        PillMenuLook::Quiet => theme
            .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest),
        PillMenuLook::Rest | PillMenuLook::Primary => layer.text,
    };
    let selector = format!("pill-menu-item-{}", item.id);
    let id = item.id.clone();
    div()
        .id(ElementId::Name(
            format!("pill-menu-item-{}", item.id).into(),
        ))
        .debug_selector(move || selector)
        .relative()
        .top(px(RISE * (1.0 - shown)))
        .opacity(shown)
        .flex()
        .flex_none()
        .items_center()
        .gap(rems_from_px(10.0))
        .h(px(height))
        .pl(rems_from_px(14.0))
        .pr(rems_from_px(18.0))
        .whitespace_nowrap()
        .layer(&layer)
        .text_color(ink)
        .text_size(rems_from_px(15.0))
        .line_height(rems_from_px(20.0))
        .font_medium()
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(crate::touch::press_highlight(
            ElementId::Name(format!("pill-menu-press-{}", item.id).into()),
            pressed,
            FULL_CORNER_RADIUS,
        ))
        .child(Icon::new(item.icon).size(rems_from_px(CHIP_ICON)))
        .child(item.label)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_pick(&id, window, cx);
        })
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use ala::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, size};

    use super::*;

    const TOP: f32 = 54.0;

    struct Host {
        log: Rc<RefCell<Vec<String>>>,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let picks = Rc::clone(&self.log);
            let closes = Rc::clone(&self.log);
            let height = window.viewport_size().height;
            let close = Bounds::new(
                point(px(16.0), height - px(90.0)),
                size(px(358.0), px(56.0)),
            );
            let items = (0..8)
                .map(|index| {
                    PillMenuItem::new(
                        format!("item-{index}"),
                        format!("Item {index}"),
                        IconName::Plus,
                    )
                })
                .collect();
            div().size_full().child(compact_pill_menu(
                vec![
                    vec![
                        PillMenuItem::new("delete", "Delete", IconName::Xmark)
                            .look(PillMenuLook::Danger),
                    ],
                    items,
                ],
                close,
                px(TOP),
                PillMenuAlign::Center,
                1.0,
                move |id, _, _| picks.borrow_mut().push(id.to_string()),
                move |_, _| closes.borrow_mut().push("close".into()),
                cx,
            ))
        }
    }

    fn redraw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn host(cx: &mut TestAppContext, height: f32) -> (ala::Entity<Host>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (host, cx) = cx.add_window_view(|_, _| Host { log: Rc::default() });
        cx.simulate_resize(size(px(402.0), px(height)));
        redraw(cx);
        (host, cx)
    }

    #[test]
    fn the_stack_draws_closer_together_when_room_is_short() {
        assert_eq!(Spacing::fitting(8, 2, px(600.0)), SPACINGS[0]);
        assert_eq!(Spacing::fitting(9, 2, px(440.0)), SPACINGS[1]);
        assert_eq!(Spacing::fitting(9, 2, px(100.0)), SPACINGS[3]);
    }

    #[ala::test]
    fn chips_stay_under_the_top_and_above_the_button(cx: &mut TestAppContext) {
        let (_, cx) = host(cx, 520.0);
        let close = cx.debug_bounds("pill-menu-close").expect("close");
        let first = cx.debug_bounds("pill-menu-item-delete").expect("first");
        let last = cx.debug_bounds("pill-menu-item-item-7").expect("last");
        assert!(first.top() >= px(TOP));
        assert!(last.bottom() < close.top());
        assert_eq!(close.size, size(px(56.0), px(56.0)));
        assert_eq!(last.center().x, close.center().x);
    }

    #[ala::test]
    fn a_chip_picks_and_the_scrim_closes(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, 800.0);
        let log =
            |cx: &mut VisualTestContext| host.read_with(cx, |host, _| host.log.borrow().clone());
        let chip = cx.debug_bounds("pill-menu-item-item-3").expect("chip");
        cx.simulate_click(chip.center(), Modifiers::none());
        redraw(cx);
        assert_eq!(log(cx), ["item-3"]);
        let scrim = cx.debug_bounds("pill-menu-scrim").expect("scrim");
        cx.simulate_click(
            point(scrim.left() + px(4.0), scrim.top() + px(4.0)),
            Modifiers::none(),
        );
        redraw(cx);
        assert_eq!(log(cx), ["item-3", "close"]);
        let close = cx.debug_bounds("pill-menu-close").expect("close");
        cx.simulate_click(close.center(), Modifiers::none());
        redraw(cx);
        assert_eq!(log(cx), ["item-3", "close", "close"]);
    }
}
