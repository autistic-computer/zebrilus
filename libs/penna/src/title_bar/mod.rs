use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, Decorations, Div, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, RenderOnce, Stateful, StatefulInteractiveElement as _,
    StyleRefinement, Styled, TitlebarOptions, Window, WindowControlArea, div, point,
    prelude::FluentBuilder as _, px,
};
use smallvec::SmallVec;

use crate::UiZoom;
use crate::{
    ActiveTheme as _, Edge, Icon, IconName, InteractiveElementExt as _, Layer, PaintState,
    RuleLook, StyledExt as _, Surface, SurfaceRole, Tone, foundation::restyle, h_flex,
};

pub const TITLE_BAR_HEIGHT: Pixels = crate::PANE_HEADER_HEIGHT;

pub const MACOS_TRAFFIC_LIGHT_GLYPH: f32 = 14.;
pub const MACOS_TRAFFIC_LIGHT_SPAN: f32 = 60.;

#[must_use]
pub fn macos_traffic_light_clearance(cx: &App) -> Pixels {
    let margin =
        (cx.theme().title_bar_height() - UiZoom::unzoomed(px(MACOS_TRAFFIC_LIGHT_GLYPH), cx)) / 2.;
    margin * 2. + UiZoom::unzoomed(px(MACOS_TRAFFIC_LIGHT_SPAN), cx)
}

#[must_use]
pub fn draws_window_controls(window: &Window) -> bool {
    (cfg!(target_os = "windows") && !window.is_fullscreen())
        || (cfg!(target_os = "linux")
            && matches!(window.window_decorations(), Decorations::Client { .. }))
}

#[must_use]
pub fn window_controls_width(window: &Window) -> Pixels {
    if draws_window_controls(window) {
        px(f32::from(CONTROL_ICON_WIDTH) * 3.0)
    } else {
        px(0.0)
    }
}

#[cfg(target_os = "macos")]
fn title_bar_left_padding(cx: &App) -> Pixels {
    macos_traffic_light_clearance(cx)
}
#[cfg(not(target_os = "macos"))]
fn title_bar_left_padding(_cx: &App) -> Pixels {
    px(12.)
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

#[cfg(not(target_os = "windows"))]
const CONTROL_ICON_WIDTH: Pixels = TITLE_BAR_HEIGHT;
#[cfg(target_os = "windows")]
const CONTROL_ICON_WIDTH: Pixels = px(46.);

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
            state => theme.layer(self.role, state),
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
            .w(CONTROL_ICON_WIDTH)
            .h_full()
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
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
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
            .child(Icon::new(control.icon()).medium())
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
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        if !draws_window_controls(window) {
            return div().id("window-controls");
        }

        let role = crate::roles::WINDOW_CONTROLS;
        let control = move |icon| WindowControl { icon, role };
        h_flex()
            .id("window-controls")
            .items_center()
            .flex_shrink_0()
            .h_full()
            .child(control(ControlIcon::minimize()))
            .child(control(if window.is_maximized() {
                ControlIcon::restore()
            } else {
                ControlIcon::maximize()
            }))
            .child(control(ControlIcon::close(self.on_close_window)))
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
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(!is_web, |this| {
                            this.window_control_area(WindowControlArea::Drag)
                                .when(is_fullscreen, ala::Styled::pl_3)
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
                .child(WindowControls {
                    on_close_window: self.on_close_window,
                }),
        )
    }
}
