use zz_gpui::{
    AnyElement, AnyView, App, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::*, px, relative,
};
use zz_protocol::{AgentProvider, DisplayPanesState, PaneId, PaneIndicator, TmuxColour, WindowId};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Elevation, IconName,
    pane::{
        FloatingSurface, PaneDragOverlayState, PaneLens, PaneOverlayCorner, PaneSplitAxis,
        PaneSplitHighlight, PaneSplitSide, pane_drag_chip, pane_drag_overlay, pane_drop_preview,
        pane_indicator, pane_indicator_overlay, pane_overlay_stack, pane_picker_choices,
        pane_picker_row, pane_picker_surface, pane_split_surface, pane_status_badge,
        pane_sync_badge, pane_unzoom_control, pane_waiting_state, terminal_link_popup,
        terminal_mode_indicator, terminal_search_prompt, terminal_status_popup,
    },
    pip::PipSurface,
    settings::panes_preview::PanesPreview,
};

use super::fixtures::{
    BUILD, LOGS, PANE_CHOICES, SERVER, TerminalPane, gap, placeholder_pane, radii, stateful,
    terminal_body,
};
use crate::stories::agent::lens;
use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "panes",
    name: "Panes",
    group: "Workspace",
    summary: "The frame around every pane, its header and corner tags, splits, the display-panes picker, drag and drop, floating popups and the new-pane picker.",
    sections: &[
        Section {
            id: "frames",
            name: "Frames",
            summary: "pane_surface with and without pane gaps. A gapped pane draws its frame in glass: the theme's glass edge, or a plain lit edge when the theme has none, tinted with the pane's color; the active pane is the lighter one, the others veiled darker with dimmer rims. Terminals dim their text when inactive; other panes take the inactive scrim. Try the pane-gaps, pane-glow and pane-opacity knobs: with pane-gaps on the frame owns the pane fill, as in the app.",
            build: |_, cx| stateless(frames, cx),
        },
        Section {
            id: "terminal-header",
            name: "Terminal header",
            summary: "terminal_pane_header with its split, Float, drag and close actions. Float sends break-pane -W at the tallest height the window fits, which turns the pane into a PiP. An inactive header hides its actions until the pointer is over it. A terminal running Claude Code or Codex adds the lens toggle before them, lit while the pane shows the agent timeline.",
            build: |_, cx| stateless(terminal_header, cx),
        },
        Section {
            id: "tags",
            name: "Status tags",
            summary: "The tags a pane stacks in its top-right and bottom-right corners: dead, waiting, sync, zoom, copy and view modes, search, status lines and link previews.",
            build: |_, cx| stateless(tags, cx),
        },
        Section {
            id: "display-panes",
            name: "Display panes",
            summary: "pane_indicator for tmux display-panes: the key on a popover chip, glass with the glass knob; the active pane ringed in the accent or display-panes-active-colour.",
            build: |_, cx| stateless(display_panes, cx),
        },
        Section {
            id: "splits",
            name: "Splits",
            summary: "pane_split_surface. Without gaps the slot is a 1px hairline, and the active pane's side of it turns accent; with gaps the slot is the margin.",
            build: |_, cx| stateless(splits, cx),
        },
        Section {
            id: "drag-and-drop",
            name: "Drag and drop",
            summary: "With the prefix armed every pane becomes a handle under the drop zone's veil. While a pane is in flight its source recedes, the other panes show as they are, the drop preview shows where it lands, and a chip follows the pointer.",
            build: |_, cx| stateless(drag_and_drop, cx),
        },
        Section {
            id: "floating",
            name: "Floating surface",
            summary: "FloatingSurface hosts display-popup and command output over the panes: bordered with a title in the frame, borderless, and recolored from a popup style. It takes the theme's glass like every popover; set backdrop=code to see it frost the panes behind.",
            build: |_, cx| stateless(floating, cx),
        },
        Section {
            id: "pip",
            name: "PiP",
            summary: "PipSurface draws every float that is not modal: the pane's content in a card that casts the float shadow, its title in the top border like a pane header, and tile back, zoom and close while pointed at (always on touch). Any pane can be one, and a terminal running Claude Code or Codex keeps its lens toggle there. Thrown by its title, it settles in the third of the window it was headed for.",
            build: |window, cx| stateful(window, cx, pip_views, pip),
        },
        Section {
            id: "picker",
            name: "New pane picker",
            summary: "A freshly split pane asks what it should become: pane_picker_surface with the three pane_picker_row choices, in their selected, resting and read-only states.",
            build: |_, cx| stateless(picker, cx),
        },
        Section {
            id: "preview",
            name: "Settings preview",
            summary: "PanesPreview, the three-pane sample on the Panes settings page, at the app defaults with gaps on and off. Click a pane to move focus.",
            build: |_, cx| stateless(preview, cx),
        },
    ],
};

fn sized(id: &'static str, height: f32, content: impl IntoElement) -> impl IntoElement {
    div()
        .id(id)
        .relative()
        .flex()
        .w_full()
        .h(px(height))
        .child(content)
}

fn frames(_: &mut Window, cx: &mut App) -> AnyElement {
    let mut grid = states().columns(3);
    for gaps in [true, false] {
        let suffix = if gaps { "gaps" } else { "no gaps" };
        grid = grid
            .state(
                format!("active terminal · {suffix}"),
                sized(
                    if gaps { "frame-a-gaps" } else { "frame-a" },
                    190.0,
                    TerminalPane::new(1, "cargo test", BUILD)
                        .active(true)
                        .gaps(gaps)
                        .render("pane", Vec::new(), cx),
                ),
            )
            .state(
                format!("inactive terminal · {suffix}"),
                sized(
                    if gaps { "frame-b-gaps" } else { "frame-b" },
                    190.0,
                    TerminalPane::new(2, "tail -f daemon.log", LOGS)
                        .gaps(gaps)
                        .render("pane", Vec::new(), cx),
                ),
            )
            .state(
                format!("inactive editor, scrim · {suffix}"),
                sized(
                    if gaps { "frame-c-gaps" } else { "frame-c" },
                    190.0,
                    placeholder_pane(
                        "pane",
                        IconName::File,
                        "flat.rs",
                        "Editor pane content",
                        false,
                        gaps,
                        Vec::new(),
                        cx,
                    ),
                ),
            );
    }
    grid.into_any_element()
}

fn header_box(
    id: &'static str,
    width: Option<f32>,
    header: impl IntoElement,
    cx: &App,
) -> AnyElement {
    div()
        .id(id)
        .when_some(width, |this, width| this.w(px(width)))
        .rounded(cx.theme().radius)
        .framed(cx)
        .overflow_hidden()
        .child(header)
        .into_any_element()
}

fn header(
    id: u64,
    title: &'static str,
    active: bool,
    draggable: bool,
    lens: Option<PaneLens>,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    zz_ui::pane::terminal_pane_header(
        active,
        title,
        zz_ui::pane::PaneDragSource::new(zz_protocol::PaneId(id), title, draggable, |_, _, _| {}),
        false,
        lens,
        |_, _, _| {},
        cx,
    )
}

fn terminal_header(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "active",
            header_box(
                "header-active",
                None,
                header(1, "cargo test -p zz-ui", true, true, None, cx),
                cx,
            ),
        )
        .state(
            "inactive (actions appear on hover)",
            header_box(
                "header-inactive",
                None,
                div().opacity(crate::pane().inactive).child(header(
                    2,
                    "tail -f daemon.log",
                    false,
                    true,
                    None,
                    cx,
                )),
                cx,
            ),
        )
        .state(
            "drag disabled: the only pane in its window, or a read-only client",
            header_box(
                "header-solo",
                None,
                header(3, "htop", true, false, None, cx),
                cx,
            ),
        )
        .state(
            "running Claude Code: the lens toggle shows the agent timeline",
            header_box(
                "header-claude",
                None,
                header(
                    5,
                    "Pinned tabs keep their icon width",
                    true,
                    true,
                    Some(PaneLens {
                        provider: AgentProvider::ClaudeCode,
                        open: false,
                    }),
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "running Codex with the lens open: the toggle lit, Show terminal on hover",
            header_box(
                "header-codex",
                None,
                header(
                    6,
                    "codex",
                    true,
                    true,
                    Some(PaneLens {
                        provider: AgentProvider::Codex,
                        open: true,
                    }),
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "long title at 320px",
            header_box(
                "header-long",
                Some(320.0),
                header(
                    4,
                    "ssh devbox -t 'cd ~/src/zz && cargo build --release --features agent'",
                    true,
                    true,
                    None,
                    cx,
                ),
                cx,
            ),
        )
        .into_any_element()
}

fn tags(_: &mut Window, cx: &mut App) -> AnyElement {
    let top = vec![
        terminal_mode_indicator(Some("Copy mode"), "12 / 340", cx).into_any_element(),
        pane_sync_badge(cx).into_any_element(),
        pane_unzoom_control().into_any_element(),
    ];
    let bottom = vec![
        terminal_link_popup("https://zed.dev/docs/key-bindings", cx).into_any_element(),
        terminal_status_popup("Copied 3 lines", cx).into_any_element(),
    ];
    let composed = TerminalPane::new(1, "cargo test", BUILD)
        .active(true)
        .gaps(true)
        .render(
            "pane",
            vec![
                pane_overlay_stack(PaneOverlayCorner::TopRight, top)
                    .top(cx.theme().pane_header_height() + px(8.0))
                    .into_any_element(),
                pane_overlay_stack(PaneOverlayCorner::BottomRight, bottom).into_any_element(),
            ],
            cx,
        );
    states()
        .state(
            "pane status",
            row()
                .child(pane_status_badge(IconName::CircleX, "Dead · exit 1", cx))
                .child(pane_waiting_state("Waiting for %3", cx))
                .child(pane_sync_badge(cx))
                .child(pane_unzoom_control()),
        )
        .state(
            "terminal modes",
            row()
                .child(terminal_mode_indicator(Some("Copy mode"), "12 / 340", cx))
                .child(terminal_mode_indicator(Some("Copy mode"), "", cx))
                .child(terminal_mode_indicator(Some("View mode"), "3 / 120", cx))
                .child(terminal_mode_indicator(None::<&str>, "+42 output", cx)),
        )
        .state(
            "search, status and link previews",
            row()
                .child(terminal_search_prompt("/cargo test", 11, |_, _, _| {}, cx))
                .child(terminal_status_popup("No matches for \"panic\"", cx))
                .child(terminal_link_popup(
                    "https://github.com/demfabris/zz/pull/412",
                    cx,
                )),
        )
        .state(
            "stacked in a terminal: modes top-right below the header, previews bottom-right",
            sized("tags-composed", 230.0, composed),
        )
        .into_any_element()
}

fn display_panes_state(
    indicators: Vec<PaneIndicator>,
    active_colour: TmuxColour,
) -> DisplayPanesState {
    DisplayPanesState {
        window: WindowId(1),
        duration_ms: 1000,
        indicators,
        colour: Some(TmuxColour::Theme(7)),
        active_colour: Some(active_colour),
    }
}

fn display_pane(index: u32, active: bool, label: &str) -> PaneIndicator {
    PaneIndicator {
        pane: PaneId(u64::from(index) + 1),
        index,
        select_key: match index {
            0..=9 => b'0' + index as u8,
            10..=35 => b'a' + (index - 10) as u8,
            _ => 0,
        },
        flags: if active { PaneIndicator::ACTIVE } else { 0 },
        label: label.to_owned(),
    }
}

fn display_panes(_: &mut Window, cx: &mut App) -> AnyElement {
    let chips = |id: &'static str, state: &DisplayPanesState, cx: &App| {
        state.indicators.iter().fold(row(), |row, indicator| {
            row.child(pane_indicator((id, indicator.index), state, indicator, cx))
        })
    };
    let keys = display_panes_state(
        vec![
            display_pane(0, true, ""),
            display_pane(1, false, ""),
            display_pane(2, false, ""),
            display_pane(12, false, "#[align=right]80x24"),
            display_pane(40, false, ""),
        ],
        TmuxColour::Theme(6),
    );
    let recoloured = display_panes_state(
        vec![display_pane(0, true, ""), display_pane(1, false, "")],
        TmuxColour::Basic(2),
    );
    let window = display_panes_state(
        vec![
            display_pane(0, true, "#[align=right]64x20"),
            display_pane(1, false, "#[align=right]63x9"),
            display_pane(2, false, "#[align=right]63x10"),
        ],
        TmuxColour::Theme(6),
    );
    let pane = |index: usize, cx: &App| {
        let indicator = &window.indicators[index];
        let overlay =
            pane_indicator_overlay(pane_indicator(("indicator", index), &window, indicator, cx))
                .into_any_element();
        let (title, lines) = [
            ("cargo test", BUILD),
            ("daemon.log", LOGS),
            ("just site", SERVER),
        ][index];
        TerminalPane::new(index as u64 + 1, title, lines)
            .active(indicator.active())
            .render(("display-pane", index), vec![overlay], cx)
    };
    let layout = pane_split_surface(
        "display-split",
        PaneSplitAxis::Horizontal,
        0.5,
        false,
        false,
        px(0.0),
        None,
        Some(PaneSplitHighlight::new(
            0.0,
            1.0,
            PaneSplitSide::First,
            cx.theme().accent,
        )),
        pane(0, cx),
        pane_split_surface(
            "display-split-right",
            PaneSplitAxis::Vertical,
            0.5,
            false,
            false,
            px(0.0),
            None,
            None,
            pane(1, cx),
            pane(2, cx),
            div().absolute(),
            cx,
        ),
        div().absolute(),
        cx,
    );
    states()
        .state(
            "keys: the active pane, digits, a letter key with its index and size, past 35 click only",
            chips("chip-keys", &keys, cx),
        )
        .state(
            "display-panes-active-colour green",
            chips("chip-recoloured", &recoloured, cx),
        )
        .state("over a window", sized("display-layout", 300.0, layout))
        .into_any_element()
}

fn split_pane(
    id: &'static str,
    label: &'static str,
    active: bool,
    gaps: bool,
    cx: &App,
) -> AnyElement {
    placeholder_pane(
        id,
        if active {
            IconName::SquareTerminal
        } else {
            IconName::Globe
        },
        label,
        if active { "active" } else { "inactive" },
        active,
        gaps,
        Vec::new(),
        cx,
    )
    .into_any_element()
}

fn split(
    id: &'static str,
    axis: PaneSplitAxis,
    gaps: bool,
    resizing: bool,
    side: PaneSplitSide,
    cx: &App,
) -> impl IntoElement {
    let first_active = side == PaneSplitSide::First;
    pane_split_surface(
        id,
        axis,
        0.5,
        resizing,
        gaps,
        gap(gaps),
        None,
        Some(PaneSplitHighlight::new(0.0, 1.0, side, cx.theme().accent)),
        split_pane("first", "First", first_active, gaps, cx),
        split_pane("second", "Second", !first_active, gaps, cx),
        div().absolute(),
        cx,
    )
}

fn nested(id: &'static str, gaps: bool, cx: &App) -> impl IntoElement {
    let right = pane_split_surface(
        "nested-right",
        PaneSplitAxis::Vertical,
        0.5,
        false,
        gaps,
        gap(gaps),
        None,
        Some(PaneSplitHighlight::new(
            0.0,
            1.0,
            PaneSplitSide::Second,
            cx.theme().accent,
        )),
        split_pane("top", "Top", false, gaps, cx),
        split_pane("bottom", "Bottom", true, gaps, cx),
        div().absolute(),
        cx,
    );
    div()
        .size_full()
        .when(gaps, |this| this.p(gap(gaps)))
        .child(pane_split_surface(
            id,
            PaneSplitAxis::Horizontal,
            0.55,
            false,
            gaps,
            gap(gaps),
            None,
            Some(PaneSplitHighlight::new(
                0.5,
                0.5,
                PaneSplitSide::Second,
                cx.theme().accent,
            )),
            split_pane("left", "Left", false, gaps, cx),
            right,
            div().absolute(),
            cx,
        ))
}

fn splits(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "horizontal · no gaps · left active",
            sized(
                "split-h",
                170.0,
                split(
                    "s",
                    PaneSplitAxis::Horizontal,
                    false,
                    false,
                    PaneSplitSide::First,
                    cx,
                ),
            ),
        )
        .state(
            "vertical · no gaps · bottom active",
            sized(
                "split-v",
                170.0,
                split(
                    "s",
                    PaneSplitAxis::Vertical,
                    false,
                    false,
                    PaneSplitSide::Second,
                    cx,
                ),
            ),
        )
        .state(
            "resizing: the hairline washes and drops the highlight",
            sized(
                "split-resizing",
                170.0,
                split(
                    "s",
                    PaneSplitAxis::Horizontal,
                    false,
                    true,
                    PaneSplitSide::First,
                    cx,
                ),
            ),
        )
        .state(
            "horizontal · 6px gaps",
            sized(
                "split-gaps",
                170.0,
                split(
                    "s",
                    PaneSplitAxis::Horizontal,
                    true,
                    false,
                    PaneSplitSide::First,
                    cx,
                ),
            ),
        )
        .state(
            "nested · no gaps: the highlight covers only the active pane's span",
            sized("split-nested", 220.0, nested("s", false, cx)),
        )
        .state(
            "nested · gaps",
            sized("split-nested-gaps", 220.0, nested("s", true, cx)),
        )
        .into_any_element()
}

fn drag_and_drop(_: &mut Window, cx: &mut App) -> AnyElement {
    let pane = crate::pane();
    let armed = |index: u64, active: bool, cx: &App| {
        let (title, lines) = if active {
            ("cargo test", BUILD)
        } else {
            ("daemon.log", LOGS)
        };
        TerminalPane::new(index, title, lines)
            .active(active)
            .gaps(true)
            .render(
                ("armed", index),
                vec![
                    pane_drag_overlay(
                        ("armed-overlay", index),
                        PaneDragOverlayState::Armed,
                        radii(true, cx),
                        cx,
                    )
                    .into_any_element(),
                ],
                cx,
            )
    };
    let armed_layout = div()
        .size_full()
        .p(px(pane.margin))
        .child(pane_split_surface(
            "armed-split",
            PaneSplitAxis::Horizontal,
            0.5,
            false,
            true,
            px(pane.margin),
            None,
            None,
            armed(1, true, cx),
            armed(2, false, cx),
            div().absolute(),
            cx,
        ));
    let source = TerminalPane::new(1, "cargo test", BUILD)
        .active(true)
        .gaps(true)
        .render(
            "source",
            vec![
                pane_drag_overlay(
                    "source-overlay",
                    PaneDragOverlayState::Source,
                    radii(true, cx),
                    cx,
                )
                .into_any_element(),
            ],
            cx,
        );
    let target = TerminalPane::new(2, "daemon.log", LOGS).gaps(true).render(
        "target",
        vec![
            pane_drag_overlay(
                "target-overlay",
                PaneDragOverlayState::Target,
                radii(true, cx),
                cx,
            )
            .into_any_element(),
        ],
        cx,
    );
    let dragging = div()
        .relative()
        .size_full()
        .p(px(pane.margin))
        .child(pane_split_surface(
            "dragging-split",
            PaneSplitAxis::Horizontal,
            0.5,
            false,
            true,
            px(pane.margin),
            None,
            None,
            source,
            target,
            div().absolute(),
            cx,
        ))
        .child(
            pane_drop_preview(
                cx.theme().pane_radius(),
                cx.theme().pane_edge_width().max(px(1.0)),
                cx,
            )
            .top(px(pane.margin))
            .bottom(px(pane.margin))
            .right(px(pane.margin))
            .w(relative(0.24)),
        )
        .child(
            div()
                .absolute()
                .left(relative(0.62))
                .top(px(96.0))
                .child(pane_drag_chip("%1", "cargo test", cx)),
        );
    states()
        .state(
            "prefix armed: every pane is a handle",
            sized("dnd-armed", 200.0, armed_layout),
        )
        .state(
            "dragging %1 onto the right edge of %2",
            sized("dnd-dragging", 200.0, dragging),
        )
        .state(
            "drag chips",
            row()
                .child(pane_drag_chip("%1", "cargo test", cx))
                .child(pane_drag_chip(
                    "%12",
                    "ssh devbox -t 'cd ~/src/zz && cargo build --release --features agent'",
                    cx,
                )),
        )
        .state(
            "drop previews at the pane radius and at zero radius",
            row()
                .child(
                    div()
                        .relative()
                        .w(px(160.0))
                        .h(px(90.0))
                        .child(pane_drop_preview(cx.theme().pane_radius(), px(1.0), cx).inset_0()),
                )
                .child(
                    div()
                        .relative()
                        .w(px(160.0))
                        .h(px(90.0))
                        .child(pane_drop_preview(px(0.0), px(1.0), cx).inset_0()),
                ),
        )
        .into_any_element()
}

fn floating_box(id: &'static str, surface: impl IntoElement) -> impl IntoElement {
    div().id(id).w(px(380.0)).h(px(190.0)).child(surface)
}

fn floating(_: &mut Window, cx: &mut App) -> AnyElement {
    let body = |cx: &App| terminal_body(SERVER, false, cx).bg(zz_gpui::transparent_black());
    states()
        .columns(2)
        .state(
            "bordered, titled",
            floating_box(
                "floating-titled",
                FloatingSurface::new("popup", body(cx), cx)
                    .title("display-popup · just site")
                    .content_inset(px(8.0), px(14.0)),
            ),
        )
        .state(
            "borderless",
            floating_box(
                "floating-borderless",
                FloatingSurface::new("popup", body(cx), cx)
                    .bordered(false)
                    .content_inset(px(0.0), px(6.0)),
            ),
        )
        .state(
            "colored by the popup style",
            floating_box(
                "floating-styled",
                FloatingSurface::new("popup", body(cx), cx)
                    .title("lazygit")
                    .content_inset(px(8.0), px(14.0))
                    .colors(
                        cx.theme().surface(Elevation::Popover).fill.opaque(),
                        cx.theme().foreground,
                        cx.theme().accent,
                    ),
            ),
        )
        .state(
            "long title clipped by the frame",
            floating_box(
                "floating-long",
                FloatingSurface::new("popup", body(cx), cx)
                    .title("display-popup -E 'cargo watch -x \"test -p zz-daemon --all-features\"'")
                    .content_inset(px(8.0), px(14.0)),
            ),
        )
        .into_any_element()
}

struct PipViews {
    claude: AnyView,
    codex: AnyView,
}

fn pip_views(window: &mut Window, cx: &mut App) -> PipViews {
    PipViews {
        claude: lens::floating_claude(window, cx),
        codex: lens::floating_codex(window, cx),
    }
}

fn pip_box(id: &'static str, height: f32, surface: impl IntoElement) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .max_w(px(420.0))
        .h(px(height))
        .child(surface)
}

fn pip_surface(id: &'static str, cx: &App) -> PipSurface {
    PipSurface::new(
        id,
        terminal_body(BUILD, false, cx).bg(zz_gpui::transparent_black()),
    )
    .header(pip_header("cargo watch -x test", cx))
    .content_inset(px(8.0), px(16.0))
}

pub(crate) fn pip_header(title: &'static str, cx: &App) -> impl IntoElement {
    zz_ui::pane::terminal_pane_header(
        true,
        title,
        zz_ui::pane::PaneDragSource::new(zz_protocol::PaneId(4), title, true, |_, _, _| {}),
        true,
        None,
        |_, _, _| {},
        cx,
    )
}

fn pip(views: &PipViews, _: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .child(
            states()
                .columns(2)
                .state(
                    "a terminal PiP: the pane header in the top border, tile back and zoom for the splits",
                    pip_box("pip-rest", 190.0, pip_surface("pip-rest", cx)),
                )
                .state(
                    "borderless: the header shows over the top only while pointed at",
                    pip_box(
                        "pip-borderless",
                        190.0,
                        pip_surface("pip-borderless", cx)
                            .bordered(false)
                            .content_inset(px(0.0), px(0.0)),
                    ),
                ),
        )
        .child(
            states()
                .columns(2)
                .state(
                    "Claude Code as a PiP with the lens open: the timeline under the terminal header",
                    pip_box("pip-claude", 340.0, views.claude.clone()),
                )
                .state(
                    "Codex as a PiP showing its terminal: the toggle opens the lens",
                    pip_box("pip-codex", 340.0, views.codex.clone()),
                ),
        )
        .child(
            states()
                .columns(2)
                .state(
                    "slid past the right edge while a tiled pane has focus; a click brings it back",
                    div()
                        .id("pip-stashed")
                        .relative()
                        .w_full()
                        .max_w(px(420.0))
                        .h(px(190.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(360.0))
                                .w(px(360.0))
                                .h_full()
                                .child(
                                    pip_surface("pip-stashed-card", cx)
                                        .stashed(true, |_, _, _| {}),
                                ),
                        ),
                )
                .state(
                    "two PiPs thrown on one place: the one behind shows its header",
                    div()
                        .id("pip-pile")
                        .relative()
                        .w_full()
                        .max_w(px(420.0))
                        .h(px(224.0))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .w(px(360.0))
                                .h(px(190.0))
                                .child(PipSurface::new(
                                    "pip-pile-back",
                                    terminal_body(LOGS, false, cx)
                                        .bg(zz_gpui::transparent_black()),
                                )
                                .header(pip_header("tail -f daemon.log", cx))
                                .content_inset(px(8.0), px(16.0))),
                        )
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(34.0))
                                .w(px(360.0))
                                .h(px(190.0))
                                .child(pip_surface("pip-pile-front", cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn picker_pane(id: &'static str, selected: usize, enabled: bool, cx: &App) -> impl IntoElement {
    let rows = PANE_CHOICES
        .iter()
        .enumerate()
        .map(|(index, (title, icon, shortcut))| {
            pane_picker_row(
                ("choice", index),
                title,
                icon.clone(),
                shortcut,
                selected == index,
                enabled,
                cx,
            )
            .into_any_element()
        })
        .collect::<Vec<_>>();
    sized(
        id,
        220.0,
        zz_ui::pane::pane_surface(
            "pane",
            pane_picker_surface("picker", radii(true, cx), rows, cx),
            Vec::new(),
            super::fixtures::chrome(true, false, true, cx),
            cx,
        ),
    )
}

fn picker(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "terminal selected",
            picker_pane("picker-terminal", 0, true, cx),
        )
        .state(
            "browser selected",
            picker_pane("picker-browser", 1, true, cx),
        )
        .state(
            "read-only client: rows dim and stop taking clicks",
            picker_pane("picker-readonly", 0, false, cx),
        )
        .state(
            "rows at full width",
            div()
                .w_full()
                .child(pane_picker_choices(PANE_CHOICES.iter().enumerate().map(
                    |(index, (title, icon, shortcut))| {
                        pane_picker_row(
                            ("bare-choice", index),
                            title,
                            icon.clone(),
                            shortcut,
                            index == 2,
                            true,
                            cx,
                        )
                        .into_any_element()
                    },
                ))),
        )
        .into_any_element()
}

fn preview(_: &mut Window, _: &mut App) -> AnyElement {
    let pane = crate::pane();
    let preview = |gaps: bool| PanesPreview {
        gaps,
        margin: pane.margin,
        inactive_opacity: pane.inactive,
    };
    states()
        .state(
            "gaps on, at the pane knobs",
            div().id("preview-gaps").child(preview(true)),
        )
        .state(
            "gaps off, the app default",
            div().id("preview-flush").child(preview(false)),
        )
        .state(
            "wide margin and deep dimming",
            div().id("preview-wide").child(PanesPreview {
                gaps: true,
                margin: 16.0,
                inactive_opacity: 0.4,
            }),
        )
        .into_any_element()
}
