pub mod about;
pub mod appearance;
pub mod panes_preview;
pub mod providers;
pub mod status_bar_preview;

use std::rc::Rc;

pub use penna::settings::*;

use crate::{ActiveTheme as _, Colorize as _, icon::Icon};
use ala::{App, ElementId, SharedString, Styled as _, Window, div, prelude::*, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    StatusBar,
    Browser,
    Terminal,
    Editor,
    Panes,
    Hosts,
    Agents,
    Advanced,
    Multiplexer,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsNavigationGroup {
    Appearance,
    Tools,
    Advanced,
}

impl SettingsSection {
    pub const ALL: [Self; 11] = [
        Self::Appearance,
        Self::StatusBar,
        Self::Editor,
        Self::Panes,
        Self::Multiplexer,
        Self::Browser,
        Self::Terminal,
        Self::Hosts,
        Self::Agents,
        Self::Advanced,
        Self::About,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Interface",
            Self::StatusBar => "Status bar",
            Self::Browser => "Browser",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Panes => "Panes",
            Self::Multiplexer => "Multiplexer",
            Self::Hosts => "Hosts",
            Self::Agents => "Agents",
            Self::Advanced => "System",
            Self::About => "About",
        }
    }

    #[must_use]
    pub const fn icon(self) -> crate::IconName {
        match self {
            Self::Appearance => crate::IconName::Palette,
            Self::StatusBar => crate::IconName::PanelBottom,
            Self::Browser => crate::IconName::Globe,
            Self::Terminal => crate::IconName::SquareTerminal,
            Self::Editor => crate::IconName::File,
            Self::Panes => crate::IconName::LayoutDashboard,
            Self::Multiplexer => crate::IconName::GalleryVerticalEnd,
            Self::Hosts => crate::IconName::HardDrive,
            Self::Agents => crate::IconName::Bot,
            Self::Advanced => crate::IconName::Cpu,
            Self::About => crate::IconName::Info,
        }
    }

    #[must_use]
    pub const fn navigation_group(self) -> SettingsNavigationGroup {
        match self {
            Self::Appearance | Self::StatusBar | Self::Editor | Self::Panes => {
                SettingsNavigationGroup::Appearance
            }
            Self::Multiplexer | Self::Browser | Self::Terminal => SettingsNavigationGroup::Tools,
            Self::Hosts | Self::Agents | Self::Advanced | Self::About => {
                SettingsNavigationGroup::Advanced
            }
        }
    }
}

pub fn settings_section_navigation(
    id: impl Into<ElementId>,
    sections: &[SettingsSection],
    selected: SettingsSection,
    on_pick: impl Fn(SettingsSection, &mut Window, &mut App) + 'static,
) -> SettingsNavigation {
    let picks = sections.to_vec();
    settings_navigation(
        id,
        sections.iter().map(|section| NavSection {
            title: section.title().into(),
            icon: section.icon(),
            group: section.navigation_group() as usize,
        }),
        sections.iter().position(|section| *section == selected),
    )
    .on_pick(move |index, window, cx| on_pick(picks[index], window, cx))
}

pub fn settings_section_index(
    sections: &[SettingsSection],
    meta: impl Fn(SettingsSection) -> Option<SharedString>,
    on_pick: impl Fn(SettingsSection, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SettingsScrollColumn {
    let on_pick = Rc::new(on_pick);
    let muted = cx.theme().foreground.muted();
    let faint = cx.theme().foreground.faint();
    let entry = |section: SettingsSection| {
        let id = ElementId::Name(format!("settings-index-{}", section.title()).into());
        let on_pick = Rc::clone(&on_pick);
        SettingEntry::new(section.title())
            .title_icon(Icon::new(section.icon()).text_color(muted))
            .control(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .children(meta(section).map(|value| {
                        div()
                            .max_w(px(160.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(crate::rems_from_px(12.0))
                            .text_color(muted)
                            .child(value)
                    }))
                    .child(
                        Icon::new(crate::IconName::ChevronRight)
                            .size(px(14.0))
                            .text_color(faint),
                    ),
            )
            .on_click(id, move |_, window, cx| on_pick(section, window, cx))
    };
    let mut groups: Vec<(SettingsNavigationGroup, SettingsStack)> = Vec::new();
    for &section in sections {
        let group = section.navigation_group();
        match groups.last_mut() {
            Some((last, stack)) if *last == group => {
                *stack = std::mem::take(stack).child(entry(section));
            }
            _ => groups.push((group, SettingsStack::new().child(entry(section)))),
        }
    }
    settings_scroll_column("settings-index").children(groups.into_iter().map(|(_, stack)| stack))
}

pub fn settings_page_title(section: SettingsSection, cx: &App) -> ala::Div {
    settings_page_heading(section.title(), cx)
}

pub fn panes_page(
    preview: panes_preview::PanesPreview,
    layout: impl IntoIterator<Item = SettingEntry>,
    pip: [SettingEntry; 1],
    focus: [SettingEntry; 2],
    frame: [SettingEntry; 1],
    cx: &App,
) -> ala::Div {
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(
            settings_scroll_column("settings-panes")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(settings_page_title(SettingsSection::Panes, cx))
                        .child(settings_group_header("Preview".into(), cx))
                        .child(preview),
                )
                .child(SettingsStack::titled("Layout").children(layout))
                .child(SettingsStack::titled("PiP").children(pip))
                .child(SettingsStack::titled("Focus").children(focus))
                .child(SettingsStack::titled("Frame").children(frame)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Context, IntoElement, Render, TestAppContext, VisualTestContext};

    #[test]
    fn settings_section_titles_match_the_sidebar_labels() {
        assert_eq!(
            SettingsSection::ALL.map(SettingsSection::title),
            [
                "Interface",
                "Status bar",
                "Editor",
                "Panes",
                "Multiplexer",
                "Browser",
                "Terminal",
                "Hosts",
                "Agents",
                "System",
                "About",
            ]
        );
    }

    #[test]
    fn sections_follow_the_sidebar_groups() {
        assert_eq!(
            SettingsSection::ALL,
            [
                SettingsSection::Appearance,
                SettingsSection::StatusBar,
                SettingsSection::Editor,
                SettingsSection::Panes,
                SettingsSection::Multiplexer,
                SettingsSection::Browser,
                SettingsSection::Terminal,
                SettingsSection::Hosts,
                SettingsSection::Agents,
                SettingsSection::Advanced,
                SettingsSection::About,
            ]
        );
        assert_eq!(
            SettingsSection::ALL.map(SettingsSection::navigation_group),
            [
                SettingsNavigationGroup::Appearance,
                SettingsNavigationGroup::Appearance,
                SettingsNavigationGroup::Appearance,
                SettingsNavigationGroup::Appearance,
                SettingsNavigationGroup::Tools,
                SettingsNavigationGroup::Tools,
                SettingsNavigationGroup::Tools,
                SettingsNavigationGroup::Advanced,
                SettingsNavigationGroup::Advanced,
                SettingsNavigationGroup::Advanced,
                SettingsNavigationGroup::Advanced,
            ]
        );
    }

    struct PanesPageTest;

    impl Render for PanesPageTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let [gaps, background, opacity, glow, margin] =
                std::array::from_fn::<_, 5, _>(|index| {
                    SettingEntry::new(format!("Setting {index}")).control(
                        div()
                            .w(px(100.0))
                            .h(px(80.0))
                            .debug_selector(move || format!("panes-control-{index}")),
                    )
                });
            div().w(px(500.0)).h(px(600.0)).child(panes_page(
                panes_preview::PanesPreview {
                    gaps: true,
                    margin: 6.0,
                    inactive_opacity: 0.7,
                },
                [gaps],
                [background],
                [opacity, glow],
                [margin],
                cx,
            ))
        }
    }

    #[ala::test]
    fn panes_preview_scrolls_with_controls(cx: &mut TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let (_, cx) = cx.add_window_view(|_, _| PanesPageTest);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let preview = cx.debug_bounds("settings-preview-terminal").unwrap();
        let control = cx.debug_bounds("panes-control-0").unwrap();
        cx.simulate_event(ala::ScrollWheelEvent {
            position: control.center(),
            delta: ala::ScrollDelta::Pixels(ala::point(px(0.0), px(-120.0))),
            ..Default::default()
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let preview_offset = cx
            .debug_bounds("settings-preview-terminal")
            .unwrap()
            .origin
            .y
            - preview.origin.y;
        let control_offset =
            cx.debug_bounds("panes-control-0").unwrap().origin.y - control.origin.y;
        assert!(preview_offset < px(0.0));
        assert_eq!(preview_offset, control_offset);
    }
}
