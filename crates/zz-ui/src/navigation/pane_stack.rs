use std::{rc::Rc, sync::Arc, time::Duration};

use zz_gpui::{
    Animation, AnimationExt as _, AnyElement, App, Div, ElementId, Entity, EntityId,
    FULL_CORNER_RADIUS, Hsla, IntoElement, MouseButton, Pixels, RenderOnce, Role, SharedString,
    Window, deferred, div, ease_out_quint, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, Ground, Icon, IconName, Paint,
    PaintState, StyledExt as _, Tone, tooltip::Tooltip, touch::CoarsePointer,
};

pub type StackAction = Rc<dyn Fn(&mut Window, &mut App)>;

const RESTING: usize = 3;
const RING: f32 = 1.5;
const OPEN_DURATION: Duration = Duration::from_millis(160);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackSize {
    Row,
    Bar,
}

impl StackSize {
    pub fn tile(self) -> f32 {
        match self {
            Self::Row => 28.0,
            Self::Bar => 20.0,
        }
    }

    fn glyph(self) -> f32 {
        match self {
            Self::Row => 16.0,
            Self::Bar => 14.0,
        }
    }

    fn step(self) -> f32 {
        match self {
            Self::Row => 7.0,
            Self::Bar => 5.0,
        }
    }

    fn gap(self) -> f32 {
        match self {
            Self::Row => 4.0,
            Self::Bar => 3.0,
        }
    }

    pub fn widest(self) -> f32 {
        self.tile() + (RESTING - 1) as f32 * self.step()
    }
}

#[derive(Clone)]
pub struct StackTile {
    pub icon: IconName,
    pub favicon: Option<Arc<[u8]>>,
    pub label: SharedString,
    pub tooltip: SharedString,
    pub select: Option<StackAction>,
}

pub fn stack_radius(tile: Pixels, around: Pixels, cx: &App) -> Pixels {
    let theme = cx.theme();
    let radius = theme.control_radius();
    if radius == FULL_CORNER_RADIUS {
        return radius;
    }
    (radius - (around - tile) / 2.0).max(theme.radius * 0.25)
}

fn grow(radius: Pixels, by: f32) -> Pixels {
    if radius == FULL_CORNER_RADIUS {
        radius
    } else {
        radius + px(by)
    }
}

#[derive(Clone, Copy, Default)]
struct Pointer {
    stack: bool,
    row: bool,
    pinned: bool,
}

impl Pointer {
    fn open(self) -> bool {
        self.stack || self.row || self.pinned
    }
}

#[derive(IntoElement)]
pub struct PaneStack {
    id: ElementId,
    tiles: Vec<StackTile>,
    size: StackSize,
    ground: Ground,
    radius: Pixels,
    hover: Option<(SharedString, Hsla)>,
    dimmed: f32,
}

impl PaneStack {
    pub fn new(
        id: impl Into<ElementId>,
        tiles: Vec<StackTile>,
        size: StackSize,
        ground: Ground,
        radius: Pixels,
    ) -> Self {
        Self {
            id: id.into(),
            tiles,
            size,
            ground,
            radius,
            hover: None,
            dimmed: 0.0,
        }
    }

    #[must_use]
    pub fn group_hover_ground(mut self, group: impl Into<SharedString>, fill: Hsla) -> Self {
        self.hover = Some((group.into(), fill));
        self
    }

    #[must_use]
    pub fn dimmed(mut self, amount: f32) -> Self {
        self.dimmed = amount;
        self
    }
}

fn glyph(tile: &StackTile, size: f32, color: Hsla) -> AnyElement {
    let icon = Icon::new(tile.icon.clone())
        .size(px(size))
        .text_color(color);
    match &tile.favicon {
        Some(png) => {
            let fallback = {
                let icon = tile.icon.clone();
                move || {
                    Icon::new(icon.clone())
                        .size(px(size))
                        .text_color(color)
                        .into_any_element()
                }
            };
            zz_gpui::img(Arc::new(zz_gpui::Image::from_bytes(
                zz_gpui::ImageFormat::Png,
                png.to_vec(),
            )))
            .size(px(size))
            .flex_none()
            .with_fallback(fallback.clone())
            .with_loading(fallback)
            .into_any_element()
        }
        None => icon.into_any_element(),
    }
}

fn tile_box(size: StackSize, radius: Pixels, fill: Hsla, border: Hsla, width: Pixels) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size.tile()))
        .rounded(radius)
        .bg(fill)
        .border(width)
        .border_color(border)
}

fn update(pointer: &Entity<Pointer>, view: EntityId, cx: &mut App, f: impl FnOnce(&mut Pointer)) {
    pointer.update(cx, |pointer, _| f(pointer));
    cx.notify(view);
}

impl PaneStack {
    fn dim(&self, color: Hsla) -> Hsla {
        if self.dimmed > 0.0 {
            color.mix_oklab(self.ground.fill, self.dimmed)
        } else {
            color
        }
    }

    fn resting(&self, slot: usize, shown: usize, paint: &Paint) -> Div {
        let size = self.size;
        let tile = tile_box(
            size,
            self.radius,
            self.dim(paint.hover),
            self.dim(paint.hover.blend(paint.border)),
            paint.border_width,
        )
        .when(slot == 0, |tile| {
            tile.children(
                self.tiles
                    .first()
                    .map(|front| glyph(front, size.glyph(), self.dim(paint.text))),
            )
        });
        let left = px(slot as f32 * size.step());
        if slot + 1 == shown {
            return tile.absolute().left(left).top_0();
        }
        div()
            .absolute()
            .left(left - px(RING))
            .top(px(-RING))
            .p(px(RING))
            .rounded(grow(self.radius, RING))
            .bg(self.ground.fill)
            .when_some(self.hover.clone(), |ring, (group, fill)| {
                ring.group_hover(group, move |style| style.bg(fill))
            })
            .child(tile)
    }

    fn open_row(&self, pointer: &Entity<Pointer>, view: EntityId, cx: &App) -> AnyElement {
        let size = self.size;
        let theme = cx.theme();
        let ground = theme.surface(Elevation::Popover).opaque();
        let paint = theme
            .paint_on(ground, Tone::Neutral, Emphasis::Soft)
            .opaque();
        let pad = size.gap();
        let travel = size.tile() + size.gap();
        let animate = !cx.reduce_motion();
        let duration = theme.duration(OPEN_DURATION);
        let id = self.id.clone();
        let tiles = self.tiles.iter().enumerate().rev().map(|(index, tile)| {
            let front = index == 0;
            let state = if front {
                PaintState::Current
            } else {
                PaintState::Rest
            };
            let ink = if front {
                paint.text
            } else {
                paint.muted_at(PaintState::Rest)
            };
            let select = tile.select.clone();
            let tooltip = tile.tooltip.clone();
            let hovered = paint.hover;
            let element = tile_box(
                size,
                self.radius,
                paint.fill_at(state),
                paint.border,
                paint.border_width,
            )
            .id(("pane-stack-tile", index))
            .debug_selector(move || format!("pane-stack-tile-{index}"))
            .role(Role::Button)
            .aria_label(tile.label.clone())
            .aria_selected(front)
            .relative()
            .when(!front, |tile| tile.hover(move |style| style.bg(hovered)))
            .tooltip(move |window, cx| {
                Tooltip::new(tooltip.clone())
                    .max_w(px(360.0))
                    .build(window, cx)
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(select, |element, select| {
                let pointer = pointer.clone();
                element.cursor_pointer().on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    update(&pointer, view, cx, |pointer| *pointer = Pointer::default());
                    select(window, cx);
                })
            })
            .child(glyph(tile, size.glyph(), ink));
            let from = index.min(RESTING - 1) as f32 * size.step() - index as f32 * travel;
            if animate {
                element
                    .with_animation(
                        ("pane-stack-open", index),
                        Animation::new(duration).with_easing(ease_out_quint()),
                        move |tile, delta| tile.left(px(from * (1.0 - delta))),
                    )
                    .into_any_element()
            } else {
                element.into_any_element()
            }
        });
        let pointer = pointer.clone();
        let leave = pointer.clone();
        let row = div()
            .id(ElementId::Name(format!("pane-stack-row-{id:?}").into()))
            .absolute()
            .left(px(-pad))
            .top(px(-pad))
            .flex()
            .flex_row_reverse()
            .gap(px(size.gap()))
            .p(px(pad))
            .occlude()
            .popover_style(cx)
            .rounded(grow(self.radius, pad))
            .on_hover(move |hovered, _, cx| {
                let hovered = *hovered;
                update(&pointer, view, cx, |pointer| pointer.row = hovered);
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(move |_, _, cx| {
                update(&leave, view, cx, |pointer| pointer.pinned = false);
            })
            .children(tiles);
        deferred(if animate {
            row.with_animation(
                "pane-stack-fade",
                Animation::new(duration).with_easing(ease_out_quint()),
                zz_gpui::Styled::opacity,
            )
            .into_any_element()
        } else {
            row.into_any_element()
        })
        .with_priority(1)
        .into_any_element()
    }
}

impl RenderOnce for PaneStack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pointer = window.use_keyed_state(self.id.clone(), cx, |_, _| Pointer::default());
        let view = window.current_view();
        let openable = self.tiles.len() > 1;
        let open = openable && pointer.read(cx).open();
        let shown = self.tiles.len().clamp(1, RESTING);
        let paint = cx
            .theme()
            .paint_on(self.ground, Tone::Neutral, Emphasis::Soft)
            .opaque();
        let size = self.size;
        let width = size.tile() + (shown - 1) as f32 * size.step();
        let coarse = CoarsePointer::get(cx);
        let row = open.then(|| self.open_row(&pointer, view, cx));
        let hover = pointer.clone();
        let tap = pointer.clone();
        div()
            .id(self.id.clone())
            .relative()
            .flex_none()
            .w(px(width))
            .h(px(size.tile()))
            .when(openable, |stack| {
                stack
                    .cursor_pointer()
                    .on_hover(move |hovered, _, cx| {
                        let hovered = *hovered;
                        update(&hover, view, cx, |pointer| pointer.stack = hovered);
                    })
                    .when(coarse, |stack| {
                        stack
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                update(&tap, view, cx, |pointer| pointer.pinned = !pointer.pinned);
                            })
                    })
            })
            .children(
                (0..shown)
                    .rev()
                    .map(|slot| self.resting(slot, shown, &paint)),
            )
            .children(row)
    }
}
