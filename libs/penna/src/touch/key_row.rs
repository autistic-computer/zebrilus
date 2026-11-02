use std::rc::Rc;

use ala::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, Keystroke, Modifiers, MouseButton,
    ParentElement as _, RenderOnce, ScrollHandle, SharedString, Stateful, Styled, Window, canvas,
    div, linear_color_stop, linear_gradient, point, prelude::*, px,
};

use super::{
    arrow_pad::ArrowPad,
    concentric,
    popover_key::PopoverKey,
    sticky::{StickyModifier, StickyModifiers},
};
use crate::{
    ActiveTheme as _, Emphasis, Icon, IconName, Layer, PaintState, Shape, StyledExt as _, Surface,
    Theme, Tone, kbd::Kbd, rems_from_px, roles,
};

pub const KEY_ROW_HEIGHT: f32 = KEY_PILL_HEIGHT + 2.0 * KEY_ROW_MARGIN_Y;
pub const KEY_PILL_HEIGHT: f32 = KEY_SIZE + 2.0 * PILL_PADDING;
pub const KEY_ROW_MARGIN_X: f32 = 8.0;
pub const KEY_ROW_MARGIN_Y: f32 = 4.0;
pub(super) const KEY_SIZE: f32 = 40.0;
pub(super) const KEY_ICON: f32 = 18.0;
const KEY_MARK: f32 = 14.0;
const PILL_PADDING: f32 = 4.0;
const KEY_GAP: f32 = 2.0;
const LEAD_GAP: f32 = 8.0;
const MORE_WIDTH: f32 = 44.0;
const MORE_ICON: f32 = 14.0;
const MORE_STOP: f32 = 0.55;
const EDGE_SLACK: f32 = 1.0;
const KEY_GROUP: &str = "key-row-key";
pub(super) const KEY_SLOP: (f32, f32) = (KEY_GAP / 2.0, (KEY_ROW_HEIGHT - KEY_SIZE) / 2.0);
pub(super) const APPLE: bool = cfg!(any(target_os = "macos", target_os = "ios"));

pub(super) fn keystroke_glyph(keystroke: &Keystroke) -> AnyElement {
    let modifiers = keystroke.modifiers;
    if modifiers.shift || modifiers.platform || modifiers.function {
        return SharedString::from(Kbd::format(keystroke)).into_any_element();
    }
    let key = Kbd::format(&Keystroke {
        modifiers: Modifiers::default(),
        key: keystroke.key.clone(),
        key_char: None,
    });
    let mark = |modifier: StickyModifier| {
        let icon = modifier.icon();
        let tight = icon == IconName::KeyControl;
        div()
            .when(tight, |mark| mark.mr(rems_from_px(-3.0)))
            .child(Icon::new(icon).size(rems_from_px(KEY_MARK)))
    };
    div()
        .flex()
        .items_center()
        .gap(rems_from_px(1.0))
        .when(modifiers.control, |glyph| {
            glyph.child(mark(StickyModifier::Control))
        })
        .when(modifiers.alt, |glyph| {
            glyph.child(mark(StickyModifier::Alt))
        })
        .child(key)
        .into_any_element()
}

pub(super) fn key_glyph(label: SharedString, icon: Option<IconName>) -> AnyElement {
    match icon {
        Some(icon) => Icon::new(icon)
            .size(rems_from_px(KEY_ICON))
            .into_any_element(),
        None => label.into_any_element(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyLook {
    Rest,
    Latched,
    Inverted,
}

pub(super) fn key_look(look: KeyLook, theme: &Theme) -> (Surface, PaintState) {
    let ground = theme
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Rest)
        .ground;
    let key = Surface::new(roles::PHONE_KEY).on(ground);
    match look {
        KeyLook::Rest => (key, PaintState::Rest),
        KeyLook::Latched => (key, PaintState::Current),
        KeyLook::Inverted => (key.prominent(true), PaintState::Rest),
    }
}

pub(super) fn key_layer(surface: Surface, state: PaintState, theme: &Theme) -> Layer {
    let layer = theme.layer(surface, state);
    if theme.style(roles::PHONE_KEY).shape != Shape::Round {
        return layer;
    }
    let pill = theme
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Rest)
        .radius;
    Layer {
        radius: concentric(pill, ala::px(PILL_PADDING)),
        ..layer
    }
}

pub(super) fn key_surface<E: Styled>(element: E, look: KeyLook, cx: &App) -> E {
    let theme = cx.theme();
    let (surface, state) = key_look(look, theme);
    let element = element
        .h(rems_from_px(KEY_SIZE))
        .layer(&key_layer(surface, state, theme));
    if look == KeyLook::Inverted {
        element.font_semibold()
    } else {
        element
    }
}

#[derive(Clone)]
pub enum KeyRowKey {
    Hide,
    Key {
        label: SharedString,
        icon: Option<IconName>,
        keystroke: Keystroke,
    },
    Modifier(StickyModifier),
    Popover(Entity<PopoverKey>),
    Arrows(Entity<ArrowPad>),
}

impl KeyRowKey {
    pub fn text(label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self::Key {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: label.to_string(),
                key_char: Some(label.to_string()),
            },
            label,
            icon: None,
        }
    }

    pub fn named(label: impl Into<SharedString>, key: impl Into<String>) -> Self {
        Self::Key {
            label: label.into(),
            icon: None,
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: key.into(),
                key_char: None,
            },
        }
    }

    pub fn tab() -> Self {
        Self::named("tab", "tab").icon(if APPLE {
            IconName::KeyTab
        } else {
            IconName::KeyTabIso
        })
    }

    #[must_use]
    pub fn icon(mut self, glyph: IconName) -> Self {
        if let Self::Key { icon, .. } = &mut self {
            *icon = Some(glyph);
        }
        self
    }
}

type KeyHandler = Rc<dyn Fn(&Keystroke, &mut Window, &mut App)>;
type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct KeyRow {
    keys: Vec<KeyRowKey>,
    lead: Option<KeyRowKey>,
    scroll: Option<ScrollHandle>,
    on_key: Option<KeyHandler>,
    on_hide: Option<Handler>,
    bare: bool,
}

impl KeyRow {
    pub fn new(keys: Vec<KeyRowKey>) -> Self {
        Self {
            keys,
            lead: None,
            scroll: None,
            on_key: None,
            on_hide: None,
            bare: false,
        }
    }

    #[must_use]
    pub fn lead(mut self, key: KeyRowKey) -> Self {
        self.lead = Some(key);
        self
    }

    pub fn lead_room(&self) -> f32 {
        if self.lead.is_some() {
            KEY_PILL_HEIGHT + LEAD_GAP
        } else {
            0.0
        }
    }

    #[must_use]
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    #[must_use]
    pub fn track_scroll(mut self, scroll: &ScrollHandle) -> Self {
        self.scroll = Some(scroll.clone());
        self
    }

    #[must_use]
    pub fn on_key(mut self, f: impl Fn(&Keystroke, &mut Window, &mut App) + 'static) -> Self {
        self.on_key = Some(Rc::new(f));
        self
    }

    #[must_use]
    pub fn on_hide(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_hide = Some(Rc::new(f));
        self
    }

    fn slot(id: impl Into<ElementId>) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_1()
            .flex_shrink_0()
            .min_w(rems_from_px(KEY_SIZE + KEY_GAP))
            .items_center()
            .justify_center()
            .h_full()
    }

    fn cap(index: usize, look: KeyLook, content: impl IntoElement, cx: &App) -> Stateful<Div> {
        let selector = format!("key-row-key-{index}");
        let theme = cx.theme();
        let pressed = key_layer(key_look(look, theme).0, PaintState::Active, theme);
        Self::slot(ElementId::named_usize("key-row-key", index))
            .group(KEY_GROUP)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .child(
                key_surface(div().id("cap"), look, cx)
                    .debug_selector(move || selector)
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(rems_from_px(KEY_SIZE))
                    .whitespace_nowrap()
                    .relative()
                    .child(crate::touch::instant_press_highlight(
                        ElementId::named_usize("key-row-press", index),
                        pressed.fill,
                        pressed.radius,
                    ))
                    .child(content),
            )
    }

    fn key(&self, index: usize, key: KeyRowKey, sticky: StickyModifiers, cx: &App) -> AnyElement {
        match key {
            KeyRowKey::Hide => {
                let on_hide = self.on_hide.clone();
                Self::cap(
                    index,
                    KeyLook::Rest,
                    Icon::new(IconName::ChevronDown).size(rems_from_px(16.0)),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    if let Some(on_hide) = &on_hide {
                        on_hide(window, cx);
                    }
                })
                .into_any_element()
            }
            KeyRowKey::Key {
                label,
                icon,
                keystroke,
            } => {
                let on_key = self.on_key.clone();
                Self::cap(index, KeyLook::Rest, key_glyph(label, icon), cx)
                    .on_click(move |_, window, cx| {
                        let keystroke = StickyModifiers::apply(&keystroke, cx);
                        window.refresh();
                        if let Some(on_key) = &on_key {
                            on_key(&keystroke, window, cx);
                        }
                    })
                    .into_any_element()
            }
            KeyRowKey::Modifier(modifier) => {
                let look = if sticky.is_latched(modifier) {
                    KeyLook::Latched
                } else {
                    KeyLook::Rest
                };
                let label = match modifier {
                    StickyModifier::Control => "ctrl",
                    StickyModifier::Alt => "alt",
                };
                Self::cap(
                    index,
                    look,
                    key_glyph(label.into(), Some(modifier.icon())),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    StickyModifiers::toggle(modifier, cx);
                    window.refresh();
                })
                .into_any_element()
            }
            KeyRowKey::Popover(key) => Self::slot(ElementId::named_usize("key-row-slot", index))
                .child(key)
                .into_any_element(),
            KeyRowKey::Arrows(pad) => Self::slot(ElementId::named_usize("key-row-slot", index))
                .child(pad)
                .into_any_element(),
        }
    }
}

impl RenderOnce for KeyRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let sticky = StickyModifiers::get(cx);
        let keys: Vec<AnyElement> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, key)| self.key(index, key, sticky, cx))
            .collect();
        let lead = self
            .lead
            .clone()
            .map(|key| self.key(self.keys.len(), key, sticky, cx));
        let start = KEY_ROW_MARGIN_X + self.lead_room();
        let theme = cx.theme();
        let pill = theme.layer(Surface::new(roles::PHONE_PILL), PaintState::Rest);
        let shade = pill.ground.fill;
        let chevron = theme
            .paint_on(pill.ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest);
        let (more_back, more_ahead) = self.scroll.as_ref().map_or((false, false), edges);
        let more = |ahead: bool, scroll: ScrollHandle| {
            let (id, angle, icon) = if ahead {
                ("key-row-more-ahead", 90.0, IconName::ChevronRight)
            } else {
                ("key-row-more-back", 270.0, IconName::ChevronLeft)
            };
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .absolute()
                .top(rems_from_px(KEY_ROW_MARGIN_Y))
                .bottom(rems_from_px(KEY_ROW_MARGIN_Y))
                .w(rems_from_px(MORE_WIDTH))
                .map(|fade| {
                    if ahead {
                        fade.right(rems_from_px(KEY_ROW_MARGIN_X))
                            .rounded_tr(pill.radius)
                            .rounded_br(pill.radius)
                            .justify_end()
                            .pr(rems_from_px(10.0))
                    } else {
                        fade.left(rems_from_px(start))
                            .rounded_tl(pill.radius)
                            .rounded_bl(pill.radius)
                            .justify_start()
                            .pl(rems_from_px(10.0))
                    }
                })
                .flex()
                .items_center()
                .bg(linear_gradient(
                    angle,
                    linear_color_stop(shade.alpha(0.0), 0.0),
                    linear_color_stop(shade, MORE_STOP),
                ))
                .text_color(chevron)
                .block_mouse_except_scroll()
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(move |_, window, _| {
                    let x = if ahead {
                        -scroll.max_offset().x
                    } else {
                        px(0.0)
                    };
                    scroll.set_offset(point(x, scroll.offset().y));
                    window.refresh();
                })
                .child(Icon::new(icon).size(rems_from_px(MORE_ICON)))
        };
        div()
            .debug_selector(|| "key-row".to_owned())
            .relative()
            .flex_none()
            .w_full()
            .h(rems_from_px(KEY_ROW_HEIGHT))
            .font_family(theme.font_family.clone())
            .text_size(rems_from_px(15.0))
            .line_height(rems_from_px(20.0))
            .when_some(lead, |row, lead| {
                row.child(
                    div()
                        .debug_selector(|| "key-row-lead".to_owned())
                        .absolute()
                        .top(rems_from_px(KEY_ROW_MARGIN_Y))
                        .left(rems_from_px(KEY_ROW_MARGIN_X))
                        .size(rems_from_px(KEY_PILL_HEIGHT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .layer(&pill)
                        .child(lead),
                )
            })
            .when(!self.bare, |row| {
                row.child(
                    div()
                        .absolute()
                        .top(rems_from_px(KEY_ROW_MARGIN_Y))
                        .bottom(rems_from_px(KEY_ROW_MARGIN_Y))
                        .left(rems_from_px(start))
                        .right(rems_from_px(KEY_ROW_MARGIN_X))
                        .layer(&pill),
                )
            })
            .child(
                div()
                    .id("key-row")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(rems_from_px(start + PILL_PADDING))
                    .right(rems_from_px(KEY_ROW_MARGIN_X + PILL_PADDING))
                    .flex()
                    .items_center()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .when_some(
                        self.scroll.as_ref(),
                        StatefulInteractiveElement::track_scroll,
                    )
                    .children(keys),
            )
            .when_some(self.scroll.clone(), |row, scroll| {
                let watch = scroll.clone();
                row.when(more_back, |row| row.child(more(false, scroll.clone())))
                    .when(more_ahead, |row| row.child(more(true, scroll)))
                    .child(
                        canvas(
                            move |_, window, _| {
                                if edges(&watch) != (more_back, more_ahead) {
                                    window.refresh();
                                }
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_0(),
                    )
            })
    }
}

fn edges(scroll: &ScrollHandle) -> (bool, bool) {
    let offset = scroll.offset().x;
    let end = -scroll.max_offset().x;
    (
        offset < px(-EDGE_SLACK),
        scroll.max_offset().x > px(EDGE_SLACK) && offset > end + px(EDGE_SLACK),
    )
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use ala::{Context, Render, Subscription, TestAppContext, VisualTestContext};

    use super::*;
    use crate::touch::{PopoverKeyEvent, PopoverKeyItem, PopoverKeyTap};

    struct Keys {
        arrows: Entity<ArrowPad>,
        control: Entity<PopoverKey>,
        alt: Entity<PopoverKey>,
        lead: Entity<PopoverKey>,
        scroll: ScrollHandle,
    }

    impl Keys {
        fn new(cx: &mut App) -> Self {
            let scroll = ScrollHandle::new();
            let latch = |modifier: StickyModifier, label: &'static str, item, cx: &mut App| {
                cx.new(|cx| {
                    PopoverKey::new(label, PopoverKeyTap::Latch(modifier), cx)
                        .icon(modifier.icon())
                        .items(vec![PopoverKeyItem::new(item, item)])
                        .columns(2)
                        .scrolls(scroll.clone())
                })
            };
            Self {
                arrows: cx.new(ArrowPad::new),
                control: latch(StickyModifier::Control, "ctrl", "ctrl-c", cx),
                alt: latch(StickyModifier::Alt, "alt", "alt-b", cx),
                lead: cx.new(|cx| PopoverKey::new("lead", PopoverKeyTap::Press, cx)),
                scroll,
            }
        }

        fn row(&self) -> Vec<KeyRowKey> {
            vec![
                KeyRowKey::Popover(self.control.clone()),
                KeyRowKey::Popover(self.alt.clone()),
                KeyRowKey::tab(),
                KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
                KeyRowKey::text("/"),
                KeyRowKey::text("@"),
                KeyRowKey::Arrows(self.arrows.clone()),
                KeyRowKey::Hide,
            ]
        }
    }

    struct Host {
        keys: Keys,
        sent: Rc<RefCell<Vec<String>>>,
        width: Option<ala::Pixels>,
        _press: Subscription,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let sent = Rc::clone(&self.sent);
            let hide = Rc::clone(&self.sent);
            div().size_full().child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .when_some(self.width, Styled::w)
                    .when(self.width.is_none(), Styled::right_0)
                    .child(
                        KeyRow::new(self.keys.row())
                            .lead(KeyRowKey::Popover(self.keys.lead.clone()))
                            .track_scroll(&self.keys.scroll)
                            .on_key(move |keystroke, _, _| {
                                sent.borrow_mut().push(keystroke.unparse());
                            })
                            .on_hide(move |_, _| hide.borrow_mut().push("hide".into())),
                    ),
            )
        }
    }

    fn redraw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn host(cx: &mut TestAppContext) -> (ala::Entity<Host>, &mut VisualTestContext) {
        host_with_width(cx, None)
    }

    fn host_with_width(
        cx: &mut TestAppContext,
        width: Option<ala::Pixels>,
    ) -> (ala::Entity<Host>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (host, cx) = cx.add_window_view(move |_, cx| {
            let keys = Keys::new(cx);
            let sent: Rc<RefCell<Vec<String>>> = Rc::default();
            let pressed = Rc::clone(&sent);
            let press = cx.subscribe(&keys.lead, move |_, _, event: &PopoverKeyEvent, _| {
                if *event == PopoverKeyEvent::Press {
                    pressed.borrow_mut().push("lead".into());
                }
            });
            Host {
                keys,
                sent,
                width,
                _press: press,
            }
        });
        redraw(cx);
        (host, cx)
    }

    fn tap(cx: &mut VisualTestContext, selector: &'static str) {
        let center = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(center, Modifiers::none());
        redraw(cx);
    }

    #[ala::test]
    fn keys_send_through_the_sticky_latch(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        assert!(cx.debug_bounds("arrow-pad").is_some());

        tap(cx, "key-row-key-3");
        tap(cx, "popover-key-ctrl");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).control));
        tap(cx, "key-row-key-4");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).is_empty()));
        tap(cx, "key-row-key-5");
        tap(cx, "key-row-key-7");
        tap(cx, "popover-key-lead");
        let sent = host.read_with(cx, |host, _| host.sent.borrow().clone());
        assert_eq!(sent, ["escape", "ctrl-/", "@", "hide", "lead"]);
        assert!(cx.debug_bounds("popover-key-card").is_none());
    }

    #[ala::test]
    fn keys_take_taps_off_the_cap(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let row = cx.debug_bounds("key-row").expect("row");
        let esc = cx.debug_bounds("key-row-key-3").expect("esc");
        assert_eq!(row.size.height, ala::px(KEY_ROW_HEIGHT));
        assert_eq!(esc.size.height, ala::px(KEY_SIZE));
        for at in [
            ala::point(esc.center().x, row.top() + ala::px(1.0)),
            ala::point(esc.center().x, row.bottom() - ala::px(1.0)),
            ala::point(esc.right() + ala::px(KEY_SLOP.0 - 0.5), esc.center().y),
            ala::point(
                esc.left() - ala::px(KEY_SLOP.0 - 0.5),
                row.top() + ala::px(1.0),
            ),
        ] {
            cx.simulate_click(at, Modifiers::none());
            redraw(cx);
        }
        let sent = host.read_with(cx, |host, _| host.sent.borrow().clone());
        assert_eq!(sent, ["escape", "escape", "escape", "escape"]);

        let ctrl = cx.debug_bounds("popover-key-ctrl").expect("ctrl");
        let above = ala::point(ctrl.center().x, row.top() + ala::px(1.0));
        for phase in [ala::TouchPhase::Started, ala::TouchPhase::Ended] {
            cx.simulate_event(ala::TouchDragEvent {
                phase,
                start_position: above,
                position: above,
            });
            redraw(cx);
        }
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).control));
    }

    #[ala::test]
    fn keys_share_one_width_in_the_set_order(cx: &mut TestAppContext) {
        let (_, cx) = host(cx);
        let keys = [
            "popover-key-lead",
            "popover-key-ctrl",
            "popover-key-alt",
            "key-row-key-2",
            "key-row-key-3",
            "key-row-key-4",
            "key-row-key-5",
            "arrow-pad",
            "key-row-key-7",
        ]
        .map(|selector| cx.debug_bounds(selector).expect(selector));
        for key in &keys {
            assert_eq!(key.size.width, ala::px(KEY_SIZE));
        }
        for pair in keys.windows(2) {
            assert!(pair[0].right() < pair[1].left());
        }
    }

    #[ala::test]
    fn a_sideways_drag_on_a_card_key_scrolls_the_row(cx: &mut TestAppContext) {
        let (host, cx) = host_with_width(cx, Some(ala::px(200.0)));
        let scroll = host.read_with(cx, |host, _| host.keys.scroll.clone());
        assert!(scroll.max_offset().x > ala::px(0.0));
        let alt = cx.debug_bounds("popover-key-alt").expect("alt").center();
        let drag = |phase, x: f32, cx: &mut VisualTestContext| {
            cx.simulate_event(ala::TouchDragEvent {
                phase,
                start_position: alt,
                position: ala::point(alt.x + ala::px(x), alt.y),
            });
            redraw(cx);
        };
        drag(ala::TouchPhase::Started, 0.0, cx);
        drag(ala::TouchPhase::Moved, -20.0, cx);
        drag(ala::TouchPhase::Moved, -60.0, cx);
        drag(ala::TouchPhase::Ended, -60.0, cx);
        assert_eq!(scroll.offset().x, ala::px(-40.0));
        assert!(cx.debug_bounds("popover-key-item-alt-b").is_none());
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).is_empty()));
    }

    #[ala::test]
    fn the_lead_key_stays_put_while_the_row_scrolls(cx: &mut TestAppContext) {
        let (host, cx) = host_with_width(cx, Some(ala::px(200.0)));
        let scroll = host.read_with(cx, |host, _| host.keys.scroll.clone());
        let lead = cx.debug_bounds("popover-key-lead").expect("lead");
        scroll.set_offset(ala::point(-scroll.max_offset().x, ala::px(0.0)));
        redraw(cx);
        assert_eq!(cx.debug_bounds("popover-key-lead"), Some(lead));
        let ctrl = cx.debug_bounds("popover-key-ctrl");
        assert!(ctrl.is_none_or(|ctrl| ctrl.right() <= lead.right()));
    }

    #[ala::test]
    fn a_fade_and_chevron_mark_the_side_with_more_keys(cx: &mut TestAppContext) {
        let (host, cx) = host_with_width(cx, Some(ala::px(200.0)));
        redraw(cx);
        assert!(cx.debug_bounds("key-row-more-ahead").is_some());
        assert!(cx.debug_bounds("key-row-more-back").is_none());

        tap(cx, "key-row-more-ahead");
        redraw(cx);
        let scroll = host.read_with(cx, |host, _| host.keys.scroll.clone());
        assert_eq!(scroll.offset().x, -scroll.max_offset().x);
        assert!(host.read_with(cx, |host, _| host.sent.borrow().is_empty()));
        assert!(cx.debug_bounds("key-row-more-ahead").is_none());
        assert!(cx.debug_bounds("key-row-more-back").is_some());
    }

    #[ala::test]
    fn a_row_that_fits_has_no_fades(cx: &mut TestAppContext) {
        let (_, cx) = host(cx);
        redraw(cx);
        assert!(cx.debug_bounds("key-row-more-ahead").is_none());
        assert!(cx.debug_bounds("key-row-more-back").is_none());
    }
}
