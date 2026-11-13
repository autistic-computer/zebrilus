use std::rc::Rc;

use ala::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, Keystroke, Modifiers, MouseButton,
    ParentElement as _, RenderOnce, ScrollHandle, SharedString, Stateful, Styled, Window, canvas,
    div, prelude::*, px,
};

use super::{
    arrow_pad::ArrowPad,
    concentric,
    stack_key::StackKey,
    sticky::{StickyModifier, StickyModifiers},
};
use crate::{
    ActiveTheme as _, Icon, IconName, Kind, Layer, PaintState, Shape, Space, StyledExt as _,
    Surface, Theme, components::kbd::Kbd, foundation::roles, rems_from_px,
};

pub const KEY_ROW_HEIGHT: f32 = KEY_PILL_HEIGHT + 2.0 * KEY_ROW_MARGIN_Y;
pub const KEY_PILL_HEIGHT: f32 = KEY_SIZE + 2.0 * PILL_PADDING;
pub const KEY_ROW_MARGIN_X: f32 = Space::S16.design_px();
pub const KEY_ROW_MARGIN_Y: f32 = Space::S8.design_px();
pub(super) const KEY_SIZE: f32 = 44.0;
pub(super) const KEY_ICON: f32 = 20.0;
const KEY_MARK: f32 = 14.0;
const PILL_PADDING: f32 = Space::S6.design_px();
const KEY_GAP: f32 = Space::S2.design_px();
const PART_GAP: f32 = Space::GAP_RELATED.design_px();
const SLOT: f32 = KEY_SIZE + KEY_GAP;
const FADE: f32 = Space::S16.design_px();
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
        .gap(Space::S2)
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

#[must_use]
pub fn key_pill_layer(theme: &Theme) -> Layer {
    let floating = theme.layer(Surface::new(Kind::Floating.role()), PaintState::Rest);
    let radius = if theme.pill_controls() {
        ala::FULL_CORNER_RADIUS
    } else {
        theme.control_corner(px(KEY_PILL_HEIGHT))
    };
    Layer {
        radius,
        share: None,
        ..floating
    }
}

pub(super) fn key_look(look: KeyLook, theme: &Theme) -> (Surface, PaintState) {
    let key = Surface::new(roles::PHONE_KEY).on(key_pill_layer(theme).ground);
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
    Layer {
        radius: concentric(key_pill_layer(theme).radius, px(PILL_PADDING)),
        share: None,
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
        element.font_weight(crate::TextStyle::TouchSmall.strong_weight())
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
    Stack(Entity<StackKey>),
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
    trail: Vec<KeyRowKey>,
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
            trail: Vec::new(),
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

    #[must_use]
    pub fn trail(mut self, keys: Vec<KeyRowKey>) -> Self {
        self.trail = keys;
        self
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
            .flex_none()
            .w(rems_from_px(SLOT))
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
            .click_cursor()
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
                    .child(crate::blocks::touch::instant_press_highlight(
                        ElementId::named_usize("key-row-press", index),
                        pressed.fill,
                        pressed.radius,
                    ))
                    .child(content),
            )
    }

    fn key(
        &self,
        index: usize,
        key: KeyRowKey,
        sticky: StickyModifiers,
        cx: &App,
    ) -> Stateful<Div> {
        match key {
            KeyRowKey::Hide => {
                let on_hide = self.on_hide.clone();
                Self::cap(
                    index,
                    KeyLook::Rest,
                    Icon::new(IconName::ChevronDown).size(rems_from_px(KEY_ICON)),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    if let Some(on_hide) = &on_hide {
                        on_hide(window, cx);
                    }
                })
            }
            KeyRowKey::Key {
                label,
                icon,
                keystroke,
            } => {
                let on_key = self.on_key.clone();
                Self::cap(index, KeyLook::Rest, key_glyph(label, icon), cx).on_click(
                    move |_, window, cx| {
                        let keystroke = StickyModifiers::apply(&keystroke, cx);
                        window.refresh();
                        if let Some(on_key) = &on_key {
                            on_key(&keystroke, window, cx);
                        }
                    },
                )
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
            }
            KeyRowKey::Stack(key) => {
                Self::slot(ElementId::named_usize("key-row-slot", index)).child(key)
            }
            KeyRowKey::Arrows(pad) => {
                Self::slot(ElementId::named_usize("key-row-slot", index)).child(pad)
            }
        }
    }

    fn pinned(keys: Vec<Stateful<Div>>) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .mx(rems_from_px(-KEY_GAP / 2.0))
            .children(keys)
    }
}

fn faded(near: f32) -> f32 {
    let area = |x: f32| {
        if x <= FADE {
            x * x / (2.0 * FADE)
        } else {
            FADE / 2.0 + x - FADE
        }
    };
    let far = near + KEY_SIZE;
    if far <= 0.0 {
        return 0.0;
    }
    let near = near.max(0.0);
    ((area(far) - area(near)) / (far - near)).clamp(0.0, 1.0)
}

fn key_opacity(index: usize, offset: f32, width: f32, (back, ahead): (bool, bool)) -> f32 {
    if width <= 0.0 {
        return 1.0;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "a key row holds a handful of keys"
    )]
    let left = index as f32 * SLOT + KEY_GAP / 2.0 + offset;
    let start = if back {
        faded(left - KEY_GAP / 2.0)
    } else {
        1.0
    };
    let end = if ahead {
        faded(width - KEY_GAP / 2.0 - left - KEY_SIZE)
    } else {
        1.0
    };
    start.min(end)
}

impl RenderOnce for KeyRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let sticky = StickyModifiers::get(cx);
        let edges = self.scroll.as_ref().map_or((false, false), edges);
        let scrolling = self.keys.len();
        let keys: Vec<AnyElement> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, key)| {
                let opacity = self.scroll.as_ref().map_or(1.0, |scroll| {
                    key_opacity(
                        index,
                        f32::from(scroll.offset().x),
                        f32::from(scroll.bounds().size.width),
                        edges,
                    )
                });
                self.key(index, key, sticky, cx)
                    .when(opacity < 1.0, |slot| slot.opacity(opacity))
                    .into_any_element()
            })
            .collect();
        let lead = self
            .lead
            .clone()
            .map(|key| Self::pinned(vec![self.key(scrolling, key, sticky, cx)]));
        let trail: Vec<Stateful<Div>> = self
            .trail
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, key)| self.key(scrolling + 1 + index, key, sticky, cx))
            .collect();
        let trail = (!trail.is_empty()).then(|| Self::pinned(trail));
        let theme = cx.theme();
        let pill = key_pill_layer(theme);
        div()
            .debug_selector(|| "key-row".to_owned())
            .relative()
            .flex_none()
            .w_full()
            .h(rems_from_px(KEY_ROW_HEIGHT))
            .font_family(theme.font_family.clone())
            .type_style(crate::TextStyle::TouchSmall, cx)
            .when(!self.bare, |row| {
                row.child(
                    div()
                        .debug_selector(|| "key-row-pill".to_owned())
                        .absolute()
                        .top(rems_from_px(KEY_ROW_MARGIN_Y))
                        .bottom(rems_from_px(KEY_ROW_MARGIN_Y))
                        .left(rems_from_px(KEY_ROW_MARGIN_X))
                        .right(rems_from_px(KEY_ROW_MARGIN_X))
                        .layer(&pill),
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(rems_from_px(KEY_ROW_MARGIN_X + PILL_PADDING))
                    .right(rems_from_px(KEY_ROW_MARGIN_X + PILL_PADDING))
                    .flex()
                    .items_center()
                    .gap(rems_from_px(PART_GAP))
                    .children(lead)
                    .child(
                        div()
                            .id("key-row")
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .mx(rems_from_px(-KEY_GAP / 2.0))
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .when_some(
                                self.scroll.as_ref(),
                                StatefulInteractiveElement::track_scroll,
                            )
                            .children(keys),
                    )
                    .children(trail),
            )
            .when_some(self.scroll.clone(), |row, scroll| {
                let offset = scroll.offset().x;
                row.child(
                    canvas(
                        move |_, window, _| {
                            if self::edges(&scroll) != edges || scroll.offset().x != offset {
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

    use ala::{Context, Render, TestAppContext, VisualTestContext};

    use super::*;
    use crate::blocks::touch::{StackChip, StackGroup, StackKeyTap};

    struct Keys {
        arrows: Entity<ArrowPad>,
        control: Entity<StackKey>,
        alt: Entity<StackKey>,
        lead: Entity<StackKey>,
        scroll: ScrollHandle,
    }

    impl Keys {
        fn new(cx: &mut App) -> Self {
            let scroll = ScrollHandle::new();
            let latch = |modifier: StickyModifier, label: &'static str, item, cx: &mut App| {
                cx.new(|cx| {
                    StackKey::new(label, StackKeyTap::Latch(modifier), cx)
                        .icon(modifier.icon())
                        .scrolls(scroll.clone())
                        .groups(
                            vec![StackGroup::new([StackChip::chord(item, item, item)])],
                            cx,
                        )
                })
            };
            Self {
                arrows: cx.new(ArrowPad::new),
                control: latch(StickyModifier::Control, "ctrl", "ctrl-c", cx),
                alt: latch(StickyModifier::Alt, "alt", "alt-b", cx),
                lead: cx.new(|cx| StackKey::new("lead", StackKeyTap::Open, cx)),
                scroll,
            }
        }

        fn row(&self) -> Vec<KeyRowKey> {
            vec![
                KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
                KeyRowKey::tab(),
                KeyRowKey::Stack(self.control.clone()),
                KeyRowKey::Stack(self.alt.clone()),
                KeyRowKey::text("/"),
                KeyRowKey::text("@"),
            ]
        }

        fn trail(&self) -> Vec<KeyRowKey> {
            vec![KeyRowKey::Arrows(self.arrows.clone()), KeyRowKey::Hide]
        }
    }

    struct Host {
        keys: Keys,
        sent: Rc<RefCell<Vec<String>>>,
        width: Option<ala::Pixels>,
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
                            .lead(KeyRowKey::Stack(self.keys.lead.clone()))
                            .trail(self.keys.trail())
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
        let (host, cx) = cx.add_window_view(move |_, cx| Host {
            keys: Keys::new(cx),
            sent: Rc::default(),
            width,
        });
        redraw(cx);
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

        tap(cx, "key-row-key-0");
        tap(cx, "stack-key-ctrl");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).control));
        tap(cx, "key-row-key-4");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).is_empty()));
        tap(cx, "key-row-key-5");
        tap(cx, "key-row-key-8");
        tap(cx, "stack-key-lead");
        let sent = host.read_with(cx, |host, _| host.sent.borrow().clone());
        assert_eq!(sent, ["escape", "ctrl-/", "@", "hide"]);
    }

    #[ala::test]
    fn keys_take_taps_off_the_cap(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let row = cx.debug_bounds("key-row").expect("row");
        let pill = cx.debug_bounds("key-row-pill").expect("pill");
        let esc = cx.debug_bounds("key-row-key-0").expect("esc");
        assert_eq!(row.size.height, ala::px(KEY_ROW_HEIGHT));
        assert_eq!(pill.size.height, ala::px(56.0));
        assert_eq!(pill.bottom(), row.bottom() - ala::px(8.0));
        assert_eq!(esc.size.height, ala::px(KEY_SIZE));
        assert_eq!(esc.size.width, ala::px(44.0));
        for at in [
            ala::point(esc.center().x, row.top() + ala::px(1.0)),
            ala::point(esc.center().x, row.bottom() - ala::px(1.0)),
            ala::point(esc.right() + ala::px(KEY_SLOP.0 - 0.5), esc.center().y),
        ] {
            cx.simulate_click(at, Modifiers::none());
            redraw(cx);
        }
        let sent = host.read_with(cx, |host, _| host.sent.borrow().clone());
        assert_eq!(sent, ["escape", "escape", "escape"]);

        let ctrl = cx.debug_bounds("stack-key-ctrl").expect("ctrl");
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
    fn the_prefix_and_the_end_keys_hold_the_ends_of_one_pill(cx: &mut TestAppContext) {
        let (host, cx) = host_with_width(cx, Some(ala::px(393.0)));
        let pill = cx.debug_bounds("key-row-pill").expect("pill");
        let lead = cx.debug_bounds("stack-key-lead").expect("lead");
        let esc = cx.debug_bounds("key-row-key-0").expect("esc");
        let arrows = cx.debug_bounds("arrow-pad").expect("arrows");
        let hide = cx.debug_bounds("key-row-key-8").expect("hide");
        assert_eq!(pill.left(), ala::px(16.0));
        assert_eq!(pill.right(), ala::px(393.0 - 16.0));
        assert_eq!(lead.left() - pill.left(), ala::px(6.0));
        assert_eq!(esc.left() - lead.right(), ala::px(8.0));
        assert_eq!(hide.left() - arrows.right(), ala::px(2.0));
        assert_eq!(pill.right() - hide.right(), ala::px(6.0));

        let scroll = host.read_with(cx, |host, _| host.keys.scroll.clone());
        assert!(scroll.max_offset().x > ala::px(0.0));
        scroll.set_offset(ala::point(-scroll.max_offset().x, ala::px(0.0)));
        redraw(cx);
        assert_eq!(cx.debug_bounds("stack-key-lead"), Some(lead));
        assert_eq!(cx.debug_bounds("arrow-pad"), Some(arrows));
        assert_eq!(cx.debug_bounds("key-row-key-8"), Some(hide));
    }

    #[ala::test]
    fn a_sideways_drag_on_a_card_key_scrolls_the_row(cx: &mut TestAppContext) {
        let (host, cx) = host_with_width(cx, Some(ala::px(340.0)));
        let scroll = host.read_with(cx, |host, _| host.keys.scroll.clone());
        assert!(scroll.max_offset().x > ala::px(0.0));
        let ctrl = cx.debug_bounds("stack-key-ctrl").expect("ctrl").center();
        let drag = |phase, x: f32, cx: &mut VisualTestContext| {
            cx.simulate_event(ala::TouchDragEvent {
                phase,
                start_position: ctrl,
                position: ala::point(ctrl.x + ala::px(x), ctrl.y),
            });
            redraw(cx);
        };
        drag(ala::TouchPhase::Started, 0.0, cx);
        drag(ala::TouchPhase::Moved, -20.0, cx);
        drag(ala::TouchPhase::Moved, -60.0, cx);
        drag(ala::TouchPhase::Ended, -60.0, cx);
        assert_eq!(scroll.offset().x, ala::px(-40.0));
        assert!(!host.read_with(cx, |host, cx| host.keys.control.read(cx).is_open(cx)));
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).is_empty()));
    }

    #[test]
    fn keys_fade_out_toward_an_edge_where_more_wait() {
        let ahead = (false, true);
        assert_eq!(key_opacity(0, 0.0, 199.0, ahead), 1.0);
        assert_eq!(key_opacity(3, 0.0, 199.0, ahead), (51.0 - 7.031_25) / 44.0);
        assert_eq!(key_opacity(4, 0.0, 199.0, ahead), 13.0 / 32.0);
        assert_eq!(key_opacity(5, 0.0, 199.0, ahead), 0.0);
        assert_eq!(key_opacity(0, -36.0, 199.0, (true, true)), 4.0 / 16.0);
        assert_eq!(key_opacity(4, 0.0, 199.0, (false, false)), 1.0);
    }
}
