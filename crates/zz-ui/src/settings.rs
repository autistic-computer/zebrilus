pub mod about;
pub mod appearance;
pub mod panes_preview;
pub mod providers;
pub mod status_bar_preview;

use std::rc::Rc;

use web_time::Duration;

pub use penna::settings::*;

use crate::{
    ActiveTheme as _, Colorize as _, PaintState,
    icon::{Icon, cap_center_drop},
    motion::{Disclosure, eased, turned},
    roles,
};
use ala::{
    AnyElement, App, ElementId, IntoElement, RenderOnce, SharedString, Styled as _, Window, div,
    prelude::*, px,
};

type EntryClick = Rc<dyn Fn(&ala::ClickEvent, &mut Window, &mut App)>;

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

#[cfg(test)]
mod scroll_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Context, Render, TestAppContext, VisualTestContext};

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

pub fn settings_page_title(section: SettingsSection, cx: &App) -> ala::Div {
    settings_page_heading(section.title(), cx)
}

pub fn settings_title_bar(
    title: impl Into<SharedString>,
    fold: &SettingsFold,
    back: impl IntoElement,
) -> SettingsTitleBar {
    SettingsTitleBar {
        title: title.into(),
        fold: fold.clone(),
        back: back.into_any_element(),
    }
}

#[derive(IntoElement)]
pub struct SettingsTitleBar {
    title: SharedString,
    fold: SettingsFold,
    back: AnyElement,
}

impl RenderOnce for SettingsTitleBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::compact::compact_nav_bar(cx)
            .relative()
            .child(
                crate::compact::folding_title_bar(self.title, self.fold.scrolled(), px(0.0), cx)
                    .h_full(),
            )
            .child(self.back)
    }
}

const REVEAL: Duration = Duration::from_millis(280);
const REVEAL_STAGGER: Duration = Duration::from_millis(30);
const REVEAL_DROP: f32 = 10.0;
const DISCLOSURE_PRESS_INSET: f32 = 6.0;
const SETTINGS_DISCLOSURE_WIDTH: f32 = 14.0;

pub fn settings_list_disclosure_header(
    id: &'static str,
    title: &'static str,
    expanded: bool,
) -> SettingsDisclosure {
    SettingsDisclosure {
        id,
        title,
        expanded,
        on_click: None,
    }
}

#[derive(IntoElement)]
pub struct SettingsDisclosure {
    id: &'static str,
    title: &'static str,
    expanded: bool,
    on_click: Option<EntryClick>,
}

impl SettingsDisclosure {
    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ala::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SettingsDisclosure {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().foreground.muted();
        let coarse = crate::touch::CoarsePointer::get(cx);
        let radius = cx
            .theme()
            .layer(roles::SETTINGS_CARD, PaintState::Rest)
            .radius;
        let inset = px(DISCLOSURE_PRESS_INSET);
        div()
            .id(self.id)
            .relative()
            .mt(px(10.0) - inset)
            .mb(-inset)
            .mx(-inset)
            .p(inset)
            .flex()
            .items_start()
            .gap(px(2.0))
            .cursor_pointer()
            .when(coarse, |this| {
                this.child(
                    crate::touch::press_highlight(
                        ElementId::from((ElementId::from(self.id), "press")),
                        None,
                        crate::touch::PressCorners::Adaptive(radius),
                    )
                    .inset_0(),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(px(SETTINGS_DISCLOSURE_WIDTH))
                    .h(px(SETTINGS_GROUP_TITLE_LINE))
                    .child(
                        div()
                            .relative()
                            .top(cap_center_drop(
                                px(SETTINGS_GROUP_TITLE_SIZE),
                                px(SETTINGS_GROUP_TITLE_SIZE),
                                cx,
                            ))
                            .child(
                                Disclosure::new(
                                    self.id,
                                    self.expanded,
                                    px(SETTINGS_GROUP_TITLE_SIZE),
                                )
                                .color(muted),
                            ),
                    ),
            )
            .child(settings_group_header(self.title.into(), cx))
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}

pub fn settings_disclosure_reveal(
    id: &'static str,
    expanded: bool,
    order: usize,
    row: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let turned = turned(id, expanded, cx);
    if !expanded || turned.is_none() {
        return row;
    }
    let delay = REVEAL_STAGGER * u32::try_from(order).unwrap_or(u32::MAX).min(12);
    let shown = eased(turned, delay, REVEAL, window, cx);
    if shown >= 1.0 {
        return row;
    }
    div()
        .relative()
        .top(px(-REVEAL_DROP * (1.0 - shown)))
        .opacity(shown)
        .child(row)
        .into_any_element()
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
