use std::rc::Rc;

use ala::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, Keystroke, Modifiers, MouseButton,
    ParentElement as _, RenderOnce, SharedString, Stateful, Styled, Window, div, prelude::*, px,
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
pub const KEY_DISC: f32 = KEY_PILL_HEIGHT;
pub const KEY_DISC_GAP: f32 = Space::GAP_RELATED.design_px();
pub(super) const KEY_SIZE: f32 = 48.0;
pub(super) const KEY_ICON: f32 = 20.0;
pub(super) const DISC_ICON: f32 = 22.0;
const KEY_MARK: f32 = 14.0;
const PILL_PADDING: f32 = Space::S4.design_px();
const KEY_GAP: f32 = Space::S2.design_px();
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
    On,
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

fn disc_layer(theme: &Theme) -> Layer {
    Layer {
        radius: ala::FULL_CORNER_RADIUS,
        ..key_pill_layer(theme)
    }
}

pub(super) fn key_look(look: KeyLook, theme: &Theme) -> (Surface, PaintState) {
    let ground = key_pill_layer(theme).ground;
    match look {
        KeyLook::Rest => (Surface::new(roles::PHONE_KEY).on(ground), PaintState::Rest),
        KeyLook::On => (
            Surface::new(roles::PHONE_PILL_ICON).on(ground),
            PaintState::Rest,
        ),
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
    element
        .h(rems_from_px(KEY_SIZE))
        .layer(&key_layer(surface, state, theme))
}

#[derive(Clone)]
pub enum KeyRowKey {
    Done,
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
    discs: Vec<KeyRowKey>,
    on_key: Option<KeyHandler>,
    on_done: Option<Handler>,
    bare: bool,
}

impl KeyRow {
    pub fn new(keys: Vec<KeyRowKey>) -> Self {
        Self {
            keys,
            discs: Vec::new(),
            on_key: None,
            on_done: None,
            bare: false,
        }
    }

    #[must_use]
    pub fn discs(mut self, keys: Vec<KeyRowKey>) -> Self {
        self.discs = keys;
        self
    }

    #[must_use]
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    #[must_use]
    pub fn on_key(mut self, f: impl Fn(&Keystroke, &mut Window, &mut App) + 'static) -> Self {
        self.on_key = Some(Rc::new(f));
        self
    }

    #[must_use]
    pub fn on_done(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_done = Some(Rc::new(f));
        self
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "a key row holds a couple of discs"
    )]
    pub fn pill_end(&self) -> f32 {
        self.discs.len() as f32 * (KEY_DISC + KEY_DISC_GAP)
    }

    fn slot(id: impl Into<ElementId>) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .px(rems_from_px(KEY_GAP / 2.0))
            .h_full()
    }

    fn cap(index: usize, look: KeyLook, content: impl IntoElement, cx: &App) -> Stateful<Div> {
        let selector = format!("key-row-key-{index}");
        let theme = cx.theme();
        let (surface, state) = key_look(KeyLook::On, theme);
        let pressed = key_layer(surface, state, theme);
        Self::slot(ElementId::named_usize("key-row-key", index))
            .group(KEY_GROUP)
            .click_cursor()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .child(
                key_surface(div().id("cap"), look, cx)
                    .debug_selector(move || selector)
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
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
            KeyRowKey::Done => {
                let on_done = self.on_done.clone();
                Self::cap(
                    index,
                    KeyLook::Rest,
                    Icon::new(IconName::Check).size(rems_from_px(DISC_ICON)),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    if let Some(on_done) = &on_done {
                        on_done(window, cx);
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
                        cx.haptic(ala::Haptic::Light);
                        let keystroke = StickyModifiers::apply(&keystroke, cx);
                        window.refresh();
                        if let Some(on_key) = &on_key {
                            on_key(&keystroke, window, cx);
                        }
                    },
                )
            }
            KeyRowKey::Modifier(modifier) => {
                let label = match modifier {
                    StickyModifier::Control => "ctrl",
                    StickyModifier::Alt => "alt",
                    StickyModifier::Command => "cmd",
                };
                let look = if sticky.is_latched(modifier) {
                    KeyLook::On
                } else {
                    KeyLook::Rest
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
            KeyRowKey::Stack(key) => Self::slot(ElementId::named_usize("key-row-slot", index))
                .child(div().flex_1().child(key)),
            KeyRowKey::Arrows(pad) => Self::slot(ElementId::named_usize("key-row-slot", index))
                .child(div().flex_1().child(pad)),
        }
    }
}

impl RenderOnce for KeyRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let sticky = StickyModifiers::get(cx);
        let count = self.keys.len();
        let keys: Vec<Stateful<Div>> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, key)| self.key(index, key, sticky, cx))
            .collect();
        let end = self.pill_end();
        let theme = cx.theme();
        let pill = key_pill_layer(theme);
        let disc = disc_layer(theme);
        let discs: Vec<Div> = self
            .discs
            .iter()
            .cloned()
            .enumerate()
            .map(|(at, key)| {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "a key row holds a couple of discs"
                )]
                let right = KEY_ROW_MARGIN_X
                    + (self.discs.len() - 1 - at) as f32 * (KEY_DISC + KEY_DISC_GAP);
                div()
                    .debug_selector(move || format!("key-row-disc-{at}"))
                    .absolute()
                    .top(rems_from_px(KEY_ROW_MARGIN_Y))
                    .right(rems_from_px(right))
                    .size(rems_from_px(KEY_DISC))
                    .flex()
                    .px(rems_from_px(PILL_PADDING - KEY_GAP / 2.0))
                    .layer(&disc)
                    .child(self.key(count + at, key, sticky, cx))
            })
            .collect();
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
                        .right(rems_from_px(KEY_ROW_MARGIN_X + end))
                        .layer(&pill),
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(rems_from_px(
                        KEY_ROW_MARGIN_X + PILL_PADDING - KEY_GAP / 2.0,
                    ))
                    .right(rems_from_px(
                        KEY_ROW_MARGIN_X + end + PILL_PADDING - KEY_GAP / 2.0,
                    ))
                    .flex()
                    .children(keys),
            )
            .children(discs)
    }
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
        lead: Entity<StackKey>,
    }

    impl Keys {
        fn new(cx: &mut App) -> Self {
            Self {
                arrows: cx.new(ArrowPad::new),
                control: cx.new(|cx| {
                    StackKey::new("ctrl", StackKeyTap::Latch(StickyModifier::Control), cx)
                        .icon(StickyModifier::Control.icon())
                        .groups(
                            vec![StackGroup::new([StackChip::text("ctrl-c", "Interrupt")])],
                            cx,
                        )
                }),
                lead: cx.new(|cx| StackKey::new("lead", StackKeyTap::Open, cx)),
            }
        }

        fn row(&self) -> KeyRow {
            KeyRow::new(vec![
                KeyRowKey::Stack(self.lead.clone()),
                KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
                KeyRowKey::Stack(self.control.clone()),
                KeyRowKey::text("/"),
                KeyRowKey::text("@"),
            ])
            .discs(vec![
                KeyRowKey::Arrows(self.arrows.clone()),
                KeyRowKey::Done,
            ])
        }
    }

    struct Host {
        keys: Keys,
        sent: Rc<RefCell<Vec<String>>>,
        width: ala::Pixels,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let sent = Rc::clone(&self.sent);
            let done = Rc::clone(&self.sent);
            div().size_full().child(
                div().absolute().bottom_0().left_0().w(self.width).child(
                    self.keys
                        .row()
                        .on_key(move |keystroke, _, _| {
                            sent.borrow_mut().push(keystroke.unparse());
                        })
                        .on_done(move |_, _| done.borrow_mut().push("done".into())),
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
        cx.update(crate::init);
        let (host, cx) = cx.add_window_view(|_, cx| Host {
            keys: Keys::new(cx),
            sent: Rc::default(),
            width: ala::px(393.0),
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
        tap(cx, "key-row-key-1");
        tap(cx, "stack-key-ctrl");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).control));
        tap(cx, "key-row-key-3");
        assert!(cx.update(|_, cx| StickyModifiers::get(cx).is_empty()));
        tap(cx, "key-row-key-4");
        tap(cx, "key-row-key-6");
        tap(cx, "stack-key-lead");
        let sent = host.read_with(cx, |host, _| host.sent.borrow().clone());
        assert_eq!(sent, ["escape", "ctrl-/", "@", "done"]);
    }

    #[ala::test]
    fn the_keys_share_the_pill_and_the_discs_close_the_row(cx: &mut TestAppContext) {
        let (_, cx) = host(cx);
        let px = ala::px;
        let row = cx.debug_bounds("key-row").expect("row");
        let pill = cx.debug_bounds("key-row-pill").expect("pill");
        let lead = cx.debug_bounds("stack-key-lead").expect("lead");
        assert_eq!(lead.center().y, pill.center().y);
        let esc = cx.debug_bounds("key-row-key-1").expect("esc");
        let at = cx.debug_bounds("key-row-key-4").expect("at");
        let arrows = cx.debug_bounds("key-row-disc-0").expect("arrows disc");
        let pad = cx.debug_bounds("arrow-pad").expect("arrow pad");
        let done = cx.debug_bounds("key-row-disc-1").expect("done disc");
        assert_eq!(row.size.height, px(72.0));
        assert_eq!(pill.left(), px(16.0));
        assert_eq!(pill.size.height, px(56.0));
        assert_eq!(pill.bottom(), row.bottom() - px(8.0));
        assert_eq!(pill.right(), px(393.0 - 16.0 - 2.0 * 56.0 - 2.0 * 8.0));
        assert_eq!(lead.left() - pill.left(), px(4.0));
        assert_eq!(pill.right() - at.right(), px(4.0));
        assert_eq!(esc.left() - lead.right(), px(2.0));
        assert_eq!(lead.size, esc.size);
        assert_eq!(esc.size.height, px(48.0));
        assert!((esc.size.width - px((249.0 - 16.0 - 8.0 - 8.0) / 5.0)).abs() <= px(0.1));
        assert_eq!(arrows.left() - pill.right(), px(8.0));
        assert_eq!(done.right(), px(393.0 - 16.0));
        assert_eq!(done.left() - arrows.right(), px(8.0));
        assert_eq!(arrows.size, ala::size(px(56.0), px(56.0)));
        assert_eq!(pad.origin - arrows.origin, ala::point(px(4.0), px(4.0)));
        assert_eq!(pad.size, ala::size(px(48.0), px(48.0)));
    }

    #[ala::test]
    fn keys_take_taps_off_the_cap(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let row = cx.debug_bounds("key-row").expect("row");
        let esc = cx.debug_bounds("key-row-key-1").expect("esc");
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
    }
}
