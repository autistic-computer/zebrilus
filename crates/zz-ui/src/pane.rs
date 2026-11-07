use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, ElementExt as _, Emphasis, Fill, Icon,
    IconName, Layer, PaintState, Selectable as _, Sizable as _, StyledExt as _, Tone,
    button::{Button, ButtonLook, ButtonVariants as _},
    pane_look::PaneLook,
};
use ala::{
    AnyElement, App, BorderStyle, Bounds, Corners, CursorStyle, ElementId, GlassMaterial, Hsla,
    IntoElement, ParentElement as _, Pixels, SharedString, Stateful, Styled as _, StyledText,
    Window, canvas, div, prelude::*, px, quad, relative, size,
};
use penna::{
    Bar, BarLook, Chip, ChipLook, ContentLook, Label, Row, RowLook, RuleLook, Tab, TabLook,
    TextStyle, Tile, TileVariant,
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
        .look(ButtonLook::Bar)
        .size(crate::PANE_HEADER_CONTROL_HEIGHT)
        .disabled(!enabled)
        .text_color(cx.theme().foreground.faint())
}

pub const fn agent_provider_icon(provider: zz_protocol::AgentProvider) -> IconName {
    match provider {
        zz_protocol::AgentProvider::Codex => crate::OPENAI_ICON,
        zz_protocol::AgentProvider::ClaudeCode => crate::CLAUDE_ICON,
    }
}

pub const fn pane_kind_icon(kind: &zz_protocol::PaneKindSnapshot) -> IconName {
    match kind {
        zz_protocol::PaneKindSnapshot::Terminal => IconName::SquareTerminal,
        zz_protocol::PaneKindSnapshot::Browser(_) => IconName::Globe,
        zz_protocol::PaneKindSnapshot::Editor(_) => IconName::File,
        zz_protocol::PaneKindSnapshot::Picker => IconName::Plus,
        zz_protocol::PaneKindSnapshot::Agent(agent) => agent_provider_icon(agent.provider),
    }
}

const PANE_TITLE_GROUP: &str = "pane-title";
const PANE_TITLE_ICON_SIZE: f32 = 14.0;
const PANE_TITLE_MAX_WIDTH: f32 = 168.0;

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
        let icon = div()
            .relative()
            .flex_none()
            .size(icon_size)
            .child(
                div()
                    .when(movable && !coarse, |icon| {
                        icon.group_hover(PANE_TITLE_GROUP, ala::Styled::invisible)
                    })
                    .child(self.icon.size(icon_size)),
            )
            .when(movable && !coarse, |icon| {
                icon.child(
                    div()
                        .absolute()
                        .inset_0()
                        .invisible()
                        .group_hover(PANE_TITLE_GROUP, ala::Styled::visible)
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
                        .group_hover(PANE_TITLE_GROUP, ala::Styled::visible)
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
        let title = Tab::new(self.id, TabLook::Title)
            .on(cx.theme().ground_of(BarLook::Pane))
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
    Flip,
}

pub use zz_client::agent_flip::FlipTo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneFlip {
    pub provider: zz_protocol::AgentProvider,
    pub to: FlipTo,
    pub armed: bool,
    pub pending: bool,
}

impl PaneFlip {
    #[must_use]
    pub fn of(
        pane: &zz_protocol::PaneSnapshot,
        flip: &zz_client::agent_flip::Flip,
    ) -> Option<Self> {
        zz_client::agent_flip::flip_target(pane).map(|target| Self {
            provider: target.provider,
            to: target.to,
            armed: flip.armed(),
            pending: flip.pending(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipChoice {
    WhenIdle,
    Now,
    Dismiss,
    Cancel,
}

impl FlipChoice {
    #[must_use]
    pub const fn mode(self) -> Option<zz_client::agent_flip::FlipMode> {
        match self {
            Self::WhenIdle => Some(zz_client::agent_flip::FlipMode::WhenIdle),
            Self::Now => Some(zz_client::agent_flip::FlipMode::Now),
            Self::Cancel => Some(zz_client::agent_flip::FlipMode::Cancel),
            Self::Dismiss => None,
        }
    }
}

impl TerminalPaneAction {
    pub fn commands(
        self,
        pane: zz_protocol::PaneId,
        float_rows: Option<u16>,
    ) -> Vec<zz_protocol::CommandInvocation> {
        match self {
            Self::SplitBottom => vec![zz_client::picker_split_command(
                pane,
                zz_protocol::Axis::Vertical,
            )],
            Self::SplitRight => vec![zz_client::picker_split_command(
                pane,
                zz_protocol::Axis::Horizontal,
            )],
            Self::Float => zz_client::floating::float_pane_commands(pane, float_rows).to_vec(),
            Self::Tile => vec![zz_client::floating::tile_pane_command(pane)],
            Self::Zoom => vec![zz_client::floating::zoom_pane_command(pane)],
            Self::Close => vec![zz_protocol::CommandInvocation::new(
                "kill-pane",
                ["-t".to_owned(), pane.to_string()],
            )],
            Self::Flip => Vec::new(),
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
    flip: Option<PaneFlip>,
    on_action: impl Fn(TerminalPaneAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<ala::Div> {
    pane_header(
        IconName::SquareTerminal,
        active,
        title,
        drag,
        floats,
        flip,
        on_action,
        cx,
    )
}

#[must_use]
pub const fn flip_icon(to: FlipTo) -> IconName {
    match to {
        FlipTo::Chat => IconName::Chat,
        FlipTo::Cli => IconName::SquareTerminal,
    }
}

pub fn pane_flip_button(
    flip: PaneFlip,
    on_flip: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Button {
    pane_header_icon_button("pane-flip", flip_icon(flip.to), !flip.pending, cx)
        .look(ButtonLook::Toggle)
        .debug_selector(|| "pane-flip".into())
        .selected(flip.armed)
        .when(flip.armed, |button| {
            button.text_color(cx.theme().foreground)
        })
        .tooltip(zz_client::agent_flip::flip_tooltip(flip.to, flip.armed))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_flip(window, cx);
        })
}

fn flip_choice_button(
    id: impl Into<ElementId>,
    label: &'static str,
    choice: FlipChoice,
    on_choice: &std::rc::Rc<dyn Fn(FlipChoice, &mut Window, &mut App)>,
) -> Button {
    let on_choice = std::rc::Rc::clone(on_choice);
    Button::new(id)
        .label(label)
        .compact()
        .when(choice != FlipChoice::Now, |button| {
            button.emphasis(Emphasis::Ghost)
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_choice(choice, window, cx);
        })
}

pub fn pane_flip_busy(
    pane: zz_protocol::PaneId,
    reason: &str,
    on_choice: impl Fn(FlipChoice, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Chip {
    let on_choice: std::rc::Rc<dyn Fn(FlipChoice, &mut Window, &mut App)> =
        std::rc::Rc::new(on_choice);
    pane_chip(("pane-flip-busy", pane.0), PaintState::Rest, cx)
        .multiline(true)
        .debug_selector(|| "pane-flip-busy".into())
        .flex()
        .flex_col()
        .items_stretch()
        .gap(px(8.0))
        .w(px(300.0))
        .max_w_full()
        .p(px(10.0))
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Body, cx)
        .line_height(px(16.0))
        .on_mouse_down(ala::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(7.0))
                .child(
                    div().relative().top(px(1.5)).child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(13.0))
                            .text_color(cx.theme().warning),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.0))
                        .child(
                            Label::new(TextStyle::Body, zz_client::agent_flip::BUSY_TITLE)
                                .strong()
                                .line_height(px(16.0)),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().foreground.muted())
                                .child(zz_client::agent_flip::busy_detail(reason)),
                        ),
                )
                .child(
                    flip_choice_button(
                        ("pane-flip-dismiss", pane.0),
                        "",
                        FlipChoice::Dismiss,
                        &on_choice,
                    )
                    .icon(IconName::Xmark)
                    .tooltip("Keep this pane as it is"),
                ),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(6.0))
                .child(flip_choice_button(
                    ("pane-flip-when-idle", pane.0),
                    zz_client::agent_flip::WHEN_IDLE_LABEL,
                    FlipChoice::WhenIdle,
                    &on_choice,
                ))
                .child(flip_choice_button(
                    ("pane-flip-now", pane.0),
                    zz_client::agent_flip::NOW_LABEL,
                    FlipChoice::Now,
                    &on_choice,
                )),
        )
}

pub fn pane_flip_armed(
    pane: zz_protocol::PaneId,
    to: FlipTo,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Chip {
    let on_choice: std::rc::Rc<dyn Fn(FlipChoice, &mut Window, &mut App)> =
        std::rc::Rc::new(move |_, window, cx| on_cancel(window, cx));
    status_badge(
        ("pane-flip-armed", pane.0),
        IconName::Clock,
        zz_client::agent_flip::armed_label(to),
        cx,
    )
    .debug_selector(|| "pane-flip-armed".into())
    .pr(px(4.0))
    .on_mouse_down(ala::MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .child(flip_choice_button(
        ("pane-flip-cancel", pane.0),
        zz_client::agent_flip::CANCEL_LABEL,
        FlipChoice::Cancel,
        &on_choice,
    ))
}

pub fn pane_flip_overlay(
    pane: zz_protocol::PaneId,
    to: FlipTo,
    flip: &zz_client::agent_flip::Flip,
    on_choice: impl Fn(FlipChoice, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    if let Some(reason) = flip.busy() {
        return Some(pane_flip_busy(pane, reason, on_choice, cx).into_any_element());
    }
    flip.armed().then(|| {
        pane_flip_armed(
            pane,
            to,
            move |window, cx| on_choice(FlipChoice::Cancel, window, cx),
            cx,
        )
        .into_any_element()
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
    flip: Option<PaneFlip>,
    on_action: impl Fn(TerminalPaneAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<ala::Div> {
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
    let bar = Bar::new("terminal-pane-header-bar", BarLook::Pane)
        .flex()
        .size_full()
        .items_center()
        .gap(px(8.0))
        .px(px(crate::CHROME_GAP))
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Body, cx)
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
        .children(flip.map(|flip| {
            div()
                .flex()
                .flex_none()
                .when(!active && !flip.armed, |slot| {
                    slot.invisible()
                        .group_hover("terminal-pane-header", ala::Styled::visible)
                })
                .child(pane_flip_button(
                    flip,
                    move |window, cx| toggle(TerminalPaneAction::Flip, window, cx),
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
                        .group_hover("terminal-pane-header", ala::Styled::visible)
                })
                .children(
                    pane_header_actions(floats)
                        .iter()
                        .map(|(kind, icon, label)| action(*kind, icon.clone(), label)),
                ),
        );
    div()
        .id("terminal-pane-header")
        .group("terminal-pane-header")
        .flex()
        .w_full()
        .h(cx.theme().pane_header_height())
        .flex_none()
        .child(bar)
}

pub fn pane_header_frame(
    header: impl IntoElement,
    radii: Corners<Pixels>,
    opacity: f32,
) -> ala::Div {
    div()
        .w_full()
        .flex_none()
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .child(div().opacity(opacity).child(header))
}

const PANE_ARRIVAL: web_time::Duration = web_time::Duration::from_millis(140);

#[derive(Default)]
pub struct PaneArrivals {
    window: Option<zz_protocol::WindowId>,
    known: std::collections::BTreeSet<zz_protocol::PaneId>,
    arrived: std::collections::BTreeMap<zz_protocol::PaneId, web_time::Instant>,
}

impl PaneArrivals {
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

    #[must_use]
    pub fn shown(&self, pane: zz_protocol::PaneId, now: web_time::Instant) -> f32 {
        self.arrived.get(&pane).map_or(1.0, |since| {
            ala::ease_out_quint()(
                (ala::animation_time(now.saturating_duration_since(*since)).as_secs_f32()
                    / PANE_ARRIVAL.as_secs_f32())
                .min(1.0),
            )
        })
    }

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
) -> Stateful<ala::Div> {
    let theme = cx.theme();
    let glass =
        theme.style_of(ContentLook::Pane).fill == Fill::Glass && theme.glass_effect().is_some();
    let rest = pane_material(cx);
    let hairline = rest.border_width;
    let outlined = hairline > Pixels::ZERO;
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
        pane_frame_layer(frame, glass, rest)
    });
    div()
        .id(id)
        .relative()
        .flex()
        .size_full()
        .overflow_hidden()
        .when_some(frame, |surface, frame| paint_material(surface, &frame))
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
                .map(|frame| {
                    if outlined {
                        hairline
                    } else {
                        frame.glint_width
                    }
                })
                .filter(|width| *width > Pixels::ZERO && (glass || outlined)),
            |surface, width| {
                surface.child(pane_frame_hairline(
                    chrome.radii,
                    width,
                    PaneLook::global(cx).edge_color(theme, chrome.active),
                    outlined,
                ))
            },
        )
        .children(overlays)
}

#[allow(
    clippy::disallowed_methods,
    reason = "a pane frame is the pane's own material: its glass and edge follow the pane"
)]
fn pane_material(cx: &App) -> Layer {
    cx.theme().layer(ContentLook::Pane, PaintState::Rest)
}

#[allow(
    clippy::disallowed_methods,
    reason = "a tmux popup is the pane's own material: its colours follow its tmux style"
)]
fn floating_pane_material(cx: &App) -> Layer {
    cx.theme()
        .layer(ContentLook::FloatingPane, PaintState::Rest)
}

#[allow(
    clippy::disallowed_methods,
    reason = "a pane frame or a tmux popup is the pane's own material: its glass, edge and colours follow the pane and its tmux style"
)]
fn paint_material(surface: Stateful<ala::Div>, material: &Layer) -> Stateful<ala::Div> {
    surface.layer(material)
}

fn pane_frame_layer(frame: GlassMaterial, glass: bool, rest: Layer) -> Layer {
    if glass {
        return Layer {
            glass: Some(frame),
            border_width: Pixels::ZERO,
            ..rest
        };
    }
    Layer {
        fill: frame.tint,
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
fn pane_inactive_scrim(radii: Corners<Pixels>, opacity: f32, cx: &App) -> ala::Div {
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

    #[must_use]
    pub const fn colors(mut self, background: Hsla, foreground: Hsla, border_color: Hsla) -> Self {
        self.background = Some(background);
        self.foreground = Some(foreground);
        self.border_color = Some(border_color);
        self
    }

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
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let mut layer = floating_pane_material(cx);
        if let Some(fill) = self.background {
            layer.fill = fill;
            layer.glass = layer
                .glass
                .map(|glass| glass.tint(fill.alpha(glass.tint.a)).fallback(fill));
        }
        if let Some(text) = self.foreground {
            layer.text = text;
        }
        if self.bordered {
            layer.border_width = forced_edge_width(cx);
            layer.border = self
                .border_color
                .filter(|color| *color != theme.border())
                .unwrap_or_else(|| theme.divider_color());
        } else {
            layer.border_width = Pixels::ZERO;
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
            .map(|surface| paint_material(surface, &layer))
            .on_mouse_down(ala::MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                        .when_some(label_fill, ala::Styled::bg)
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
                        .type_style_mono(TextStyle::Caption, cx)
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
    Armed,
    Source,
    Target,
}

pub fn pane_drag_overlay(
    id: impl Into<ElementId>,
    state: PaneDragOverlayState,
    radii: Corners<Pixels>,
    cx: &App,
) -> Stateful<ala::Div> {
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
            PaneDragOverlayState::Armed => veil.child(
                Tile::new("pane-drag-veil")
                    .variant(TileVariant::Veil)
                    .absolute()
                    .inset_0()
                    .rounded_tl(radii.top_left)
                    .rounded_tr(radii.top_right)
                    .rounded_bl(radii.bottom_left)
                    .rounded_br(radii.bottom_right),
            ),
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

pub fn pane_drop_preview(
    id: impl Into<ElementId>,
    radius: Pixels,
    border_width: Pixels,
    _: &App,
) -> Tile {
    Tile::new(id)
        .variant(TileVariant::Drop)
        .state(PaintState::Selected)
        .edge(border_width)
        .absolute()
        .rounded(radius)
}

pub fn pane_drag_chip(
    id: impl Into<ElementId>,
    pane: impl Into<SharedString>,
    title: impl Into<SharedString>,
    cx: &App,
) -> Chip {
    let ground = cx.theme().ground_of(ChipLook::Drag);
    Chip::empty(id)
        .look(ChipLook::Drag)
        .min_w_auto()
        .h_auto()
        .gap(px(8.0))
        .max_w(px(280.0))
        .px(px(10.0))
        .py(px(6.0))
        .overflow_hidden()
        .child(
            Label::new(TextStyle::Small, pane)
                .mono()
                .strong()
                .flex_none(),
        )
        .child(
            Label::new(TextStyle::Small, title)
                .muted()
                .on(ground)
                .truncate()
                .min_w_0(),
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

#[must_use]
pub fn pane_split_slot(gap: Pixels) -> Pixels {
    px(f32::from(gap).max(PANE_SPLIT_DIVIDER_THICKNESS))
}

pub fn pane_split_hit_target(
    id: impl Into<ElementId>,
    axis: PaneSplitAxis,
    ratio: f32,
    gap: Pixels,
) -> Stateful<ala::Div> {
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
) -> Stateful<ala::Div> {
    let slot = f32::from(pane_split_slot(gap));
    let ruled = cx.theme().style_of(RuleLook::Plain).outline || hairline.is_some();
    let first = div()
        .flex()
        .flex_none()
        .min_w_0()
        .min_h_0()
        .when(!gaps, ala::Styled::overflow_hidden)
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
        .when(!gaps, ala::Styled::overflow_hidden)
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
                .when(!gaps, ala::Styled::overflow_hidden)
                .child(second_content),
        )
        .child(hit_target)
}

pub fn pane_waiting_state(label: impl IntoElement, cx: &App) -> Chip {
    status_badge("pane-waiting", IconName::Clock, label, cx)
}

pub fn pane_sync_badge(cx: &App) -> Chip {
    status_badge("pane-sync", IconName::Layers, "Sync", cx)
}

pub fn pane_status_badge(icon: IconName, label: impl IntoElement, cx: &App) -> Chip {
    status_badge("pane-status", icon, label, cx)
}

fn status_badge(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl IntoElement,
    cx: &App,
) -> Chip {
    pane_chip(id, PaintState::Rest, cx)
        .flex()
        .items_center()
        .gap(px(7.0))
        .min_h(px(28.0))
        .px(px(10.0))
        .py(px(5.0))
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Body, cx)
        .line_height(px(16.0))
        .child(
            div()
                .relative()
                .top(px(0.5))
                .child(Icon::new(icon).size(px(13.0))),
        )
        .child(label)
}

pub fn pane_indicator_overlay(indicator: impl IntoElement) -> ala::Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(indicator)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneOverlayCorner {
    TopRight,
    BottomRight,
}

pub fn pane_overlay_stack(
    corner: PaneOverlayCorner,
    tags: impl IntoIterator<Item = AnyElement>,
) -> ala::Div {
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

pub fn pane_indicator(
    id: impl Into<ElementId>,
    state: &zz_protocol::DisplayPanesState,
    indicator: &zz_protocol::PaneIndicator,
    cx: &App,
) -> Chip {
    let theme = cx.theme();
    let active = indicator.active();
    let chip_state = if active {
        PaintState::Current
    } else {
        PaintState::Rest
    };
    let ink = theme.paint_on(
        theme.ground_at(ChipLook::Marker, chip_state),
        Tone::Neutral,
        Emphasis::Ghost,
    );
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

    Chip::empty(id)
        .look(ChipLook::Marker)
        .state(chip_state)
        .interactive(true)
        .when_some(option.filter(|_| active), Chip::ring)
        .debug_selector(move || format!("pane-indicator-{}", pane.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .h(px(40.0))
        .min_w(px(40.0))
        .px_0()
        .when(!caption.is_empty(), |chip| chip.pl(px(14.0)).pr(px(16.0)))
        .cursor(CursorStyle::PointingHand)
        .font_family(theme.font_family.clone())
        .child(
            Label::new(TextStyle::Title, glyph)
                .line_height(crate::rems_from_px(24.0))
                .text_color(glyph_ink),
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
                    .type_style(TextStyle::Small, cx)
                    .text_color(muted)
                    .children(caption),
            )
        })
}

pub fn pane_unzoom_control() -> Button {
    Button::new("pane-unzoom")
        .icon(IconName::WindowRestore)
        .label("Unzoom")
        .look(ButtonLook::FloatingChip)
}

pub fn terminal_mode_indicator(
    id: impl Into<ElementId>,
    label: Option<impl Into<SharedString>>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> Chip {
    let label = label.map(Into::into);
    let icon = match label.as_deref() {
        Some("Copy mode" | "COPY MODE") => IconName::Copy,
        Some(_) => IconName::File,
        None => IconName::ArrowDown,
    };
    let detail = detail.into();
    status_badge(
        id,
        icon,
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .when_some(label, ala::ParentElement::child)
            .when(!detail.is_empty(), |row| {
                row.child(
                    Label::new(TextStyle::Small, detail)
                        .mono()
                        .text_color(cx.theme().foreground.muted()),
                )
            }),
        cx,
    )
}

pub fn terminal_search_prompt(
    message: impl Into<SharedString>,
    caret: usize,
    on_caret: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Chip {
    let text = StyledText::new(message);
    let layout = text.layout().clone();
    overlay_line("terminal-search-prompt", PaintState::Focused, cx)
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

pub fn terminal_status_popup(message: impl IntoElement, cx: &App) -> Chip {
    let quiet = overlay_paint(cx).muted_at(PaintState::Rest);
    overlay_line("terminal-status-popup", PaintState::Rest, cx)
        .text_color(quiet)
        .child(message)
}

pub fn terminal_link_popup(uri: impl IntoElement, cx: &App) -> Chip {
    overlay_line("terminal-link-popup", PaintState::Rest, cx)
        .overflow_hidden()
        .child(uri)
}

fn overlay_paint(cx: &App) -> crate::Paint {
    cx.theme().paint(Tone::Neutral, Emphasis::Soft).opaque()
}

fn pane_chip(id: impl Into<ElementId>, state: PaintState, cx: &App) -> Chip {
    Chip::empty(id)
        .look(ChipLook::Floating)
        .state(state)
        .min_w_auto()
        .h_auto()
        .px_0()
        .gap_0()
        .when(state == PaintState::Focused, |chip| {
            chip.focus_ring(&overlay_paint(cx))
        })
}

fn overlay_line(id: impl Into<ElementId>, state: PaintState, cx: &App) -> Chip {
    pane_chip(id, state, cx)
        .flex()
        .items_center()
        .max_w(px(560.0))
        .px(px(10.0))
        .py(px(4.0))
        .type_style(TextStyle::Caption, cx)
        .line_height(relative(1.))
}

pub fn pane_picker_row(
    id: impl Into<ElementId>,
    title: &'static str,
    icon: crate::IconName,
    shortcut: &'static str,
    selected: bool,
    enabled: bool,
    cx: &App,
) -> Row {
    picker_item(
        id,
        title.into(),
        icon,
        None,
        Some(shortcut),
        selected,
        enabled,
        cx,
    )
}

pub fn pane_picker_folder_row(
    id: impl Into<ElementId>,
    path: impl Into<SharedString>,
    note: Option<SharedString>,
    shortcut: Option<&'static str>,
    selected: bool,
    cx: &App,
) -> Row {
    picker_item(
        id,
        path.into(),
        crate::IconName::Folder,
        note,
        shortcut,
        selected,
        true,
        cx,
    )
}

pub fn pane_picker_label(text: impl Into<SharedString>, cx: &App) -> Label {
    Label::new(TextStyle::Caption, text)
        .strong()
        .flex_none()
        .px(px(12.0))
        .pt(px(8.0))
        .pb(px(2.0))
        .text_color(cx.theme().foreground.muted())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatPickerTarget {
    MachineLine,
    Machine(usize),
    FolderLine,
    Agent(usize, zz_client::new_chat::AgentChoice),
    Browser,
}

const fn machine_icon(remote: bool) -> crate::IconName {
    if remote {
        crate::IconName::Server
    } else {
        crate::IconName::Laptop
    }
}

pub enum ChatPickerRow {
    Label(Box<Label>),
    Item(ChatPickerTarget, Box<Row>),
}

pub fn chat_picker_rows(
    pane: u64,
    picker: &zz_client::new_chat::ChatPicker,
    recents: &[zz_client::new_chat::FolderChoice],
    machines: &[zz_client::new_chat::MachineChoice],
    enabled: bool,
    cx: &App,
) -> Vec<ChatPickerRow> {
    use zz_client::new_chat::{AGENT_CHOICES, ChatStage, MachineChoice};
    let machine_label = || ChatPickerRow::Label(Box::new(pane_picker_label("Machine", cx)));
    if picker.stage == ChatStage::Machines {
        let mut rows = vec![machine_label()];
        rows.extend(machines.iter().enumerate().map(|(index, machine)| {
            let note = if machine.current {
                Some("here".into())
            } else if machine.live {
                None
            } else {
                Some("offline".into())
            };
            ChatPickerRow::Item(
                ChatPickerTarget::Machine(index),
                Box::new(picker_item(
                    ElementId::NamedInteger(format!("chat-picker-machine-{index}").into(), pane),
                    machine.name.clone().into(),
                    machine_icon(machine.remote),
                    note,
                    None,
                    picker.selected == index,
                    enabled && machine.live,
                    cx,
                )),
            )
        }));
        return rows;
    }
    let mut rows = Vec::new();
    if let Some(machine) = MachineChoice::current(machines) {
        rows.push(machine_label());
        rows.push(ChatPickerRow::Item(
            ChatPickerTarget::MachineLine,
            Box::new(picker_item(
                ElementId::NamedInteger("chat-picker-machine".into(), pane),
                machine.name.clone().into(),
                machine_icon(machine.remote),
                None,
                (machines.len() > 1).then_some("m"),
                false,
                enabled,
                cx,
            )),
        ));
    }
    rows.extend([
        ChatPickerRow::Label(Box::new(pane_picker_label("Folder", cx))),
        ChatPickerRow::Item(
            ChatPickerTarget::FolderLine,
            Box::new(pane_picker_folder_row(
                ElementId::NamedInteger("chat-picker-folder".into(), pane),
                picker.folder(recents).to_owned(),
                None,
                Some("tab"),
                false,
                cx,
            )),
        ),
        ChatPickerRow::Label(Box::new(pane_picker_label("Agent", cx))),
    ]);
    rows.extend(AGENT_CHOICES.iter().enumerate().map(|(index, agent)| {
        ChatPickerRow::Item(
            ChatPickerTarget::Agent(index, *agent),
            Box::new(pane_picker_row(
                ElementId::NamedInteger(format!("chat-picker-{}", agent.program).into(), pane),
                agent.title(),
                agent_provider_icon(agent.provider()),
                agent.shortcut,
                picker.selected == index,
                enabled,
                cx,
            )),
        )
    }));
    if picker.browser {
        rows.extend([
            ChatPickerRow::Label(Box::new(pane_picker_label("Pane", cx))),
            ChatPickerRow::Item(
                ChatPickerTarget::Browser,
                Box::new(pane_picker_row(
                    ElementId::NamedInteger("chat-picker-browser".into(), pane),
                    "Browser",
                    crate::CHROME_ICON,
                    "b",
                    picker.selected == AGENT_CHOICES.len(),
                    enabled,
                    cx,
                )),
            ),
        ]);
    }
    rows
}

fn picker_item(
    id: impl Into<ElementId>,
    title: SharedString,
    icon: crate::IconName,
    note: Option<SharedString>,
    shortcut: Option<&'static str>,
    selected: bool,
    enabled: bool,
    cx: &App,
) -> Row {
    let state = crate::row_state(selected, enabled);
    let ground = cx.theme().ground_at(RowLook::MenuCard, state);
    let height = crate::picker::list_row_height(cx);
    Row::new(id, RowLook::MenuCard)
        .state(state)
        .row_height(height)
        .h_flex()
        .flex_none()
        .h(height)
        .w_full()
        .gap(px(10.0))
        .px(px(12.0))
        .when(enabled, ala::Styled::cursor_pointer)
        .child(
            crate::Icon::new(icon)
                .with_size(px(16.0))
                .text_color(crate::muted_on(ground, cx)),
        )
        .child(
            Label::new(TextStyle::Small, title)
                .truncate()
                .flex_1()
                .min_w_0(),
        )
        .children(note.map(|note| {
            Label::new(TextStyle::Caption, note)
                .muted()
                .on(ground)
                .flex_none()
        }))
        .children(shortcut.map(|shortcut| {
            crate::kbd::Kbd::new(
                ala::Keystroke::parse(shortcut).expect("static pane picker shortcut"),
            )
            .lowercase()
            .on(ground)
        }))
}

pub fn pane_picker_surface(
    id: impl Into<ElementId>,
    radii: Corners<Pixels>,
    rows: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Stateful<ala::Div> {
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

pub fn pane_picker_choices(rows: impl IntoIterator<Item = AnyElement>) -> ala::Div {
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
    use ala::point;

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

    fn title_width(label: &'static str, cx: &mut ala::TestAppContext) -> Pixels {
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

    #[ala::test]
    fn the_title_fits_its_label_up_to_a_cap(cx: &mut ala::TestAppContext) {
        assert!(title_width("zsh", cx) < px(PANE_TITLE_MAX_WIDTH));
        assert_eq!(
            title_width("cargo test --workspace --all-features -- --nocapture", cx),
            px(PANE_TITLE_MAX_WIDTH)
        );
    }

    #[ala::test]
    fn the_title_drags_the_pane_and_its_x_closes_it(cx: &mut ala::TestAppContext) {
        use ala::{Modifiers, MouseButton};
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, _| TitleTest {
            label: "cargo test",
            started: Vec::new(),
            closed: 0,
        });
        let draw = |cx: &mut ala::VisualTestContext| {
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
        cell: ala::Size<Pixels>,
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
                ala::Size::default()
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

    #[ala::test]
    fn a_floating_surface_gives_its_content_every_cell_of_the_float(cx: &mut ala::TestAppContext) {
        cx.update(crate::init_without_theme);
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
