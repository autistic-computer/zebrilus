use std::{rc::Rc, sync::Arc, time::Duration};

use zz_gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, Div, Element, ElementId, Entity,
    EntityId, FULL_CORNER_RADIUS, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId,
    MouseButton, Pixels, RenderOnce, Role, SharedString, Window, deferred, div, ease_out_quint,
    prelude::*, px,
};

use zz_client::program::Program;

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, Layer, PaintState,
    StyledExt as _, Surface, Tone,
    program::{Face, Lit, program_colors, program_mark},
    roles,
    tooltip::Tooltip,
    touch::CoarsePointer,
};

pub type StackAction = Rc<dyn Fn(&mut Window, &mut App)>;

const RESTING: usize = 3;
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
        self.width(RESTING)
    }

    fn width(self, tiles: usize) -> f32 {
        self.tile() + (tiles.clamp(1, RESTING) - 1) as f32 * self.step()
    }
}

#[derive(Clone)]
pub struct StackTile {
    pub icon: IconName,
    pub favicon: Option<Arc<[u8]>>,
    pub program: Option<&'static Program>,
    pub label: SharedString,
    pub tooltip: SharedString,
    pub select: Option<StackAction>,
}

/// The corner radius of a `tile` tall stack tile inside something `around`
/// tall: the role's corners, kept concentric with what holds them.
pub fn stack_radius(tile: Pixels, around: Pixels, cx: &App) -> Pixels {
    let theme = cx.theme();
    let radius = theme.layer(roles::PANE_STACK, PaintState::Rest).radius;
    if radius == FULL_CORNER_RADIUS || radius == Pixels::ZERO {
        return radius;
    }
    (radius - (around - tile) / 2.0).max(theme.radius * 0.25)
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
    dimmed: f32,
    beside: Option<(AnyElement, Pixels)>,
    room: usize,
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
            dimmed: 0.0,
            beside: None,
            room: RESTING,
        }
    }

    /// Holds room beside a row stack for `tiles` tiles, or for its own when
    /// it shows more, so the labels of a list of stacks line up.
    #[must_use]
    pub fn room(mut self, tiles: usize) -> Self {
        self.room = tiles;
        self
    }

    /// Lays `content` out `gap` after the stack. Open, the stack's tiles run
    /// over the start of it, and its text is hidden under them and fades back
    /// in past them.
    #[must_use]
    pub fn beside(mut self, content: impl IntoElement, gap: Pixels) -> Self {
        self.beside = Some((content.into_any_element(), gap));
        self
    }

    #[must_use]
    pub fn dimmed(mut self, amount: f32) -> Self {
        self.dimmed = amount;
        self
    }
}

fn glyph(tile: &StackTile, size: f32, color: Hsla) -> AnyElement {
    if let Some(mark) = tile
        .program
        .and_then(|program| program_mark(program, px(size), color, None))
    {
        return mark;
    }
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

fn tile_box(size: StackSize) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size.tile()))
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

    fn lit(&self, dark: bool, dimmed: f32) -> Lit {
        Lit {
            size: px(self.size.tile()),
            dark,
            strength: if dimmed > 0.0 { dimmed } else { 1.0 },
        }
    }

    fn face(&self, program: &'static Program, smoothing: f32, dark: bool, dimmed: f32) -> Face {
        Face {
            program,
            size: px(self.size.tile()),
            radius: self.radius,
            smoothing,
            dark,
            ground: self.ground.fill,
            dimmed,
        }
    }

    fn resting(&self, slot: usize, layer: &Layer, dark: bool, colors: bool) -> Div {
        let size = self.size;
        let front = self.tiles.first().filter(|_| slot == 0);
        let tile = if let Some(program) = front.and_then(|front| front.program).filter(|_| colors) {
            self.face(program, layer.smoothing, dark, self.dimmed)
                .tile()
        } else {
            let layer = Layer {
                fill: self.dim(layer.fill),
                border: self.dim(layer.fill.blend(layer.border)),
                ..layer.clone()
            };
            tile_box(size)
                .layer(&layer)
                .rounded(self.radius)
                .when(layer.glass.is_none(), |tile| {
                    tile.bg(Lit::background(layer.fill))
                })
                .shadow(self.lit(dark, self.dimmed).shadows())
                .children(front.map(|front| glyph(front, size.glyph(), self.dim(layer.text))))
        };
        tile.absolute().left(px(slot as f32 * size.step())).top_0()
    }

    fn open_row(&self, pointer: &Entity<Pointer>, view: EntityId, cx: &App) -> AnyElement {
        let size = self.size;
        let colors = program_colors(cx);
        let theme = cx.theme();
        let tile = Surface::new(roles::PANE_STACK).on(self.ground);
        let pad = size.gap();
        let travel = size.tile() + size.gap();
        let animate = !cx.reduce_motion();
        let duration = OPEN_DURATION;
        let id = self.id.clone();
        let tiles = self
            .tiles
            .iter()
            .enumerate()
            .rev()
            .map(|(index, stack_tile)| {
                let front = index == 0;
                let state = if front {
                    PaintState::Current
                } else {
                    PaintState::Rest
                };
                let layer = theme.layer(tile, state);
                let ink = if front {
                    layer.text
                } else {
                    theme
                        .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
                        .muted_at(PaintState::Rest)
                };
                let select = stack_tile.select.clone();
                let tooltip = stack_tile.tooltip.clone();
                let program = stack_tile.program;
                let dark = theme.is_dark();
                let hover = (!front).then(|| theme.layer(tile, PaintState::Hover));
                let element = tile_box(size)
                    .id(("pane-stack-tile", index))
                    .debug_selector(move || format!("pane-stack-tile-{index}"))
                    .role(Role::Button)
                    .aria_label(stack_tile.label.clone())
                    .aria_selected(front)
                    .relative()
                    .layer(&layer)
                    .rounded(self.radius)
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
                    });
                let element = match program.filter(|_| colors) {
                    Some(program) => self
                        .face(program, layer.smoothing, dark, 0.0)
                        .paint(element),
                    None => element
                        .when(layer.glass.is_none(), |element| {
                            element.bg(Lit::background(layer.fill))
                        })
                        .shadow(self.lit(dark, 0.0).shadows())
                        .when_some(hover, |element, hover| {
                            element.hover(move |style| {
                                style
                                    .border_color(hover.border)
                                    .text_color(hover.text)
                                    .bg(Lit::background(hover.fill))
                            })
                        })
                        .child(glyph(stack_tile, size.glyph(), ink)),
                };
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

impl PaneStack {
    /// How far the open stack's tiles reach past its leading edge.
    fn reach(&self) -> Pixels {
        let size = self.size;
        let count = self.tiles.len() as f32;
        px(count * size.tile() + (count - 1.0) * size.gap() + size.gap())
    }
}

impl RenderOnce for PaneStack {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pointer = window.use_keyed_state(self.id.clone(), cx, |_, _| Pointer::default());
        let view = window.current_view();
        let openable = self.tiles.len() > 1;
        let open = openable && pointer.read(cx).open();
        let shown = self.tiles.len().clamp(1, RESTING);
        let resting = cx.theme().layer(
            Surface::new(roles::PANE_STACK).on(self.ground),
            PaintState::Rest,
        );
        let size = self.size;
        let width = size.width(shown);
        let coarse = CoarsePointer::get(cx);
        let row = open.then(|| self.open_row(&pointer, view, cx));
        let reach = self.reach();
        let beside = self.beside.take();
        let hover = pointer.clone();
        let tap = pointer.clone();
        let stack = div()
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
            .when(!open, |stack| {
                stack.children((0..shown).rev().map(|slot| {
                    self.resting(slot, &resting, cx.theme().is_dark(), program_colors(cx))
                }))
            })
            .children(row);
        let Some((content, gap)) = beside else {
            return stack.into_any_element();
        };
        let stack = if size == StackSize::Row {
            div()
                .flex_none()
                .w(px(size.width(self.room.max(shown))))
                .child(stack)
                .into_any_element()
        } else {
            stack.into_any_element()
        };
        let row = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(gap)
            .child(stack)
            .child(content);
        if open {
            UnderTiles {
                child: row.into_any_element(),
                reach,
            }
            .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

/// Paints `child` with the text in it hidden up to `reach` from its leading
/// edge, where an open stack's tiles end, fading back in over the next
/// [`crate::LABEL_FADE`].
struct UnderTiles {
    child: AnyElement,
    reach: Pixels,
}

impl IntoElement for UnderTiles {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for UnderTiles {
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_text_fade(Some(self.fade(bounds)), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_text_fade(Some(self.fade(bounds)), |window| {
            self.child.paint(window, cx);
        });
    }
}

impl UnderTiles {
    fn fade(&self, bounds: Bounds<Pixels>) -> (Pixels, Pixels) {
        let gone = bounds.left() + self.reach;
        (gone + crate::LABEL_FADE, gone)
    }
}
