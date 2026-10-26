use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, ElementExt as _, Emphasis, Fill, Ground,
    Icon, IconName, InteractiveSurfaceExt as _, Layer, PaintState, Selectable as _, Sizable as _,
    StyledExt as _, Surface, Tone,
    button::{Button, ButtonVariants as _},
    roles,
};
use zz_gpui::{
    AnyElement, App, BorderStyle, Bounds, Corners, CursorStyle, ElementId, FontWeight,
    GlassMaterial, Hsla, IntoElement, ParentElement as _, Pixels, SharedString, Stateful,
    Styled as _, StyledText, Window, canvas, div, prelude::*, px, quad, relative, size,
};

mod drag;
mod motion;
pub use drag::{
    DropPreview, DropPreviewFrame, PaneDrag, PaneDragSource, PipGrab, pane_drag_preview,
};
pub use motion::{MovingPane, PaneMotion, moving_pane};

pub fn pane_header_icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    enabled: bool,
    cx: &App,
) -> Button {
    Button::compact_icon(id, icon)
        .surface_role(roles::PANE_HEADER_BUTTON)
        .size(crate::PANE_HEADER_CONTROL_HEIGHT)
        .disabled(!enabled)
        .text_color(cx.theme().foreground.faint())
}

pub const fn agent_provider_icon(provider: zz_protocol::AgentProvider) -> IconName {
    match provider {
        zz_protocol::AgentProvider::Codex => IconName::Openai,
        zz_protocol::AgentProvider::ClaudeCode => IconName::Claude,
    }
}

pub const fn pane_kind_icon(kind: &zz_protocol::PaneKindSnapshot) -> IconName {
    match kind {
        zz_protocol::PaneKindSnapshot::Terminal => IconName::SquareTerminal,
        zz_protocol::PaneKindSnapshot::Browser(_) => IconName::Globe,
        zz_protocol::PaneKindSnapshot::Editor(_) => IconName::File,
        zz_protocol::PaneKindSnapshot::Picker => IconName::Plus,
    }
}

const PANE_TITLE_GROUP: &str = "pane-title";
const PANE_TITLE_ICON_SIZE: f32 = 14.0;
const PANE_TITLE_MAX_WIDTH: f32 = 168.0;

/// A pane header's title as a button: dragged, it moves the pane; pointed at,
/// its icon turns into the move arrows and its X shows at the right end. A
/// coarse pointer, which never hovers, always shows the X. The title fits its
/// label up to a cap, and a label too long for it fades out before the X.
#[derive(IntoElement)]
pub struct PaneTitle {
    id: ElementId,
    icon: Icon,
    label: AnyElement,
    drag: PaneDragSource,
    on_close: Option<std::rc::Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl PaneTitle {
    pub fn new(
        id: impl Into<ElementId>,
        icon: impl Into<Icon>,
        label: impl IntoElement,
        drag: PaneDragSource,
    ) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            label: label.into_any_element(),
            drag,
            on_close: None,
        }
    }

    #[must_use]
    pub fn on_close(mut self, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(std::rc::Rc::new(on_close));
        self
    }
}

impl RenderOnce for PaneTitle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let coarse = crate::touch::CoarsePointer::get(cx);
        let movable = self.drag.enabled();
        let icon_size = px(PANE_TITLE_ICON_SIZE);
        let surface = Surface::new(roles::PANE_TITLE).on(cx.theme().ground_of(roles::PANE_HEADER));
        let icon = div()
            .relative()
            .flex_none()
            .size(icon_size)
            .child(
                div()
                    .when(movable && !coarse, |icon| {
                        icon.group_hover(PANE_TITLE_GROUP, zz_gpui::Styled::invisible)
                    })
                    .child(self.icon.size(icon_size)),
            )
            .when(movable && !coarse, |icon| {
                icon.child(
                    div()
                        .absolute()
                        .inset_0()
                        .invisible()
                        .group_hover(PANE_TITLE_GROUP, zz_gpui::Styled::visible)
                        .child(Icon::new(IconName::ArrowsMove).size(icon_size)),
                )
            });
        let close = self.on_close.map(|on_close| {
            div()
                .flex_none()
                .debug_selector(|| "pane-title-close".into())
                .when(!coarse, |close| {
                    close
                        .invisible()
                        .group_hover(PANE_TITLE_GROUP, zz_gpui::Styled::visible)
                })
                .child(
                    Button::compact_icon("pane-title-close", IconName::Xmark)
                        .size(crate::PANE_HEADER_CONTROL_HEIGHT)
                        .text()
                        .text_color(cx.theme().foreground.faint())
                        .tooltip("Close pane")
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_close(window, cx);
                        }),
                )
        });
        let end = if close.is_some() { px(0.0) } else { px(8.0) };
        let title = div()
            .id(self.id)
            .group(PANE_TITLE_GROUP)
            .debug_selector(|| "pane-title".into())
            .flex()
            .flex_initial()
            .max_w(px(PANE_TITLE_MAX_WIDTH))
            .h(crate::PANE_HEADER_CONTROL_HEIGHT)
            .items_center()
            .gap(px(6.0))
            .pl(px(6.0))
            .pr(end)
            .surface_hover(surface, PaintState::Rest, cx)
            .text_color(cx.theme().foreground.muted())
            .child(icon)
            .child(
                div()
                    .flex_initial()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_fade(crate::LABEL_FADE)
                    .child(self.label),
            )
            .children(close);
        self.drag.attach(title)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPaneAction {
    SplitBottom,
    SplitRight,
    Float,
    Tile,
    Zoom,
    Close,
    Lens,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneLens {
    pub provider: zz_protocol::AgentProvider,
    pub open: bool,
}

impl TerminalPaneAction {
    /// What the action sends for `pane`. Float makes a float `float_rows` tall,
    /// the most [`crate::pip::PipLayout::fill_rows`] says fits.
    pub fn commands(
        self,
        pane: zz_protocol::PaneId,
        float_rows: Option<u16>,
    ) -> Vec<zz_protocol::CommandInvocation> {
        let split = |axis: &str| {
            zz_protocol::CommandInvocation::new(
                "split-window",
                [
                    "--kind".to_owned(),
                    "picker".to_owned(),
                    axis.to_owned(),
                    "-t".to_owned(),
                    pane.to_string(),
                ],
            )
        };
        match self {
            Self::SplitBottom => vec![split("-v")],
            Self::SplitRight => vec![split("-h")],
            Self::Float => zz_client::floating::float_pane_commands(pane, float_rows).to_vec(),
            Self::Tile => vec![zz_client::floating::tile_pane_command(pane)],
            Self::Zoom => vec![zz_client::floating::zoom_pane_command(pane)],
            Self::Close => vec![zz_protocol::CommandInvocation::new(
                "kill-pane",
                ["-t".to_owned(), pane.to_string()],
            )],
            Self::Lens => Vec::new(),
        }
    }
}

pub const fn pane_header_actions(
    floats: bool,
) -> &'static [(TerminalPaneAction, IconName, &'static str)] {
    if floats {
        &[
            (
                TerminalPaneAction::Tile,
                IconName::LayoutColumns,
                "Tile back",
            ),
            (TerminalPaneAction::Zoom, IconName::WindowMaximize, "Zoom"),
        ]
    } else {
        &[
            (
                TerminalPaneAction::SplitBottom,
                IconName::PanelBottom,
                "Split bottom",
            ),
            (
                TerminalPaneAction::SplitRight,
                IconName::PanelRight,
                "Split right",
            ),
            (
                TerminalPaneAction::Float,
                IconName::PictureInPicture,
                "Float",
            ),
        ]
    }
}

pub fn terminal_pane_header(
    active: bool,
    title: impl Into<SharedString>,
    drag: PaneDragSource,
    floats: bool,
    lens: Option<PaneLens>,
    on_action: impl Fn(TerminalPaneAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    pane_header(
        IconName::SquareTerminal,
        active,
        title,
        drag,
        floats,
        lens,
        on_action,
        cx,
    )
}

pub fn pane_lens_button(
    lens: PaneLens,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Button {
    let label = if lens.open {
        "Show terminal"
    } else {
        "Show agent view"
    };
    pane_header_icon_button("pane-lens", IconName::Eye, true, cx)
        .surface_role(roles::PANE_LENS)
        .debug_selector(|| "pane-lens".into())
        .selected(lens.open)
        .when(lens.open, |button| button.text_color(cx.theme().foreground))
        .tooltip(label)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_toggle(window, cx);
        })
}

#[allow(
    clippy::too_many_arguments,
    reason = "a header is the sum of these independent pieces"
)]
pub fn pane_header(
    icon: IconName,
    active: bool,
    title: impl Into<SharedString>,
    drag: PaneDragSource,
    floats: bool,
    lens: Option<PaneLens>,
    on_action: impl Fn(TerminalPaneAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let on_action = std::rc::Rc::new(on_action);
    let close = std::rc::Rc::clone(&on_action);
    let toggle = std::rc::Rc::clone(&on_action);
    let action = |action, icon, label: &'static str| {
        let on_action = std::rc::Rc::clone(&on_action);
        pane_header_icon_button(label, icon, true, cx)
            .debug_selector(move || format!("pane-action-{label}"))
            .tooltip(label)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_action(action, window, cx);
            })
    };
    div()
        .id("terminal-pane-header")
        .group("terminal-pane-header")
        .flex()
        .w_full()
        .h(cx.theme().pane_header_height())
        .flex_none()
        .items_center()
        .gap(px(8.0))
        .px(px(crate::CHROME_GAP))
        .surface(Surface::new(roles::PANE_HEADER), cx)
        .font_family(cx.theme().font_family.clone())
        .text_size(crate::rems_from_px(13.0))
        .line_height(px(16.0))
        .text_color(cx.theme().foreground.muted())
        .child(
            div().flex().flex_1().min_w_0().items_center().child(
                PaneTitle::new(
                    "terminal-pane-title",
                    icon,
                    title.into(),
                    drag.floating(floats),
                )
                .on_close(move |window, cx| close(TerminalPaneAction::Close, window, cx)),
            ),
        )
        .children(lens.map(|lens| {
            div()
                .flex()
                .flex_none()
                .when(!active, |slot| {
                    slot.invisible()
                        .group_hover("terminal-pane-header", zz_gpui::Styled::visible)
                })
                .child(pane_lens_button(
                    lens,
                    move |window, cx| toggle(TerminalPaneAction::Lens, window, cx),
                    cx,
                ))
        }))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .when(!active, |actions| {
                    actions
                        .invisible()
                        .group_hover("terminal-pane-header", zz_gpui::Styled::visible)
                })
                .children(
                    pane_header_actions(floats)
                        .iter()
                        .map(|(kind, icon, label)| action(*kind, icon.clone(), label)),
                ),
        )
}

/// A terminal header over the pane's `background`, rounded into the pane's
/// top corners and faded to `opacity` while the pane is inactive.
pub fn pane_header_frame(
    header: impl IntoElement,
    radii: Corners<Pixels>,
    opacity: f32,
) -> zz_gpui::Div {
    div()
        .w_full()
        .flex_none()
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .child(div().opacity(opacity).child(header))
}

const PANE_ARRIVAL: web_time::Duration = web_time::Duration::from_millis(140);

/// The panes of the window on screen that appeared after it was first drawn,
/// so a split's new pane fades in instead of popping. Keyed by pane, not by
/// element, because a split moves its old pane under a new split node too.
#[derive(Default)]
pub struct PaneArrivals {
    window: Option<zz_protocol::WindowId>,
    known: std::collections::BTreeSet<zz_protocol::PaneId>,
    arrived: std::collections::BTreeMap<zz_protocol::PaneId, web_time::Instant>,
}

impl PaneArrivals {
    /// Notes the panes `window` holds at `now`: one it did not hold the last
    /// time fades in, while switching to another window fades nothing.
    pub fn observe(
        &mut self,
        window: zz_protocol::WindowId,
        panes: impl IntoIterator<Item = zz_protocol::PaneId>,
        now: web_time::Instant,
    ) {
        let panes = panes.into_iter().collect::<std::collections::BTreeSet<_>>();
        if self.window == Some(window) {
            for pane in panes.difference(&self.known) {
                self.arrived.insert(*pane, now);
            }
        }
        self.arrived.retain(|pane, since| {
            panes.contains(pane) && now.saturating_duration_since(*since) < PANE_ARRIVAL
        });
        self.window = Some(window);
        self.known = panes;
    }

    /// How far `pane` has faded in at `now`: 1 once it has settled.
    #[must_use]
    pub fn shown(&self, pane: zz_protocol::PaneId, now: web_time::Instant) -> f32 {
        self.arrived.get(&pane).map_or(1.0, |since| {
            zz_gpui::ease_out_quint()(
                (zz_gpui::animation_time(now.saturating_duration_since(*since)).as_secs_f32()
                    / PANE_ARRIVAL.as_secs_f32())
                .min(1.0),
            )
        })
    }

    /// Whether a pane is still fading in, so the window wants another frame.
    #[must_use]
    pub fn fading(&self) -> bool {
        !self.arrived.is_empty()
    }
}

const PANE_DRAG_SOURCE_FADE: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneChrome {
    pub radii: Corners<Pixels>,
    pub shadow: bool,
    pub active: bool,
    pub inactive_opacity: f32,
    pub frame: Option<GlassMaterial>,
    pub rim_over_content: bool,
}

impl PaneChrome {
    #[must_use]
    pub const fn new(radii: Corners<Pixels>, shadow: bool) -> Self {
        Self {
            radii,
            shadow,
            active: false,
            inactive_opacity: 1.0,
            frame: None,
            rim_over_content: false,
        }
    }

    #[must_use]
    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    #[must_use]
    pub fn dimmed(mut self, dimmed: bool, opacity: f32) -> Self {
        self.inactive_opacity = if dimmed { opacity.clamp(0.0, 1.0) } else { 1.0 };
        self
    }

    /// The glass the pane draws as its frame and fill, from
    /// [`Theme::pane_frame`](crate::Theme::pane_frame); `None` leaves the
    /// pane flush, its content painting its own fill.
    #[must_use]
    pub const fn frame(mut self, frame: Option<GlassMaterial>) -> Self {
        self.frame = frame;
        self
    }

    #[must_use]
    pub const fn rim_over_content(mut self, rim_over_content: bool) -> Self {
        self.rim_over_content = rim_over_content;
        self
    }
}

#[must_use]
pub fn pane_border_color(active: bool, cx: &App) -> Hsla {
    if active {
        cx.theme().accent
    } else {
        cx.theme().divider_color()
    }
}

pub fn pane_surface(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    overlays: impl IntoIterator<Item = AnyElement>,
    chrome: PaneChrome,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let theme = cx.theme();
    let glass = theme.style(roles::PANE).fill == Fill::Glass && theme.glass_effect().is_some();
    let rest = theme.layer(roles::PANE, PaintState::Rest);
    let outlined = rest.border_width > Pixels::ZERO;
    let shadow = if chrome.shadow {
        rest.shadow.clone()
    } else {
        Vec::new()
    };
    let rim = chrome
        .frame
        .filter(|_| glass && chrome.rim_over_content)
        .map(|frame| frame.rim());
    let frame = chrome.frame.map(|frame| {
        let frame = if rim.is_some() {
            frame.specular(0.0).fresnel(0.0)
        } else {
            frame
        };
        pane_frame_layer(frame, glass, rest, cx)
    });
    div()
        .id(id)
        .relative()
        .flex()
        .size_full()
        .overflow_hidden()
        .when_some(frame, |surface, frame| surface.layer(&frame))
        .shadow(shadow)
        .rounded_tl(chrome.radii.top_left)
        .rounded_tr(chrome.radii.top_right)
        .rounded_bl(chrome.radii.bottom_left)
        .rounded_br(chrome.radii.bottom_right)
        .child(content)
        .when_some(rim, |surface, rim| {
            surface.child(pane_frame_rim(chrome.radii, rim))
        })
        .when(chrome.inactive_opacity < 1.0, |surface| {
            surface.child(pane_inactive_scrim(
                chrome.radii,
                chrome.inactive_opacity,
                cx,
            ))
        })
        .when_some(
            chrome
                .frame
                .filter(|frame| frame.glint_width > Pixels::ZERO && (glass || outlined)),
            |surface, frame| {
                surface.child(pane_frame_hairline(
                    chrome.radii,
                    frame.glint_width,
                    theme.pane_edge_color(chrome.active),
                    !glass,
                ))
            },
        )
        .children(overlays)
}

fn pane_frame_layer(frame: GlassMaterial, glass: bool, rest: Layer, cx: &App) -> Layer {
    if glass {
        return Layer {
            glass: Some(frame),
            border_width: Pixels::ZERO,
            ..rest
        };
    }
    let solid = matches!(
        cx.theme().style(roles::PANE).fill,
        Fill::Solid | Fill::Glass
    );
    Layer {
        fill: if solid { frame.tint } else { rest.fill },
        border_width: Pixels::ZERO,
        ..rest
    }
}

fn pane_frame_rim(radii: Corners<Pixels>, rim: GlassMaterial) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| window.paint_glass(bounds, radii, &rim),
    )
    .absolute()
    .inset_0()
}

/// The frame's edge without glass, or where the window cannot sample its
/// backdrop and glass paints only its fill: a hairline the width of the
/// glint, over the content as a border would be.
fn pane_frame_hairline(
    radii: Corners<Pixels>,
    width: Pixels,
    color: Hsla,
    always: bool,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, cx| {
            if always || !window.supports_backdrop_sampling() {
                window.paint_quad(quad(
                    bounds,
                    radii,
                    cx.theme().transparent,
                    width,
                    color,
                    BorderStyle::default(),
                ));
            }
        },
    )
    .absolute()
    .inset_0()
}

#[allow(
    clippy::disallowed_methods,
    reason = "an inactive pane dims by the look's inactive-pane opacity"
)]
fn pane_inactive_scrim(radii: Corners<Pixels>, opacity: f32, cx: &App) -> zz_gpui::Div {
    div()
        .absolute()
        .inset_0()
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_bl(radii.bottom_left)
        .rounded_br(radii.bottom_right)
        .bg(cx.theme().background.opaque().opacity(1.0 - opacity))
}

#[derive(IntoElement)]
pub struct FloatingSurface {
    id: ElementId,
    title: SharedString,
    title_right: SharedString,
    content: AnyElement,
    inset_x: Pixels,
    inset_y: Pixels,
    background: Option<Hsla>,
    foreground: Option<Hsla>,
    border_color: Option<Hsla>,
    bordered: bool,
}

impl FloatingSurface {
    pub fn new(id: impl Into<ElementId>, content: impl IntoElement, _: &App) -> Self {
        Self {
            id: id.into(),
            title: SharedString::default(),
            title_right: SharedString::default(),
            content: content.into_any_element(),
            inset_x: Pixels::ZERO,
            inset_y: Pixels::ZERO,
            background: None,
            foreground: None,
            border_color: None,
            bordered: true,
        }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    #[must_use]
    pub fn title_right(mut self, title: impl Into<SharedString>) -> Self {
        self.title_right = title.into();
        self
    }

    #[must_use]
    pub const fn content_inset(mut self, x: Pixels, y: Pixels) -> Self {
        self.inset_x = x;
        self.inset_y = y;
        self
    }

    /// Fills, inks and edges the float in colors a popup style sets, over
    /// the floating pane role's own.
    #[must_use]
    pub const fn colors(mut self, background: Hsla, foreground: Hsla, border_color: Hsla) -> Self {
        self.background = Some(background);
        self.foreground = Some(foreground);
        self.border_color = Some(border_color);
        self
    }

    /// Edges the float in `color`, such as the accent of the active pane,
    /// keeping the role's fill and ink.
    #[must_use]
    pub const fn edge_color(mut self, color: Hsla) -> Self {
        self.border_color = Some(color);
        self
    }

    #[must_use]
    pub const fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }
}

impl RenderOnce for FloatingSurface {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let mut layer = theme.layer(roles::FLOATING_PANE, PaintState::Rest);
        if let Some(fill) = self.background {
            layer.fill = fill;
            layer.glass = layer
                .glass
                .map(|glass| glass.tint(fill.alpha(glass.tint.a)).fallback(fill));
        }
        if let Some(text) = self.foreground {
            layer.text = text;
        }
        if !self.bordered {
            layer.border_width = Pixels::ZERO;
        } else if let Some(color) = self.border_color.filter(|color| *color != theme.border()) {
            layer.border_width = forced_edge_width(cx);
            layer.border = color;
        }
        let border = layer.border_width;
        let inset_x = (self.inset_x - border).max(Pixels::ZERO);
        let inset_y = (self.inset_y - border).max(Pixels::ZERO);
        let title_inset = self.inset_x.max(px(8.0)) - border;
        let titled = self.bordered && !(self.title.is_empty() && self.title_right.is_empty());
        let label_fill = layer.glass.is_none().then_some(layer.fill);
        div()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .occlude()
            .layer(&layer)
            .on_mouse_down(zz_gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .absolute()
                    .left(inset_x)
                    .right(inset_x)
                    .top(inset_y)
                    .bottom(inset_y)
                    .overflow_hidden()
                    .child(self.content),
            )
            .when(titled, |surface| {
                let label = |text: SharedString| {
                    div()
                        .px(px(4.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .when_some(label_fill, zz_gpui::Styled::bg)
                        .child(text)
                };
                surface.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(title_inset)
                        .right(title_inset)
                        .flex()
                        .justify_between()
                        .gap(px(8.0))
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(crate::rems_from_px(11.0))
                        .line_height(inset_y.max(px(14.0)))
                        .child(label(self.title).min_w_0())
                        .when(!self.title_right.is_empty(), |row| {
                            row.child(label(self.title_right).flex_none())
                        }),
                )
            })
    }
}

fn forced_edge_width(cx: &App) -> Pixels {
    cx.theme().outline.max(px(1.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneDragOverlayState {
    /// The prefix is armed: every pane is a handle waiting to be picked up.
    Armed,
    /// This pane is the one in flight. It renders in place and recedes.
    Source,
    /// Another pane is in flight. This one only takes the pointer and shows
    /// as it is, so the drop preview is all that draws over it.
    Target,
}

/// The occluding pane layer used while the tmux prefix is armed or a pane is
/// being dragged. The armed veil is the drop zone's tint even where the
/// theme makes drop zones glass, so the panes under it stay sharp.
pub fn pane_drag_overlay(
    id: impl Into<ElementId>,
    state: PaneDragOverlayState,
    radii: Corners<Pixels>,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let cursor = match state {
        PaneDragOverlayState::Armed => CursorStyle::OpenHand,
        PaneDragOverlayState::Source | PaneDragOverlayState::Target => CursorStyle::ClosedHand,
    };
    div()
        .id(id)
        .absolute()
        .inset_0()
        .occlude()
        .cursor(cursor)
        .map(|veil| match state {
            PaneDragOverlayState::Armed => veil.layer(&Layer {
                border_width: Pixels::ZERO,
                ..cx.theme().layer(
                    Surface::new(roles::DROP_ZONE).on(Ground {
                        glass: true,
                        ..cx.theme().ground()
                    }),
                    PaintState::Hover,
                )
            }),
            PaneDragOverlayState::Source => veil.bg(pane_drag_source_veil(cx)),
            PaneDragOverlayState::Target => veil,
        })
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_bl(radii.bottom_left)
        .rounded_br(radii.bottom_right)
}

#[allow(
    clippy::disallowed_methods,
    reason = "the dragged pane fades under a veil of the window color"
)]
fn pane_drag_source_veil(cx: &App) -> Hsla {
    cx.theme()
        .background
        .opaque()
        .opacity(PANE_DRAG_SOURCE_FADE)
}

/// The rectangle a dragged pane lands in if it is dropped now. Positioning and
/// animation belong to the caller; pass the target pane's own radius and border
/// width so the preview's arcs land on the pane's.
pub fn pane_drop_preview(radius: Pixels, border_width: Pixels, cx: &App) -> zz_gpui::Div {
    let layer = cx.theme().layer(roles::DROP_ZONE, PaintState::Selected);
    let border_width = if layer.border_width > Pixels::ZERO {
        border_width
    } else {
        Pixels::ZERO
    };
    div().absolute().layer(&Layer {
        radius,
        border_width,
        ..layer
    })
}

/// The chip that follows the pointer while a pane is being dragged, carrying
/// the pane's name rather than a copy of its contents.
pub fn pane_drag_chip(
    pane: impl Into<SharedString>,
    title: impl Into<SharedString>,
    cx: &App,
) -> zz_gpui::Div {
    let theme = cx.theme();
    let layer = theme.layer(roles::DRAG_CHIP, PaintState::Rest);
    let ink = theme.paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost);
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .max_w(px(280.0))
        .px(px(10.0))
        .py(px(6.0))
        .overflow_hidden()
        .layer(&layer)
        .text_size(crate::rems_from_px(12.0))
        .child(
            div()
                .flex_none()
                .font_family(cx.theme().mono_font_family.clone())
                .font_weight(FontWeight::BOLD)
                .child(pane.into()),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(ink.muted_at(PaintState::Rest))
                .child(title.into()),
        )
}

const PANE_SPLIT_DIVIDER_THICKNESS: f32 = 1.0;
const PANE_SPLIT_HIT_THICKNESS: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSplitSide {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneSplitHighlight {
    pub start: f32,
    pub length: f32,
    pub side: PaneSplitSide,
    pub color: Hsla,
}

impl PaneSplitHighlight {
    #[must_use]
    pub const fn new(start: f32, length: f32, side: PaneSplitSide, color: Hsla) -> Self {
        Self {
            start,
            length,
            side,
            color,
        }
    }
}

/// The space a split keeps between its two children: the configured gap, or a
/// 1px hairline slot when there is none. Anything reconstructing pane boxes
/// from slot geometry must use this too.
#[must_use]
pub fn pane_split_slot(gap: Pixels) -> Pixels {
    px(f32::from(gap).max(PANE_SPLIT_DIVIDER_THICKNESS))
}

/// Draggable hit target for a pane split. The caller attaches the drag
/// callbacks to the returned element.
pub fn pane_split_hit_target(
    id: impl Into<ElementId>,
    axis: PaneSplitAxis,
    ratio: f32,
    gap: Pixels,
) -> Stateful<zz_gpui::Div> {
    let slot = f32::from(pane_split_slot(gap));
    let offset = -(PANE_SPLIT_HIT_THICKNESS - slot) / 2.0;
    div()
        .id(id)
        .absolute()
        .cursor(match axis {
            PaneSplitAxis::Horizontal => CursorStyle::ResizeLeftRight,
            PaneSplitAxis::Vertical => CursorStyle::ResizeUpDown,
        })
        .occlude()
        .when(axis == PaneSplitAxis::Horizontal, |element| {
            element
                .left(relative(ratio))
                .ml(px(offset))
                .w(px(PANE_SPLIT_HIT_THICKNESS))
                .h_full()
        })
        .when(axis == PaneSplitAxis::Vertical, |element| {
            element
                .top(relative(ratio))
                .mt(px(offset))
                .h(px(PANE_SPLIT_HIT_THICKNESS))
                .w_full()
        })
}

/// Split composition. The slot between the children is the single inter-pane
/// gap: `gap` wide when a pane margin is configured, a 1px hairline otherwise.
/// The hit target centers on the slot.
pub fn pane_split_surface(
    id: impl Into<ElementId>,
    axis: PaneSplitAxis,
    ratio: f32,
    resizing: bool,
    gaps: bool,
    gap: Pixels,
    hairline: Option<Hsla>,
    highlight: Option<PaneSplitHighlight>,
    first_content: impl IntoElement,
    second_content: impl IntoElement,
    hit_target: impl IntoElement,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let slot = f32::from(pane_split_slot(gap));
    let ruled = cx.theme().style(roles::SPLIT_RULE).outline || hairline.is_some();
    let first = div()
        .flex()
        .flex_none()
        .min_w_0()
        .min_h_0()
        .when(!gaps, zz_gpui::Styled::overflow_hidden)
        .when(axis == PaneSplitAxis::Horizontal, |element| {
            element.w(relative(ratio)).h_full()
        })
        .when(axis == PaneSplitAxis::Vertical, |element| {
            element.h(relative(ratio)).w_full()
        })
        .child(first_content);
    let divider = div()
        .relative()
        .flex_none()
        .when(axis == PaneSplitAxis::Horizontal, |element| {
            element.w(px(slot)).h_full()
        })
        .when(axis == PaneSplitAxis::Vertical, |element| {
            element.h(px(slot)).w_full()
        })
        .when(!gaps && (ruled || resizing), |divider| {
            let hairline = div()
                .absolute()
                .bg(if resizing {
                    cx.theme().foreground.faint()
                } else {
                    hairline.unwrap_or_else(|| cx.theme().divider_color())
                })
                .when(axis == PaneSplitAxis::Horizontal, |line| {
                    line.left(relative(0.5))
                        .ml(px(-PANE_SPLIT_DIVIDER_THICKNESS / 2.0))
                        .w(px(PANE_SPLIT_DIVIDER_THICKNESS))
                        .h_full()
                })
                .when(axis == PaneSplitAxis::Vertical, |line| {
                    line.top(relative(0.5))
                        .mt(px(-PANE_SPLIT_DIVIDER_THICKNESS / 2.0))
                        .h(px(PANE_SPLIT_DIVIDER_THICKNESS))
                        .w_full()
                });
            divider.child(hairline).when_some(
                highlight.filter(|_| ruled && !resizing),
                |divider, highlight| {
                    let half = PANE_SPLIT_DIVIDER_THICKNESS / 2.0;
                    divider.child(
                        div()
                            .absolute()
                            .bg(highlight.color)
                            .when(axis == PaneSplitAxis::Horizontal, |segment| {
                                segment
                                    .top(relative(highlight.start))
                                    .h(relative(highlight.length))
                                    .w(px(half))
                                    .when(highlight.side == PaneSplitSide::First, |segment| {
                                        segment.left(relative(0.5)).ml(px(-half))
                                    })
                                    .when(highlight.side == PaneSplitSide::Second, |segment| {
                                        segment.left(relative(0.5))
                                    })
                            })
                            .when(axis == PaneSplitAxis::Vertical, |segment| {
                                segment
                                    .left(relative(highlight.start))
                                    .w(relative(highlight.length))
                                    .h(px(half))
                                    .when(highlight.side == PaneSplitSide::First, |segment| {
                                        segment.top(relative(0.5)).mt(px(-half))
                                    })
                                    .when(highlight.side == PaneSplitSide::Second, |segment| {
                                        segment.top(relative(0.5))
                                    })
                            }),
                    )
                },
            )
        });

    div()
        .id(id)
        .relative()
        .flex()
        .size_full()
        .min_w_0()
        .min_h_0()
        .when(!gaps, zz_gpui::Styled::overflow_hidden)
        .when(axis == PaneSplitAxis::Vertical, |element| {
            element.flex_col()
        })
        .child(first)
        .child(divider)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .when(!gaps, zz_gpui::Styled::overflow_hidden)
                .child(second_content),
        )
        .child(hit_target)
}

/// A pending-entity placeholder shown as a top-right status tag while a pane
/// waits for its backing terminal, browser, or agent to attach.
pub fn pane_waiting_state(label: impl IntoElement, cx: &App) -> zz_gpui::Div {
    pane_status_badge(IconName::Clock, label, cx)
}

/// Warns that keyboard input is mirrored to every pane in a synchronized group.
pub fn pane_sync_badge(cx: &App) -> zz_gpui::Div {
    pane_status_badge(IconName::Layers, "Sync", cx)
}

pub fn pane_status_badge(icon: IconName, label: impl IntoElement, cx: &App) -> zz_gpui::Div {
    pane_chip(PaintState::Rest, cx)
        .flex()
        .items_center()
        .gap(px(7.0))
        .min_h(px(28.0))
        .px(px(10.0))
        .py(px(5.0))
        .font_family(cx.theme().font_family.clone())
        .text_size(crate::rems_from_px(13.0))
        .line_height(px(16.0))
        .child(
            div()
                .relative()
                .top(px(0.5))
                .child(Icon::new(icon).size(px(13.0))),
        )
        .child(label)
}

pub fn pane_indicator_overlay(indicator: impl IntoElement) -> zz_gpui::Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(indicator)
}

/// Which corner a [`pane_overlay_stack`] anchors to. Both right-align their
/// tags and differ only in whether the column grows down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneOverlayCorner {
    TopRight,
    BottomRight,
}

/// A right-aligned column of status tags pinned to one corner of a pane. The
/// caller passes the already-built tags in visual order.
pub fn pane_overlay_stack(
    corner: PaneOverlayCorner,
    tags: impl IntoIterator<Item = AnyElement>,
) -> zz_gpui::Div {
    let column = div()
        .absolute()
        .right(px(8.0))
        .flex()
        .flex_col()
        .items_end()
        .gap(px(6.0));
    match corner {
        PaneOverlayCorner::TopRight => column.top(px(8.0)),
        PaneOverlayCorner::BottomRight => column.bottom(px(8.0)),
    }
    .children(tags)
}

const DISPLAY_PANES_COLOUR: zz_protocol::TmuxColour = zz_protocol::TmuxColour::Theme(7);
const DISPLAY_PANES_ACTIVE_COLOUR: zz_protocol::TmuxColour = zz_protocol::TmuxColour::Theme(6);

/// The pane-selection chip for tmux `display-panes`: the key that picks the
/// pane, or its index in muted ink when it has none, beside the
/// `display-panes-format` caption, on the popover surface. The active pane is
/// ringed in the accent. `display-panes-colour` and
/// `display-panes-active-colour` take over the glyph and ring only when set
/// away from their defaults.
pub fn pane_indicator(
    id: impl Into<ElementId>,
    state: &zz_protocol::DisplayPanesState,
    indicator: &zz_protocol::PaneIndicator,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let theme = cx.theme();
    let active = indicator.active();
    let surface = Surface::new(roles::DISPLAY_PANES);
    let chip_state = if active {
        PaintState::Current
    } else {
        PaintState::Rest
    };
    let layer = theme.layer(surface, chip_state);
    let ink = theme.paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost);
    let muted = ink.muted_at(PaintState::Rest);
    let (option, default) = if active {
        (state.active_colour, DISPLAY_PANES_ACTIVE_COLOUR)
    } else {
        (state.colour, DISPLAY_PANES_COLOUR)
    };
    let option = option
        .filter(|colour| *colour != default)
        .and_then(|colour| crate::tmux_style::resolve_tmux_colour(colour, cx));
    let ring = active.then(|| option.unwrap_or(theme.accent));
    let key = indicator.selection_key();
    let glyph_ink = ring
        .or(option)
        .unwrap_or(if key.is_some() { ink.text } else { muted });
    let glyph = key.map_or_else(|| indicator.index.to_string(), String::from);
    let index = key
        .is_some_and(|key| key.is_ascii_alphabetic())
        .then(|| indicator.index.to_string().into_any_element());
    let caption: Vec<AnyElement> = index
        .into_iter()
        .chain(
            crate::tmux_style::split_tmux_alignment(&indicator.label)
                .into_iter()
                .filter(|segments| !segments.is_empty())
                .map(|segments| {
                    crate::tmux_style::tmux_styled_segments_text(
                        &segments,
                        muted,
                        ink.ground.fill,
                        cx,
                    )
                    .into_styled_text()
                    .into_any_element()
                }),
        )
        .collect();
    let pane = indicator.pane;
    let accent = theme.accent.opaque();
    let ringed = option.filter(|_| active).map(|color| {
        layer
            .shadow
            .iter()
            .cloned()
            .map(|mut shadow| {
                if shadow.color == accent {
                    shadow.color = color;
                }
                shadow
            })
            .collect::<Vec<_>>()
    });

    div()
        .id(id)
        .debug_selector(move || format!("pane-indicator-{}", pane.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .h(px(40.0))
        .min_w(px(40.0))
        .when(!caption.is_empty(), |chip| chip.pl(px(14.0)).pr(px(16.0)))
        .surface_hover(surface, chip_state, cx)
        .when_some(ringed, zz_gpui::Styled::shadow)
        .cursor(CursorStyle::PointingHand)
        .font_family(theme.font_family.clone())
        .child(
            div()
                .text_size(crate::rems_from_px(20.0))
                .line_height(crate::rems_from_px(24.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(glyph_ink)
                .child(glyph),
        )
        .when(!caption.is_empty(), |chip| {
            chip.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .max_w(px(240.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(crate::rems_from_px(12.0))
                    .line_height(px(16.0))
                    .text_color(muted)
                    .children(caption),
            )
        })
}

pub fn pane_unzoom_control() -> Button {
    Button::new("pane-unzoom")
        .icon(IconName::WindowRestore)
        .label("Unzoom")
        .surface_role(roles::PANE_CHIP)
}

pub fn terminal_mode_indicator(
    label: Option<impl Into<SharedString>>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> zz_gpui::Div {
    let label = label.map(Into::into);
    let icon = match label.as_deref() {
        Some("Copy mode" | "COPY MODE") => IconName::Copy,
        Some(_) => IconName::File,
        None => IconName::ArrowDown,
    };
    let detail = detail.into();
    pane_status_badge(
        icon,
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .when_some(label, zz_gpui::ParentElement::child)
            .when(!detail.is_empty(), |row| {
                row.child(
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(crate::rems_from_px(12.0))
                        .text_color(cx.theme().foreground.muted())
                        .child(detail),
                )
            }),
        cx,
    )
}

/// The find prompt, shown as a focused status tag in the pane's bottom-right
/// overlay stack. Borrows the focus ring so it reads as the active input.
pub fn terminal_search_prompt(
    message: impl Into<SharedString>,
    caret: usize,
    on_caret: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
    let text = StyledText::new(message);
    let layout = text.layout().clone();
    overlay_line(PaintState::Focused, cx)
        .child(text)
        .on_prepaint(move |_, window, cx| {
            if let Some(position) = layout.position_for_index(caret) {
                on_caret(
                    Bounds::new(
                        position,
                        size(px(1.0 / window.scale_factor()), layout.line_height()),
                    ),
                    window,
                    cx,
                );
            }
        })
}

/// A transient status line (copy confirmations, search errors) for the pane's
/// bottom-right overlay stack.
pub fn terminal_status_popup(message: impl IntoElement, cx: &App) -> zz_gpui::Div {
    let quiet = overlay_paint(cx).muted_at(PaintState::Rest);
    overlay_line(PaintState::Rest, cx)
        .text_color(quiet)
        .child(message)
}

/// A preview of the URI under the pointer, shown in the pane's bottom-right
/// overlay stack.
pub fn terminal_link_popup(uri: impl IntoElement, cx: &App) -> zz_gpui::Div {
    overlay_line(PaintState::Rest, cx)
        .overflow_hidden()
        .child(uri)
}

fn overlay_paint(cx: &App) -> crate::Paint {
    cx.theme().paint(Tone::Neutral, Emphasis::Soft).opaque()
}

fn pane_chip(state: PaintState, cx: &App) -> zz_gpui::Div {
    div()
        .surface_at(roles::PANE_CHIP, state, cx)
        .when(state == PaintState::Focused, |chip| {
            chip.focus_ring(&overlay_paint(cx))
        })
}

/// A line in the pane's bottom-right overlay stack.
fn overlay_line(state: PaintState, cx: &App) -> zz_gpui::Div {
    pane_chip(state, cx)
        .flex()
        .items_center()
        .max_w(px(560.0))
        .px(px(10.0))
        .py(px(4.0))
        .line_height(relative(1.))
        .text_size(crate::rems_from_px(11.0))
}

pub fn pane_picker_row(
    id: impl Into<ElementId>,
    title: &'static str,
    icon: crate::IconName,
    shortcut: &'static str,
    selected: bool,
    enabled: bool,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let state = crate::picker::row_state(selected, enabled);
    let ground = cx.theme().layer(roles::PANE_PICKER, state).ground;
    let ink = cx
        .theme()
        .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest);
    let height = crate::picker::LIST_ROW_HEIGHT;
    crate::picker::row_surface(
        crate::h_flex().id(id).flex_none().h(height),
        roles::PANE_PICKER,
        state,
        height,
        cx,
    )
    .w_full()
    .gap(px(10.0))
    .px(px(12.0))
    .when(enabled, zz_gpui::Styled::cursor_pointer)
    .child(crate::Icon::new(icon).with_size(px(16.0)).text_color(ink))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(crate::rems_from_px(12.0))
            .child(title),
    )
    .child(
        crate::kbd::Kbd::new(
            zz_gpui::Keystroke::parse(shortcut).expect("static pane picker shortcut"),
        )
        .lowercase()
        .on(ground),
    )
}

/// A freshly split pane asking what it should become: `rows` centered in
/// the pane, rounded into the pane's corners. The caller wires focus, keys
/// and clicks.
pub fn pane_picker_surface(
    id: impl Into<ElementId>,
    radii: Corners<Pixels>,
    rows: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .size_full()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_bl(radii.bottom_left)
        .rounded_br(radii.bottom_right)
        .text_color(theme.foreground)
        .px(px(12.0))
        .child(pane_picker_choices(rows))
}

pub fn pane_picker_choices(rows: impl IntoIterator<Item = AnyElement>) -> zz_gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(360.0))
        .gap(px(2.0))
        .children(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_gpui::point;

    struct TitleTest {
        label: &'static str,
        started: Vec<PaneDrag>,
        closed: usize,
    }

    impl Render for TitleTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let view = cx.entity();
            let close = cx.entity();
            div().w(px(400.0)).h(px(100.0)).flex().child(
                PaneTitle::new(
                    "title",
                    IconName::SquareTerminal,
                    self.label,
                    PaneDragSource::new(
                        zz_protocol::PaneId(3),
                        self.label,
                        true,
                        move |drag, _, cx| {
                            view.update(cx, |view, _| view.started.push(*drag));
                        },
                    ),
                )
                .on_close(move |_, cx| close.update(cx, |view, _| view.closed += 1)),
            )
        }
    }

    fn title_width(label: &'static str, cx: &mut zz_gpui::TestAppContext) -> Pixels {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| TitleTest {
            label,
            started: Vec::new(),
            closed: 0,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.debug_bounds("pane-title").unwrap().size.width
    }

    #[test]
    fn only_a_pane_new_to_the_window_on_screen_fades_in() {
        use zz_protocol::{PaneId, WindowId};
        let start = web_time::Instant::now();
        let mut arrivals = PaneArrivals::default();
        arrivals.observe(WindowId(1), [PaneId(1)], start);
        assert!(!arrivals.fading());
        arrivals.observe(WindowId(1), [PaneId(1), PaneId(2)], start);
        assert_eq!(arrivals.shown(PaneId(1), start), 1.0);
        assert_eq!(arrivals.shown(PaneId(2), start), 0.0);
        let settled = start + PANE_ARRIVAL;
        assert_eq!(arrivals.shown(PaneId(2), settled), 1.0);
        arrivals.observe(WindowId(1), [PaneId(1), PaneId(2)], settled);
        assert!(!arrivals.fading());
        arrivals.observe(WindowId(2), [PaneId(3), PaneId(4)], settled);
        assert!(!arrivals.fading());
    }

    #[zz_gpui::test]
    fn the_title_fits_its_label_up_to_a_cap(cx: &mut zz_gpui::TestAppContext) {
        assert!(title_width("zsh", cx) < px(PANE_TITLE_MAX_WIDTH));
        assert_eq!(
            title_width("cargo test --workspace --all-features -- --nocapture", cx),
            px(PANE_TITLE_MAX_WIDTH)
        );
    }

    #[zz_gpui::test]
    fn the_title_drags_the_pane_and_its_x_closes_it(cx: &mut zz_gpui::TestAppContext) {
        use zz_gpui::{Modifiers, MouseButton};
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, _| TitleTest {
            label: "cargo test",
            started: Vec::new(),
            closed: 0,
        });
        let draw = |cx: &mut zz_gpui::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        };
        draw(cx);
        let title = cx.debug_bounds("pane-title").unwrap();
        cx.simulate_mouse_move(title.center(), None, Modifiers::default());
        draw(cx);
        let close = cx.debug_bounds("pane-title-close").unwrap();
        assert!(close.right() <= title.right() && title.right() - close.right() < px(1.0));
        cx.simulate_click(close.center(), Modifiers::default());
        draw(cx);
        assert_eq!(view.read_with(cx, |view, _| view.closed), 1);
        assert!(view.read_with(cx, |view, _| view.started.is_empty()));

        let start = point(title.left() + px(10.0), title.center().y);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            point(px(300.0), px(80.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
        assert!(cx.update(|_, cx| cx.has_active_drag()));
        assert_eq!(
            view.read_with(cx, |view, _| view.started.clone()),
            [PaneDrag {
                pane: zz_protocol::PaneId(3),
                requires_prefix: false,
            }]
        );
        cx.simulate_mouse_up(
            point(px(300.0), px(80.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert_eq!(view.read_with(cx, |view, _| view.closed), 1);
    }

    #[test]
    fn inactive_opacity_one_disables_surface_dimming() {
        let chrome = PaneChrome::new(Corners::default(), false);

        assert_eq!(chrome.dimmed(true, 1.0).inactive_opacity, 1.0);
        assert_eq!(chrome.dimmed(false, 0.0).inactive_opacity, 1.0);
        assert_eq!(chrome.dimmed(true, 0.7).inactive_opacity, 0.7);
    }

    struct FloatHost {
        cell: zz_gpui::Size<Pixels>,
        columns: u16,
        rows: u16,
        bordered: bool,
    }

    impl Render for FloatHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let pad = f32::from(u8::from(self.bordered));
            let inset = if self.bordered {
                self.cell
            } else {
                zz_gpui::Size::default()
            };
            div().size_full().child(
                div()
                    .absolute()
                    .left(px(40.0))
                    .top(px(60.0))
                    .w(self.cell.width * (f32::from(self.columns) + 2.0 * pad))
                    .h(self.cell.height * (f32::from(self.rows) + 2.0 * pad))
                    .child(
                        FloatingSurface::new(
                            "float",
                            div().size_full().debug_selector(|| "float-content".into()),
                            cx,
                        )
                        .title("0 \"sh\"")
                        .title_right("[t][z][x]")
                        .content_inset(inset.width, inset.height)
                        .bordered(self.bordered),
                    ),
            )
        }
    }

    #[zz_gpui::test]
    fn a_floating_surface_gives_its_content_every_cell_of_the_float(
        cx: &mut zz_gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        let cell = size(px(9.0), px(19.0));
        for bordered in [true, false] {
            let (_, cx) = cx.add_window_view(|_, _| FloatHost {
                cell,
                columns: 30,
                rows: 4,
                bordered,
            });
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            let content = cx.debug_bounds("float-content").expect("float content");
            let pad = if bordered {
                cell
            } else {
                size(px(0.0), px(0.0))
            };
            assert_eq!(
                content,
                Bounds::new(
                    point(px(40.0) + pad.width, px(60.0) + pad.height),
                    size(cell.width * 30.0, cell.height * 4.0),
                ),
                "bordered: {bordered}"
            );
            let grid =
                crate::terminal::terminal_grid_size(content.size, cell.width, cell.height, 1.0);
            assert_eq!((grid.columns, grid.rows), (30, 4), "bordered: {bordered}");
        }
    }
}
