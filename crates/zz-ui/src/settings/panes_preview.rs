use crate::{
    ActiveTheme as _, Colorize as _, IconName,
    browser::{
        BrowserHeader, BrowserTabInfo, BrowserTabStrip, BrowserToolbar, browser_address,
        browser_site_controls_button, browser_toolbar_button,
    },
    input::InputState,
    pane::{
        PaneChrome, PaneDragSource, PaneLens, PaneSplitAxis, PaneSplitHighlight, PaneSplitSide,
        pane_header_actions, pane_header_frame, pane_header_icon_button, pane_split_surface,
        pane_surface, terminal_pane_header,
    },
    pane_look::PaneLook,
};
use ala::{
    AnyElement, App, Corners, Hsla, IntoElement, Pixels, RenderOnce, Window, div, prelude::*, px,
};
use zz_protocol::{AgentProvider, PaneId};

#[derive(Clone, Copy, IntoElement)]
pub struct PanesPreview {
    pub gaps: bool,
    pub margin: f32,
    pub inactive_opacity: f32,
}

#[derive(Clone, Copy)]
enum Line {
    Plain(&'static str),
    Muted(&'static str),
    Success(&'static str),
    Prompt(&'static str),
}

const SHELL: &[Line] = &[
    Line::Muted("~/workspace"),
    Line::Plain("$ cargo run"),
    Line::Muted("   Compiling workspace v0.1.0"),
    Line::Success("    Finished in 0.42s"),
    Line::Plain("Hello, workspace."),
    Line::Prompt("$"),
];

const CLAUDE: &[Line] = &[
    Line::Muted("~/workspace"),
    Line::Plain("> Fix the flaky drag test"),
    Line::Muted("• Bash(cargo test drag)"),
    Line::Success("  └ 9 passed; 0 failed"),
    Line::Prompt(">"),
];

impl RenderOnce for PanesPreview {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selection =
            window.use_keyed_state("settings-pane-preview-selection", cx, |_, _| 0_usize);
        let address = window.use_keyed_state("settings-pane-preview-address", cx, |window, cx| {
            InputState::new(window, cx).default_value("zed.dev/docs")
        });
        let selected = *selection.read(cx);
        let margin = px(if self.gaps { self.margin } else { 0.0 });
        let radii = Corners::all(if self.gaps {
            cx.theme().pane_radius()
        } else {
            px(0.0)
        });
        let height = (f32::from(window.viewport_size().height) * 0.4)
            .clamp(300.0, 380.0)
            .max(f32::from(margin) * 3.0 + 112.0);
        let backdrop = cx.theme().background;
        let opacity = |index: usize| {
            if selected == index {
                1.0
            } else {
                self.inactive_opacity.clamp(0.0, 1.0)
            }
        };
        let pane = |index: usize, content: ala::Div, covers_frame: bool| {
            let active = selected == index;
            pane_surface(
                ("settings-preview-pane", index),
                content
                    .size_full()
                    .debug_selector(move || format!("settings-preview-active-{index}-{active}")),
                [div()
                    .id(("settings-preview-pick", index))
                    .absolute()
                    .inset_0()
                    .occlude()
                    .cursor_pointer()
                    .on_click(window.listener_for(&selection, move |selected, _, _, cx| {
                        if *selected != index {
                            *selected = index;
                            cx.notify();
                        }
                    }))
                    .into_any_element()],
                PaneChrome::new(radii, self.gaps)
                    .active(active)
                    .rim_over_content(covers_frame)
                    .frame(
                        self.gaps
                            .then(|| PaneLook::global(cx).frame(cx.theme(), backdrop, active)),
                    ),
                cx,
            )
            .min_w_0()
            .min_h_0()
            .debug_selector(move || {
                [
                    "settings-preview-terminal",
                    "settings-preview-agent",
                    "settings-preview-browser",
                ][index]
                    .to_owned()
            })
        };
        let terminal = pane(
            0,
            terminal_sample(
                1,
                "cargo run",
                None,
                SHELL,
                selected == 0,
                opacity(0),
                radii,
                cx,
            ),
            false,
        );
        let agent = pane(
            1,
            terminal_sample(
                2,
                "claude",
                Some(PaneLens {
                    provider: AgentProvider::ClaudeCode,
                    open: false,
                }),
                CLAUDE,
                selected == 1,
                opacity(1),
                radii,
                cx,
            ),
            false,
        );
        let browser = pane(
            2,
            browser_sample(&address, selected == 2, opacity(2), radii, window, cx),
            true,
        );
        let horizontal_highlight = match selected {
            0 => PaneSplitHighlight::new(0.0, 1.0, PaneSplitSide::First, cx.theme().accent),
            1 => PaneSplitHighlight::new(0.0, 0.5, PaneSplitSide::Second, cx.theme().accent),
            _ => PaneSplitHighlight::new(0.5, 0.5, PaneSplitSide::Second, cx.theme().accent),
        };
        let vertical_highlight = match selected {
            1 => Some(PaneSplitHighlight::new(
                0.0,
                1.0,
                PaneSplitSide::First,
                cx.theme().accent,
            )),
            2 => Some(PaneSplitHighlight::new(
                0.0,
                1.0,
                PaneSplitSide::Second,
                cx.theme().accent,
            )),
            _ => None,
        };
        let right = pane_split_surface(
            "settings-preview-vertical-split",
            PaneSplitAxis::Vertical,
            0.5,
            false,
            self.gaps,
            margin,
            None,
            vertical_highlight,
            agent,
            browser,
            div().absolute(),
            cx,
        );
        super::settings_preview(backdrop, cx)
            .id("settings-panes-preview")
            .w_full()
            .h(px(height))
            .flex_none()
            .text_color(cx.theme().foreground)
            .p(margin)
            .child(pane_split_surface(
                "settings-preview-horizontal-split",
                PaneSplitAxis::Horizontal,
                0.55,
                false,
                self.gaps,
                margin,
                None,
                Some(horizontal_highlight),
                terminal,
                right,
                div().absolute(),
                cx,
            ))
    }
}

fn noop_drag(pane: u64, title: &'static str) -> PaneDragSource {
    PaneDragSource::new(PaneId(pane), title, false, |_, _, _| {})
}

#[allow(
    clippy::too_many_arguments,
    reason = "a sample pane is the sum of these independent pieces"
)]
fn terminal_sample(
    pane: u64,
    title: &'static str,
    lens: Option<PaneLens>,
    lines: &'static [Line],
    active: bool,
    opacity: f32,
    radii: Corners<Pixels>,
    cx: &App,
) -> ala::Div {
    let header = terminal_pane_header(
        active,
        title,
        noop_drag(pane, title),
        false,
        lens,
        |_, _, _| {},
        cx,
    );
    div()
        .flex()
        .flex_col()
        .child(pane_header_frame(header, radii, opacity))
        .child(terminal_body(lines, opacity, cx))
}

fn terminal_body(lines: &'static [Line], opacity: f32, cx: &App) -> ala::Div {
    let theme = cx.theme();
    let tone = |color: Hsla, text: &'static str| div().text_color(color).child(text);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .px(px(12.0))
        .pt(px(4.0))
        .font_family(theme.mono_font_family.clone())
        .text_size(px(12.0))
        .line_height(px(18.0))
        .whitespace_nowrap()
        .opacity(opacity)
        .children(lines.iter().map(|line| -> AnyElement {
            match *line {
                Line::Plain(text) => tone(theme.foreground, text).into_any_element(),
                Line::Muted(text) => tone(theme.foreground.muted(), text).into_any_element(),
                Line::Success(text) => tone(theme.success, text).into_any_element(),
                Line::Prompt(text) => tone(theme.foreground, text)
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(div().w(px(7.0)).h(px(14.0)).bg(theme.foreground))
                    .into_any_element(),
            }
        }))
}

fn browser_sample(
    address: &ala::Entity<InputState>,
    active: bool,
    opacity: f32,
    radii: Corners<Pixels>,
    window: &Window,
    cx: &App,
) -> ala::Div {
    let button = |id, icon, tooltip, disabled| {
        browser_toolbar_button(cx, id, icon, tooltip, disabled, false)
    };
    let actions = div().flex().flex_none().items_center().gap_1().children(
        pane_header_actions(false)
            .iter()
            .map(|(_, icon, label)| pane_header_icon_button(*label, icon.clone(), true, cx)),
    );
    let header = BrowserHeader::new(
        active,
        BrowserTabStrip::new(vec![BrowserTabInfo::new(1, "zed.dev", "Zed docs")], 0)
            .title(noop_drag(3, "Browser"), |_, _| {}),
        actions,
        BrowserToolbar::new(
            button("browser-back", IconName::ArrowLeft, "Back", false),
            button("browser-forward", IconName::ArrowRight, "Forward", true),
            button("browser-reload", IconName::Redo2, "Reload", false),
            browser_address(address, browser_site_controls_button(cx), window, cx),
            button(
                "browser-element-picker",
                IconName::Inspector,
                "Pick an element",
                false,
            ),
            button(
                "browser-more",
                IconName::EllipsisVertical,
                "More browser actions",
                false,
            ),
        ),
    );
    div()
        .flex()
        .flex_col()
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .child(
            div()
                .h(BrowserHeader::HEIGHT)
                .w_full()
                .flex_none()
                .opacity(opacity)
                .child(header),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .gap(px(4.0))
                .px(px(16.0))
                .py(px(12.0))
                .rounded_bl(radii.bottom_left)
                .rounded_br(radii.bottom_right)
                .bg(cx.theme().background.opaque())
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(div().text_size(px(16.0)).child("Key bindings"))
                .child(
                    div()
                        .text_color(cx.theme().foreground.muted())
                        .child("Zed has a very customizable key binding system."),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Context, Modifiers, Render, TestAppContext, VisualTestContext};

    struct PreviewTest(PanesPreview);

    impl Render for PreviewTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(500.0)).child(self.0)
        }
    }

    fn preview() -> PanesPreview {
        PanesPreview {
            gaps: true,
            margin: 8.0,
            inactive_opacity: 0.6,
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[ala::test]
    fn clicking_samples_changes_selection_and_preserves_it_on_settings_updates(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, _| PreviewTest(preview()));
        draw(cx);
        assert!(cx.debug_bounds("settings-preview-active-0-true").is_some());
        for (pane, active) in [
            ("settings-preview-agent", "settings-preview-active-1-true"),
            ("settings-preview-browser", "settings-preview-active-2-true"),
            (
                "settings-preview-terminal",
                "settings-preview-active-0-true",
            ),
        ] {
            let bounds = cx.debug_bounds(pane).unwrap();
            cx.simulate_click(bounds.center(), Modifiers::none());
            draw(cx);
            assert!(cx.debug_bounds(active).is_some());
        }
        let bounds = cx.debug_bounds("settings-preview-agent").unwrap();
        cx.simulate_click(bounds.center(), Modifiers::none());
        view.update(cx, |view, cx| {
            view.0.margin = 16.0;
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("settings-preview-active-1-true").is_some());
        assert!(cx.debug_bounds("settings-preview-active-0-true").is_none());
    }

    #[ala::test]
    fn spacing_updates_use_workspace_pixel_geometry(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (view, cx) = cx.add_window_view(|_, _| PreviewTest(preview()));
        for (gaps, margin) in [(true, 8.0), (true, 32.0), (false, 32.0)] {
            view.update(cx, |view, cx| {
                view.0.gaps = gaps;
                view.0.margin = margin;
                cx.notify();
            });
            draw(cx);
            let terminal = cx.debug_bounds("settings-preview-terminal").unwrap();
            let agent = cx.debug_bounds("settings-preview-agent").unwrap();
            let browser = cx.debug_bounds("settings-preview-browser").unwrap();
            let terminal_content = cx.debug_bounds("settings-preview-active-0-true").unwrap();
            let gap = px(if gaps { margin } else { 1.0 });
            let edge = cx.update(|_, cx| cx.theme().outline);
            assert_eq!(agent.origin.x - terminal.right(), gap);
            assert_eq!(browser.origin.y - agent.bottom(), gap);
            assert_eq!(
                terminal.origin.x,
                edge + px(if gaps { margin } else { 0.0 })
            );
            assert_eq!(terminal_content.origin, terminal.origin);
            assert!(browser.size.height > px(40.0));
        }
    }
}
