use std::rc::Rc;

use zz_client::StatusBarPane;
use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Corners, ElementId, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Stateful, Styled as _, Window, div, px,
};
use zz_protocol::{AgentProvider, BrowserDescriptor, EditorDescriptor, PaneId, PaneKindSnapshot};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    navigation::status::{StatusAction, StatusPaneEntry},
    pane::{PaneChrome, PaneDragSource, PaneLens, pane_surface, terminal_pane_header},
};

/// What a freshly split pane offers, as the desktop and thin clients list
/// it: title, icon and the key that picks it.
pub const PANE_CHOICES: [(&str, IconName, &str); 3] = [
    ("Terminal", IconName::SquareTerminal, "t"),
    ("Browser", IconName::BrandChrome, "b"),
    ("Editor", IconName::File, "e"),
];

#[derive(Clone, Copy)]
pub enum Tone {
    Plain,
    Muted,
    Success,
    Danger,
    Prompt,
}

pub const BUILD: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz main*"),
    (Tone::Plain, "$ cargo test -p zz-ui"),
    (Tone::Muted, "   Compiling zz-ui v0.16.0"),
    (Tone::Success, "    Finished test profile in 4.21s"),
    (Tone::Plain, "test result: ok. 214 passed; 0 failed"),
    (Tone::Prompt, "$"),
];

pub const LOGS: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz main*"),
    (Tone::Plain, "$ tail -f daemon.log"),
    (Tone::Muted, "12:04:51 attach client c3 to $0"),
    (Tone::Muted, "12:04:52 resize %2 to 118x36"),
    (Tone::Danger, "12:04:58 pty %5 exited with status 1"),
    (Tone::Muted, "12:05:03 select-window @2"),
];

pub const SERVER: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz/site main"),
    (Tone::Plain, "$ just site"),
    (Tone::Success, "  astro ready in 412 ms"),
    (Tone::Plain, "  Local  http://localhost:4321/"),
    (Tone::Prompt, "$"),
];

pub const CLAUDE: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz fix/pinned-tab-width*"),
    (
        Tone::Plain,
        "> Run the whole browser suite and fix whatever breaks.",
    ),
    (Tone::Muted, "• Bash(cargo test -p zz-ui browser)"),
    (Tone::Danger, "  └ test result: FAILED. 12 passed; 2 failed"),
    (Tone::Muted, "• Update(crates/zz-ui/src/browser/drag.rs)"),
    (Tone::Success, "  └ Updated with 6 additions and 2 removals"),
    (Tone::Muted, "• Bash(cargo test -p zz-ui browser::drag)"),
    (Tone::Muted, "  Running… (esc to interrupt)"),
    (Tone::Prompt, ">"),
];

pub const CODEX: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz main*"),
    (Tone::Plain, "› Fix the flaky tab drag test"),
    (Tone::Muted, "• Explored"),
    (Tone::Muted, "  └ Read drag.rs, tab_strip.rs"),
    (Tone::Muted, "• Ran cargo test -p zz-ui browser::drag"),
    (Tone::Success, "  └ test result: ok. 9 passed; 0 failed"),
    (
        Tone::Plain,
        "• The drop preview reads the widths the layout stored.",
    ),
    (Tone::Prompt, "›"),
];

pub fn noop() -> StatusAction {
    Rc::new(|_, _| {})
}

pub fn radii(gaps: bool, cx: &App) -> Corners<Pixels> {
    Corners::all(if gaps {
        cx.theme().pane_radius()
    } else {
        px(0.0)
    })
}

pub fn gap(gaps: bool) -> Pixels {
    px(if gaps { crate::pane().margin } else { 0.0 })
}

pub fn chrome(active: bool, scrim: bool, gaps: bool, cx: &App) -> PaneChrome {
    PaneChrome::new(radii(gaps, cx), gaps)
        .active(active)
        .dimmed(!active && scrim, crate::pane().inactive)
        .frame(gaps.then(|| cx.theme().pane_frame(cx.theme().background, active)))
}

pub fn terminal_body(lines: &[(Tone, &'static str)], dimmed: bool, cx: &App) -> zz_gpui::Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .px(px(12.0))
        .pt(px(4.0))
        .pb(px(10.0))
        .font_family(theme.mono_font_family.clone())
        .text_size(px(12.0))
        .line_height(px(18.0))
        .whitespace_nowrap()
        .text_color(theme.foreground)
        .children(lines.iter().map(|(tone, text)| {
            let line = div().opacity(if dimmed { crate::pane().inactive } else { 1.0 });
            match tone {
                Tone::Plain => line.child(*text),
                Tone::Muted => line.text_color(theme.foreground.muted()).child(*text),
                Tone::Success => line.text_color(theme.success).child(*text),
                Tone::Danger => line.text_color(theme.danger).child(*text),
                Tone::Prompt => line
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(*text)
                    .child(div().w(px(7.0)).h(px(14.0)).bg(theme.foreground)),
            }
        }))
}

pub fn placeholder_body(
    icon: IconName,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> zz_gpui::Div {
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .p(px(12.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(cx.theme().foreground)
        .child(
            Icon::new(icon)
                .size(px(18.0))
                .text_color(cx.theme().foreground.muted()),
        )
        .child(title.into())
        .child(
            div()
                .text_color(cx.theme().foreground.muted())
                .child(detail.into()),
        )
}

pub struct TerminalPane {
    pub id: u64,
    pub title: &'static str,
    pub lines: &'static [(Tone, &'static str)],
    pub active: bool,
    pub gaps: bool,
    pub lens: Option<PaneLens>,
}

impl TerminalPane {
    pub const fn new(id: u64, title: &'static str, lines: &'static [(Tone, &'static str)]) -> Self {
        Self {
            id,
            title,
            lines,
            active: false,
            gaps: false,
            lens: None,
        }
    }

    pub const fn lens(mut self, provider: AgentProvider) -> Self {
        self.lens = Some(PaneLens {
            provider,
            open: false,
        });
        self
    }

    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub const fn gaps(mut self, gaps: bool) -> Self {
        self.gaps = gaps;
        self
    }

    pub fn render(
        self,
        scope: impl Into<ElementId>,
        overlays: Vec<AnyElement>,
        cx: &App,
    ) -> Stateful<zz_gpui::Div> {
        let radii = radii(self.gaps, cx);
        let header = terminal_pane_header(
            self.active,
            self.title,
            PaneDragSource::new(PaneId(self.id), self.title, true, |_, _, _| {}),
            false,
            self.lens,
            |_, _, _| {},
            cx,
        );
        let content = div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex_none()
                    .rounded_tl(radii.top_left)
                    .rounded_tr(radii.top_right)
                    .child(
                        div()
                            .opacity(if self.active {
                                1.0
                            } else {
                                crate::pane().inactive
                            })
                            .child(header),
                    ),
            )
            .child(
                terminal_body(self.lines, !self.active, cx)
                    .rounded_bl(radii.bottom_left)
                    .rounded_br(radii.bottom_right),
            );
        pane_surface(
            scope,
            content,
            overlays,
            chrome(self.active, false, self.gaps, cx),
            cx,
        )
    }
}

pub fn placeholder_pane(
    scope: impl Into<ElementId>,
    icon: IconName,
    title: &'static str,
    detail: &'static str,
    active: bool,
    gaps: bool,
    overlays: Vec<AnyElement>,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    pane_surface(
        scope,
        placeholder_body(icon, title, detail, cx).rounded(radii(gaps, cx).top_left),
        overlays,
        chrome(active, true, gaps, cx),
        cx,
    )
}

pub fn browser_kind(url: &str) -> PaneKindSnapshot {
    PaneKindSnapshot::Browser(BrowserDescriptor::single(url.into(), "default".into()))
}

pub fn editor_kind(path: &str) -> PaneKindSnapshot {
    PaneKindSnapshot::Editor(EditorDescriptor {
        path: Some(path.into()),
        cwd: "/Users/fabrico/dev/zz".into(),
    })
}

pub fn status_pane(id: u64, label: &str, kind: PaneKindSnapshot, active: bool) -> StatusPaneEntry {
    StatusPaneEntry::from_pane(
        &StatusBarPane {
            id: PaneId(id),
            label: label.into(),
            kind,
            active,
        },
        noop(),
    )
}

pub fn workspace_panes() -> Vec<StatusPaneEntry> {
    vec![
        status_pane(1, "cargo test", PaneKindSnapshot::Terminal, false),
        status_pane(2, "zed.dev", browser_kind("https://zed.dev/docs"), true),
        status_pane(3, "codex", PaneKindSnapshot::Terminal, false),
        status_pane(4, "Review pane titles", PaneKindSnapshot::Terminal, false),
        status_pane(
            5,
            "status_bar.rs",
            editor_kind("crates/zz-ui/src/shell.rs"),
            false,
        ),
        status_pane(6, "dev server", PaneKindSnapshot::Terminal, false),
    ]
}

pub struct Fixture<T: 'static> {
    state: T,
    render: fn(&T, &mut Window, &mut App) -> AnyElement,
}

impl<T: 'static> Render for Fixture<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.render)(&self.state, window, cx)
    }
}

pub fn stateful<T: 'static>(
    window: &mut Window,
    cx: &mut App,
    state: impl FnOnce(&mut Window, &mut App) -> T,
    render: fn(&T, &mut Window, &mut App) -> AnyElement,
) -> AnyView {
    let state = state(window, cx);
    cx.new(|_| Fixture { state, render }).into()
}
