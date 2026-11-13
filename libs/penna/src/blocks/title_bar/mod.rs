use std::rc::Rc;

use ala::{
    AnyElement, App, Bounds, ClickEvent, Context, Decorations, DefiniteLength, Div,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled,
    TitlebarOptions, Window, WindowControlArea, div,
    flat::{self, Align, BoxStyle, BoxStyleId, Extent, Flat, Flow, Ui},
    point,
    prelude::FluentBuilder as _,
    px,
};
use smallvec::SmallVec;

use crate::{
    ActiveTheme as _, Edge, Icon, IconName, InteractiveElementExt as _, Layer, PaintState,
    RuleLook, StyledExt as _, Surface, SurfaceRole, Tone, foundation::restyle, h_flex,
};
use crate::{Sizable as _, Space, TextStyle, UiZoom};

pub const TITLE_BAR_HEIGHT: Pixels = crate::PANE_HEADER_HEIGHT;

pub const MACOS_TRAFFIC_LIGHT_GLYPH: f32 = 14.;
pub const MACOS_TRAFFIC_LIGHT_SPAN: f32 = 60.;

#[must_use]
pub fn macos_traffic_light_clearance(cx: &App) -> Pixels {
    macos_traffic_light_clearance_in(cx.theme().title_bar_height(), cx)
}

#[must_use]
pub fn macos_traffic_light_clearance_in(height: Pixels, cx: &App) -> Pixels {
    let margin = (height - UiZoom::unzoomed(px(MACOS_TRAFFIC_LIGHT_GLYPH), cx)) / 2.;
    margin * 2. + UiZoom::unzoomed(px(MACOS_TRAFFIC_LIGHT_SPAN), cx)
}

#[must_use]
pub fn draws_window_controls(window: &Window) -> bool {
    (cfg!(target_os = "windows") && !window.is_fullscreen())
        || (cfg!(target_os = "linux")
            && matches!(window.window_decorations(), Decorations::Client { .. }))
}

const WINDOW_CONTROL_SIZE_PX: f32 = 24.0;
const WINDOW_CONTROL_GAP_PX: f32 = 8.0;
pub const WINDOW_CONTROL_SIZE: Pixels = px(WINDOW_CONTROL_SIZE_PX);
pub const WINDOW_CONTROL_GAP: Pixels = px(WINDOW_CONTROL_GAP_PX);
pub const WINDOW_CONTROLS_SPAN: Pixels =
    px(WINDOW_CONTROL_SIZE_PX * 3.0 + WINDOW_CONTROL_GAP_PX * 2.0);
pub const WINDOW_CONTROLS_INSET: Pixels = px(crate::CHROME_GAP);

#[must_use]
pub fn window_controls_width(window: &Window) -> Pixels {
    if draws_window_controls(window) {
        WINDOW_CONTROLS_SPAN
    } else {
        px(0.0)
    }
}

#[cfg(target_os = "macos")]
fn title_bar_left_padding(cx: &App) -> DefiniteLength {
    macos_traffic_light_clearance(cx).into()
}
#[cfg(not(target_os = "macos"))]
fn title_bar_left_padding(_cx: &App) -> DefiniteLength {
    Space::S12.into()
}

type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TitleBar {
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
    on_close_window: Option<CloseHandler>,
}

impl TitleBar {
    #[must_use]
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: SmallVec::new(),
            on_close_window: None,
        }
    }

    #[must_use]
    pub fn title_bar_options(height: Pixels) -> TitlebarOptions {
        TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(Self::traffic_light_position(height, 1.)),
            ..TitlebarOptions::default()
        }
    }

    #[must_use]
    pub fn traffic_light_position(height: Pixels, zoom: f32) -> Point<Pixels> {
        let margin = (height * zoom - px(MACOS_TRAFFIC_LIGHT_GLYPH)) / 2.;
        point(margin, margin)
    }

    #[must_use]
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(f));
        }
        self
    }
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for TitleBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

#[derive(IntoElement)]
struct WindowControl {
    icon: ControlIcon,
    role: SurfaceRole,
}

#[derive(Clone)]
enum ControlIcon {
    Minimize,
    Restore,
    Maximize,
    Close {
        on_close_window: Option<CloseHandler>,
    },
}

impl ControlIcon {
    fn minimize() -> Self {
        Self::Minimize
    }

    fn restore() -> Self {
        Self::Restore
    }

    fn maximize() -> Self {
        Self::Maximize
    }

    fn close(on_close_window: Option<CloseHandler>) -> Self {
        Self::Close { on_close_window }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
            Self::Maximize => "maximize",
            Self::Close { .. } => "close",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Restore => IconName::WindowRestore,
            Self::Maximize => IconName::WindowMaximize,
            Self::Close { .. } => IconName::WindowClose,
        }
    }

    fn window_control_area(&self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Restore | Self::Maximize => WindowControlArea::Max,
            Self::Close { .. } => WindowControlArea::Close,
        }
    }

    fn is_close(&self) -> bool {
        matches!(self, Self::Close { .. })
    }
}

impl WindowControl {
    fn layer(&self, state: PaintState, cx: &App) -> Layer {
        let theme = cx.theme();
        match state {
            PaintState::Hover | PaintState::Active if self.icon.is_close() => theme.layer(
                Surface::new(self.role).tone(Tone::Danger).prominent(true),
                if state == PaintState::Hover {
                    PaintState::Rest
                } else {
                    PaintState::Active
                },
            ),
            state => theme.layer(Surface::new(self.role), state),
        }
    }
}

impl RenderOnce for WindowControl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_linux = cfg!(target_os = "linux");
        let is_windows = cfg!(target_os = "windows");
        let hover = self.layer(PaintState::Hover, cx);
        let active = self.layer(PaintState::Active, cx);
        let rest = self.layer(PaintState::Rest, cx);
        let icon = self.icon.clone();
        let on_close_window = match &self.icon {
            ControlIcon::Close { on_close_window } => on_close_window.clone(),
            _ => None,
        };
        let control = self.icon;

        div()
            .id(control.id())
            .flex()
            .size(WINDOW_CONTROL_SIZE)
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .layer(&rest)
            .hover(move |style| restyle(style, &hover))
            .active(move |style| restyle(style, &active))
            .when(is_windows, |this| {
                this.window_control_area(control.window_control_area())
            })
            .when(is_linux, |this| {
                this.click_cursor()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        match icon {
                            ControlIcon::Minimize => window.minimize_window(),
                            ControlIcon::Restore | ControlIcon::Maximize => window.zoom_window(),
                            ControlIcon::Close { .. } => {
                                if let Some(f) = on_close_window.clone() {
                                    f(&ClickEvent::default(), window, cx);
                                } else {
                                    window.remove_window();
                                }
                            }
                        }
                    })
            })
            .child(Icon::new(control.icon()).large())
    }
}

#[derive(IntoElement, Default)]
pub struct WindowControls {
    on_close_window: Option<CloseHandler>,
}

impl WindowControls {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(f));
        }
        self
    }
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !draws_window_controls(window) {
            return div().id("window-controls").into_any_element();
        }

        let role = crate::foundation::roles::WINDOW_CONTROLS;
        if cfg!(target_os = "linux")
            && let Some(look) = ControlsLook::plain(role, window, cx)
        {
            return flat_controls(look, self.on_close_window, window, cx);
        }
        tree_controls(role, self.on_close_window, window)
    }
}

fn tree_controls(role: SurfaceRole, close: Option<CloseHandler>, window: &Window) -> AnyElement {
    let control = move |icon| WindowControl { icon, role };
    h_flex()
        .id("window-controls")
        .occlude()
        .items_center()
        .gap(WINDOW_CONTROL_GAP)
        .flex_shrink_0()
        .h_full()
        .child(control(ControlIcon::minimize()))
        .child(control(if window.is_maximized() {
            ControlIcon::restore()
        } else {
            ControlIcon::maximize()
        }))
        .child(control(ControlIcon::close(close)))
        .into_any_element()
}

fn flat_controls(
    look: ControlsLook,
    close: Option<CloseHandler>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let controls = window.use_keyed_state("window-controls", cx, |window, _| FlatControls {
        ui: Ui::new(window),
        look: None,
        styles: Vec::new(),
        row: None,
        close: None,
    });
    controls.update(cx, |controls, _| {
        controls.close = close;
        controls.restyle(look, window);
    });
    flat::surface("window-controls", &controls)
        .occlude()
        .flex_shrink_0()
        .w(WINDOW_CONTROLS_SPAN)
        .h_full()
        .into_any_element()
}

const CONTROL_STATES: usize = 3;

#[derive(Clone, PartialEq)]
struct ControlsLook {
    layers: [Layer; 5],
    icons: [SharedString; 4],
    icon_size: Pixels,
    scale_factor: f32,
}

impl ControlsLook {
    fn plain(role: SurfaceRole, window: &Window, cx: &App) -> Option<Self> {
        let layer = |icon: ControlIcon, state| WindowControl { icon, role }.layer(state, cx);
        let layers = [
            layer(ControlIcon::minimize(), PaintState::Rest),
            layer(ControlIcon::minimize(), PaintState::Hover),
            layer(ControlIcon::minimize(), PaintState::Active),
            layer(ControlIcon::close(None), PaintState::Hover),
            layer(ControlIcon::close(None), PaintState::Active),
        ];
        let plain = layers
            .iter()
            .all(|layer| layer.glass.is_none() && layer.shadow.is_empty() && layer.mark.is_none());
        let icon = |icon: ControlIcon| Icon::new(icon.icon()).drawn_path(cx);
        plain.then(|| Self {
            layers,
            icons: [
                icon(ControlIcon::minimize()),
                icon(ControlIcon::maximize()),
                icon(ControlIcon::restore()),
                icon(ControlIcon::close(None)),
            ],
            icon_size: crate::rems_from_px(crate::Size::Large.icon_px())
                .to_pixels(window.rem_size()),
            scale_factor: window.scale_factor(),
        })
    }

    fn layer(&self, close: bool, state: usize) -> &Layer {
        match (close, state) {
            (_, 0) => &self.layers[0],
            (false, state) => &self.layers[state],
            (true, state) => &self.layers[state + 2],
        }
    }
}

struct FlatControls {
    ui: Ui,
    look: Option<ControlsLook>,
    styles: Vec<BoxStyleId>,
    row: Option<BoxStyleId>,
    close: Option<CloseHandler>,
}

impl FlatControls {
    fn restyle(&mut self, look: ControlsLook, window: &Window) {
        if self.look.as_ref() == Some(&look) {
            return;
        }
        self.ui = Ui::new(window);
        self.row = Some(self.ui.box_style(BoxStyle {
            flow: Flow::Row,
            width: Extent::Grow,
            height: Extent::Grow,
            gap: WINDOW_CONTROL_GAP,
            align: Align::Center,
            ..BoxStyle::default()
        }));
        self.styles.clear();
        let rest = look.layer(false, 0).clone();
        for close in [false, true] {
            for state in 0..CONTROL_STATES {
                let layer = look.layer(close, state);
                self.styles.push(self.ui.box_style(BoxStyle {
                    flow: Flow::Row,
                    width: Extent::Fixed(WINDOW_CONTROL_SIZE),
                    height: Extent::Fixed(WINDOW_CONTROL_SIZE),
                    align: Align::Center,
                    justify: Align::Center,
                    fill: Some(layer.fill),
                    border: rest.border_width,
                    border_color: layer.border,
                    radius: rest.radius,
                    corner_share: rest.share,
                    corner_smoothing: Some(rest.smoothing),
                    ..BoxStyle::default()
                }));
            }
        }
        self.look = Some(look);
    }
}

impl Flat for FlatControls {
    fn ui(&mut self) -> &mut Ui {
        &mut self.ui
    }

    fn build(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut Context<Self>) {
        let (Some(look), Some(row)) = (&self.look, self.row) else {
            return;
        };
        let ui = &mut self.ui;
        let hovered = ui.hovered();
        let pressed = ui.pressed();
        ui.begin();
        ui.open(row);
        let middle = if window.is_maximized() {
            (2, "Restore")
        } else {
            (1, "Maximize")
        };
        for (key, (icon, name)) in [(0, "Minimize"), middle, (3, "Close")]
            .into_iter()
            .enumerate()
        {
            let key = key as u64;
            let close = icon == 3;
            let state = if pressed == Some(key) {
                2
            } else {
                usize::from(hovered == Some(key))
            };
            let style = self.styles[usize::from(close) * CONTROL_STATES + state];
            let control = ui.open(style);
            ui.target(control, key, crate::PointerCursor::style());
            ui.describe(control, key, Role::Button, &SharedString::new_static(name));
            ui.icon(
                &look.icons[icon],
                look.icon_size,
                look.layer(close, state).text,
            );
            ui.close();
        }
        ui.close();
        ui.layout(bounds.size);
    }

    fn clicked(&mut self, click: flat::Click, window: &mut Window, cx: &mut Context<Self>) {
        if click.button != MouseButton::Left {
            return;
        }
        match click.key {
            0 => window.minimize_window(),
            1 => window.zoom_window(),
            _ => match self.close.clone() {
                Some(close) => close(&ClickEvent::default(), window, cx),
                None => window.remove_window(),
            },
        }
    }
}

struct TitleBarState {
    should_move: bool,
}

fn arm_window_move(bar: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    if cfg!(target_os = "windows") {
        return bar;
    }
    let state = window.use_state(cx, |_, _| TitleBarState { should_move: false });
    bar.on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
        state.should_move = false;
    }))
    .on_mouse_down(
        MouseButton::Left,
        window.listener_for(&state, |state, _, _, _| {
            state.should_move = true;
        }),
    )
    .on_mouse_up(
        MouseButton::Left,
        window.listener_for(&state, |state, _, _, _| {
            state.should_move = false;
        }),
    )
    .on_mouse_move(window.listener_for(&state, |state, _, window, _| {
        if state.should_move {
            state.should_move = false;
            window.start_window_move();
        }
    }))
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let is_web = cfg!(target_family = "wasm");
        let is_linux = cfg!(target_os = "linux");
        let is_macos = cfg!(target_os = "macos");
        let is_fullscreen = window.is_fullscreen();

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(cx.theme().title_bar_height())
                .pl(title_bar_left_padding(cx))
                .rule(RuleLook::Plain, Edge::Bottom, cx)
                .bg(cx.theme().background)
                .refine_style(&self.style)
                .when(is_linux, |this| {
                    this.on_double_click(|_, window, _| window.zoom_window())
                })
                .when(is_macos, |this| {
                    this.on_double_click(|_, window, _| window.titlebar_double_click())
                })
                .map(|bar| arm_window_move(bar, window, cx))
                .child(
                    h_flex()
                        .id("bar")
                        .h_full()
                        .type_style(TextStyle::Body, cx)
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(!is_web, |this| {
                            this.window_control_area(WindowControlArea::Drag)
                                .when(is_fullscreen, |this| this.pl(Space::S12))
                                .when(is_linux && is_client_decorated, |this| {
                                    this.child(
                                        div()
                                            .top_0()
                                            .left_0()
                                            .absolute()
                                            .size_full()
                                            .h_full()
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                move |ev, window, _| {
                                                    window.show_window_menu(ev.position);
                                                },
                                            ),
                                    )
                                })
                        })
                        .children(self.children),
                )
                .child(
                    div()
                        .h_full()
                        .pr(WINDOW_CONTROLS_INSET)
                        .child(WindowControls {
                            on_close_window: self.on_close_window,
                        }),
                ),
        )
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::cell::Cell;

    use ala::{Modifiers, Render, TestAppContext, VisualTestContext};

    use super::*;

    struct Host {
        flat: bool,
        closed: Rc<Cell<usize>>,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let role = crate::foundation::roles::WINDOW_CONTROLS;
            let closed = self.closed.clone();
            let close: CloseHandler = Rc::new(move |_, _, _| closed.set(closed.get() + 1));
            let controls = if self.flat {
                let look = ControlsLook::plain(role, window, cx).expect("the look is plain");
                flat_controls(look, Some(close), window, cx)
            } else {
                tree_controls(role, Some(close), window)
            };
            div().flex().w_full().h(px(41.0)).child(controls)
        }
    }

    fn drawn(cx: &mut VisualTestContext) -> Vec<String> {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.draw(cx).clear(cx);
            let scene = window.rendered_scene();
            let quads = scene.quads.iter().map(|quad| {
                format!(
                    "quad {:?} {:?} {:?} {:?} {:?} {}",
                    quad.bounds,
                    quad.background,
                    quad.border_color,
                    quad.corner_radii,
                    quad.border_widths,
                    quad.corner_smoothing
                )
            });
            let sprites = scene.monochrome_sprites.iter().map(|sprite| {
                format!(
                    "sprite {:?} {:?} {:?}",
                    sprite.bounds, sprite.color, sprite.tile.bounds
                )
            });
            let mut drawn = quads.chain(sprites).collect::<Vec<_>>();
            drawn.sort();
            drawn
        })
    }

    fn states(flat: bool, cx: &mut TestAppContext) -> (Vec<Vec<String>>, usize) {
        let closed = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view(|_, _| Host {
            flat,
            closed: closed.clone(),
        });
        let close = point(WINDOW_CONTROLS_SPAN - WINDOW_CONTROL_SIZE / 2.0, px(20.0));
        let mut states = vec![drawn(cx)];
        cx.simulate_mouse_move(close, None, Modifiers::none());
        states.push(drawn(cx));
        cx.simulate_mouse_down(close, MouseButton::Left, Modifiers::none());
        states.push(drawn(cx));
        cx.simulate_mouse_up(close, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(point(px(11.0), px(20.0)), None, Modifiers::none());
        states.push(drawn(cx));
        (states, closed.get())
    }

    #[ala::test]
    fn flat_controls_draw_and_close_like_the_element_tree(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (tree, tree_closed) = states(false, cx);
        let (flat, flat_closed) = states(true, cx);
        assert_eq!(tree.iter().map(Vec::len).collect::<Vec<_>>(), [0, 1, 1, 1]);
        assert_ne!(tree[0], tree[1]);
        assert_ne!(tree[1], tree[2]);
        assert_eq!(flat, tree);
        assert_eq!((tree_closed, flat_closed), (1, 1));
    }
}
