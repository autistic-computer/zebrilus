pub mod about;
pub mod appearance;
pub mod panes_preview;
pub mod providers;
pub mod status_bar_preview;

use std::rc::Rc;

use web_time::Duration;

use crate::Colorize as _;
use crate::{
    ActiveTheme as _, Emphasis, Ground, Layer, PaintState, RULE_WIDTH, Selectable as _,
    Sizable as _, StyledExt as _,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    icon::{Icon, cap_center_drop},
    motion::{Disclosure, eased, turned},
    roles,
    scroll::ScrollableElement as _,
    select::SelectItem,
    separator::Separator,
};
use zz_gpui::{
    AnyElement, App, Bounds, Corners, ElementId, FocusHandle, Hsla, IntoElement, ListAlignment,
    ListSizingBehavior, ListState, ParentElement, Pixels, RenderOnce, ScrollHandle, SharedString,
    Styled as _, Window, div, list, prelude::*, px, relative,
};

type EntryClick = Rc<dyn Fn(&zz_gpui::ClickEvent, &mut Window, &mut App)>;

/// A page in the settings sidebar, in sidebar order.
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

/// A run of related pages in the settings sidebar, set apart from the next
/// run by a gap.
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

/// A settings-section navigation button. The caller attaches behavior. It
/// draws as [`roles::SIDEBAR_ROW`], the role of the session tree's rows, so a
/// theme styles both sidebars at once.
pub fn settings_navigation_button(section: SettingsSection, selected: bool, cx: &App) -> Button {
    Button::new(section.title())
        .w_full()
        .px(px(8.0))
        .ghost()
        .surface_role(roles::SIDEBAR_ROW)
        .on(cx.theme().ground_of(roles::SIDEBAR))
        .icon(section.icon())
        .selected(selected)
        .label(section.title())
        .child(div().flex_1())
        .when(selected, crate::StyledExt::font_medium)
}

/// The row that leaves the settings route, above the section list.
pub fn settings_navigation_back_button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .w_full()
        .px(px(8.0))
        .ghost()
        .surface_role(roles::SIDEBAR_ROW)
        .icon(crate::IconName::ArrowLeft)
        .label("Back")
        .child(div().flex_1())
}

/// Holds the back button at the workspace tree's row height.
pub fn settings_navigation_back_row(button: Button) -> zz_gpui::Div {
    div()
        .flex()
        .flex_none()
        .h(px(crate::navigation::WORKSPACE_TREE_ROW_HEIGHT))
        .items_center()
        .child(button)
}

/// The space between two navigation groups in the settings sidebar.
pub fn settings_navigation_group_gap() -> zz_gpui::Div {
    div().flex_none().h(px(12.0))
}

/// The settings root on a narrow screen: the sidebar's groups as stacks of
/// touch rows. `meta` puts a short value at the end of a row, and tapping a
/// row hands its section to `on_pick`.
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use zz_gpui::{Context, Render, TestAppContext, VisualTestContext};

    struct VirtualSettingsTest {
        rendered: Arc<AtomicUsize>,
    }

    impl Render for VirtualSettingsTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = Arc::clone(&self.rendered);
            div()
                .flex()
                .w(px(400.0))
                .h(px(220.0))
                .child(settings_virtual_column(
                    "virtual-settings-test",
                    100,
                    move |index, _, _| {
                        rendered.fetch_add(1, Ordering::Relaxed);
                        div()
                            .h(px(120.0))
                            .flex_none()
                            .debug_selector(move || format!("virtual-settings-row-{index}"))
                            .into_any_element()
                    },
                ))
        }
    }

    struct FocusedRowTest {
        focus: FocusHandle,
    }

    impl Render for FocusedRowTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            div().flex().w(px(400.0)).h(px(220.0)).child(
                settings_virtual_column("focused-row-test", 100, move |index, _, _| {
                    div()
                        .h(px(120.0))
                        .flex_none()
                        .when(index == 99, |row| row.track_focus(&focus))
                        .debug_selector(move || format!("focused-row-{index}"))
                        .into_any_element()
                })
                .row_focus((0..100).map(|index| (index == 99).then(|| self.focus.clone()))),
            )
        }
    }

    struct RevealTest {
        input: zz_gpui::Entity<crate::input::InputState>,
        virtual_rows: bool,
    }

    impl Render for RevealTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let input = self.input.clone();
            let focus = zz_gpui::Focusable::focus_handle(self.input.read(cx), cx);
            let row = move |index: usize| {
                if index == 8 {
                    div()
                        .flex_none()
                        .debug_selector(|| "reveal-field".to_owned())
                        .child(crate::input::Input::new(&input))
                } else {
                    div().h(px(50.0)).flex_none()
                }
            };
            let page = if self.virtual_rows {
                settings_virtual_column("reveal-virtual", 10, move |index, _, _| {
                    row(index).into_any_element()
                })
                .row_focus((0..10).map(|index| (index == 8).then(|| focus.clone())))
                .into_any_element()
            } else {
                settings_scroll_column("reveal-scroll")
                    .children((0..10).map(row))
                    .into_any_element()
            };
            div().flex().w(px(400.0)).h(px(220.0)).child(page)
        }
    }

    fn reveal_field(cx: &mut TestAppContext, virtual_rows: bool) -> Bounds<Pixels> {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|window, cx| RevealTest {
            input: cx.new(|cx| crate::input::InputState::new(window, cx)),
            virtual_rows,
        });
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| {
            window.set_a11y_forced(true);
            _ = window.draw(cx);
        });
        let hidden = cx.debug_bounds("reveal-field");
        assert!(hidden.is_none_or(|field| field.top() >= px(220.0)));
        cx.update(|window, cx| {
            let focus = zz_gpui::Focusable::focus_handle(view.read(cx).input.read(cx), cx);
            focus.focus(window, cx);
        });
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
        cx.debug_bounds("reveal-field")
            .expect("the focused field paints")
    }

    #[zz_gpui::test]
    fn a_focused_field_scrolls_into_a_list_page(cx: &mut TestAppContext) {
        let field = reveal_field(cx, true);
        assert!(field.top() >= px(0.0) && field.bottom() <= px(220.0));
    }

    #[zz_gpui::test]
    fn a_focused_field_scrolls_into_a_scroll_page(cx: &mut TestAppContext) {
        let field = reveal_field(cx, false);
        assert!(field.top() >= px(0.0) && field.bottom() <= px(220.0));
    }

    struct SettingsColumnGutterTest;

    impl Render for SettingsColumnGutterTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .w(px(400.0))
                .h(px(440.0))
                .child(
                    div().flex().w(px(400.0)).h(px(220.0)).child(
                        settings_scroll_column("scroll-gutter-test").child(
                            div()
                                .h(px(50.0))
                                .flex_none()
                                .debug_selector(|| "scroll-gutter-row".to_owned()),
                        ),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .w(px(400.0))
                        .h(px(220.0))
                        .child(settings_virtual_column(
                            "virtual-gutter-test",
                            1,
                            |_, _, _| {
                                div()
                                    .h(px(50.0))
                                    .flex_none()
                                    .debug_selector(|| "virtual-gutter-row".to_owned())
                                    .into_any_element()
                            },
                        )),
                )
        }
    }

    #[test]
    fn stack_positions_round_and_rule_the_ends_of_a_run() {
        assert_eq!(StackPosition::at(0, 0), StackPosition::Only);
        assert_eq!(StackPosition::at(0, 2), StackPosition::First);
        assert_eq!(StackPosition::at(1, 2), StackPosition::Middle);
        assert_eq!(StackPosition::at(2, 2), StackPosition::Last);

        for last in 0..4 {
            let run: Vec<_> = (0..=last)
                .map(|index| StackPosition::at(index, last))
                .collect();
            assert_eq!(run.iter().filter(|p| p.rounds_top()).count(), 1);
            assert_eq!(run.iter().filter(|p| p.rounds_bottom()).count(), 1);
            assert_eq!(run.iter().filter(|p| p.rules_above()).count(), last);
            assert!(run[0].rounds_top() && !run[0].rules_above());
            assert!(run[last].rounds_bottom());
        }
    }

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

    #[zz_gpui::test]
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
        cx.simulate_event(zz_gpui::ScrollWheelEvent {
            position: control.center(),
            delta: zz_gpui::ScrollDelta::Pixels(zz_gpui::point(px(0.0), px(-120.0))),
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

    #[zz_gpui::test]
    fn one_long_scroll_reaches_the_last_row(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let rendered = Arc::new(AtomicUsize::new(0));
        let rendered_for_view = Arc::clone(&rendered);
        let (_, cx) = cx.add_window_view(move |_, _| VirtualSettingsTest {
            rendered: rendered_for_view,
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        assert!(cx.debug_bounds("virtual-settings-row-0").is_some());
        assert!(cx.debug_bounds("virtual-settings-row-99").is_none());

        cx.simulate_event(zz_gpui::ScrollWheelEvent {
            position: zz_gpui::point(px(100.0), px(100.0)),
            delta: zz_gpui::ScrollDelta::Pixels(zz_gpui::point(px(0.0), px(-100_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("virtual-settings-row-99").is_some());
        assert!(cx.debug_bounds("virtual-settings-row-0").is_none());
    }

    #[zz_gpui::test]
    fn a_focused_row_keeps_painting_out_of_view(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, cx| FocusedRowTest {
            focus: cx.focus_handle(),
        });
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| {
            view.read(cx).focus.clone().focus(window, cx);
            _ = window.draw(cx);
        });

        assert!(cx.debug_bounds("focused-row-0").is_some());
        assert!(cx.debug_bounds("focused-row-99").is_some());
        assert!(cx.debug_bounds("focused-row-98").is_none());

        cx.update(|window, cx| {
            window.blur(cx);
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("focused-row-99").is_none());
    }

    struct InsetPageTest;

    impl Render for InsetPageTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().w(px(400.0)).h(px(220.0)).child(
                settings_scroll_column("inset-test").children((0..10).map(|index| {
                    div()
                        .h(px(50.0))
                        .flex_none()
                        .debug_selector(move || format!("inset-row-{index}"))
                })),
            )
        }
    }

    #[zz_gpui::test]
    fn the_bottom_inset_keeps_the_last_row_clear(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_global(SettingsBottomInset(px(40.0)));
        });
        let (_, cx) = cx.add_window_view(|_, _| InsetPageTest);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.simulate_event(zz_gpui::ScrollWheelEvent {
            position: zz_gpui::point(px(100.0), px(100.0)),
            delta: zz_gpui::ScrollDelta::Pixels(zz_gpui::point(px(0.0), px(-10_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let last = cx.debug_bounds("inset-row-9").expect("last row");
        assert!(last.bottom() <= px(220.0 - 40.0));
    }

    #[zz_gpui::test]
    fn virtual_rows_land_where_scrolled_rows_do(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| SettingsColumnGutterTest);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let scrolled = cx
            .debug_bounds("scroll-gutter-row")
            .expect("the scrolled column renders its row");
        let virtualized = cx
            .debug_bounds("virtual-gutter-row")
            .expect("the virtual column renders its row");
        assert_eq!(scrolled.origin.x, virtualized.origin.x);
        assert_eq!(scrolled.size.width, virtualized.size.width);
    }
}

const SETTINGS_CONTENT_MAX_WIDTH: f32 = 760.0;
const SETTINGS_PAGE_PADDING: f32 = 14.0;

/// Centered, bounded content shared by every settings page.
pub fn settings_page_content() -> zz_gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .max_w(px(SETTINGS_CONTENT_MAX_WIDTH))
        .mx_auto()
}

/// The heading of `section`'s page.
pub fn settings_page_title(section: SettingsSection, cx: &App) -> zz_gpui::Div {
    settings_page_heading(section.title(), cx)
}

/// A settings page's title. Under a coarse pointer it leaves the title to the
/// sidebar or the bar above the page.
pub fn settings_page_heading(title: impl Into<SharedString>, cx: &App) -> zz_gpui::Div {
    let title = title.into();
    div()
        .flex()
        .flex_col()
        .flex_none()
        .when(!crate::touch::CoarsePointer::get(cx), |this| {
            this.child(
                crate::StyledExt::font_medium(div().text_size(crate::rems_from_px(20.0)))
                    .child(title),
            )
        })
}

/// Room a host keeps clear below the last row of every settings page, such as
/// an iOS home indicator the page draws under. Unset means none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SettingsBottomInset(pub Pixels);

impl zz_gpui::Global for SettingsBottomInset {}

impl SettingsBottomInset {
    #[must_use]
    pub fn get(cx: &App) -> Pixels {
        cx.try_global::<Self>().map_or(px(0.0), |inset| inset.0)
    }
}

/// A settings page: a scrolling column of [`SettingsStack`]s, with the shared
/// scrollbar overlaid. `id` keys the scroll handle, so each page keeps its own
/// position.
#[must_use]
pub fn settings_scroll_column(id: &'static str) -> SettingsScrollColumn {
    SettingsScrollColumn {
        id,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsScrollColumn {
    id: &'static str,
    children: Vec<AnyElement>,
}

impl zz_gpui::ParentElement for SettingsScrollColumn {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsScrollColumn {
    fn render(self, window: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        let handle = window
            .use_keyed_state(zz_gpui::ElementId::Name(self.id.into()), cx, |_, _| {
                zz_gpui::ScrollHandle::default()
            })
            .read(cx)
            .clone();
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(
                div()
                    .on_children_prepainted({
                        let handle = handle.clone();
                        move |_, window, _| {
                            if let Some(target) = window.take_autoscroll()
                                && reveal_in_scroll(&handle, target)
                            {
                                window.request_animation_frame();
                            }
                        }
                    })
                    .id(self.id)
                    .flex()
                    .flex_col()
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .p(px(SETTINGS_PAGE_PADDING))
                    .child(
                        settings_page_content()
                            .flex_none()
                            .gap(px(18.0))
                            .children(self.children),
                    )
                    .child(div().flex_none().h(SettingsBottomInset::get(cx))),
            )
            .vertical_scrollbar(&handle)
    }
}

fn reveal_shift(viewport: Bounds<Pixels>, target: Bounds<Pixels>) -> Option<Pixels> {
    if target.right() <= viewport.left() || target.left() >= viewport.right() {
        return None;
    }
    if target.bottom() > viewport.bottom() {
        Some((target.bottom() - viewport.bottom()).min(target.top() - viewport.top()))
    } else if target.top() < viewport.top() {
        Some(target.top() - viewport.top())
    } else {
        None
    }
}

fn reveal_in_scroll(handle: &ScrollHandle, target: Bounds<Pixels>) -> bool {
    let Some(shift) = reveal_shift(handle.bounds(), target) else {
        return false;
    };
    let mut offset = handle.offset();
    let floor = -handle.max_offset().y.max(px(0.0));
    let next = (offset.y - shift).clamp(floor, px(0.0));
    if next == offset.y {
        return false;
    }
    offset.y = next;
    handle.set_offset(offset);
    true
}

const SETTINGS_LIST_OVERDRAW: f32 = 24.0;

type SettingsItemRenderer = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static>;

/// A settings page that paints only rows in or around the viewport. Every row
/// is measured once per width, so a fling reaches the real end of the page.
/// Rows keep [`settings_scroll_column`]'s bounded content width, but each owns
/// the space beneath it, so a glued run of entries stays glued.
#[must_use]
pub fn settings_virtual_column(
    id: &'static str,
    item_count: usize,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> SettingsVirtualColumn {
    SettingsVirtualColumn {
        id,
        item_count,
        render_item: Box::new(render_item),
        row_focus: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsVirtualColumn {
    id: &'static str,
    item_count: usize,
    render_item: SettingsItemRenderer,
    row_focus: Vec<Option<FocusHandle>>,
}

impl SettingsVirtualColumn {
    /// The focus handle of each row's text field. A row whose field holds focus
    /// keeps painting after it leaves the viewport, so scrolling or a soft
    /// keyboard covering it does not take its input away.
    #[must_use]
    pub fn row_focus(mut self, handles: impl IntoIterator<Item = Option<FocusHandle>>) -> Self {
        self.row_focus = handles.into_iter().collect();
        self
    }
}

impl RenderOnce for SettingsVirtualColumn {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state_key = ElementId::Name(format!("{}-list-state", self.id).into());
        let mut focus = self.row_focus;
        focus.resize(self.item_count, None);
        let list_state = window
            .use_keyed_state(state_key, cx, |_, _| {
                (
                    ListState::new(0, ListAlignment::Top, px(SETTINGS_LIST_OVERDRAW)).measure_all(),
                    Vec::new(),
                )
            })
            .update(cx, |(list_state, applied), _| {
                if *applied != focus {
                    let kept = applied
                        .iter()
                        .zip(&focus)
                        .take_while(|(was, is)| was == is)
                        .count();
                    list_state.splice_focusable(kept..applied.len(), focus[kept..].iter().cloned());
                    list_state.remeasure_items(kept..focus.len());
                    *applied = focus;
                }
                list_state.clone()
            });

        let mut render_item = self.render_item;
        let rows = list(list_state.clone(), move |index, window, cx| {
            div()
                .flex()
                .w_full()
                .px(px(SETTINGS_PAGE_PADDING))
                .child(
                    settings_page_content()
                        .flex_none()
                        .child(render_item(index, window, cx)),
                )
                .into_any_element()
        })
        .with_sizing_behavior(ListSizingBehavior::Auto)
        .size_full()
        .pt(px(SETTINGS_PAGE_PADDING))
        .pb(px(SETTINGS_PAGE_PADDING) + SettingsBottomInset::get(cx));

        div()
            .id(self.id)
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(rows)
            .vertical_scrollbar(&list_state)
    }
}

const SETTINGS_GROUP_TITLE_SIZE: f32 = 13.0;
const SETTINGS_GROUP_TITLE_LINE: f32 = 20.0;
const SETTINGS_DESCRIPTION_SIZE: f32 = 12.0;
const SETTINGS_DISCLOSURE_WIDTH: f32 = 14.0;

fn settings_group_header(title: SharedString, cx: &App) -> zz_gpui::Div {
    crate::StyledExt::font_semibold(
        div()
            .px(px(2.0))
            .text_size(crate::rems_from_px(SETTINGS_GROUP_TITLE_SIZE))
            .line_height(px(SETTINGS_GROUP_TITLE_LINE))
            .text_color(cx.theme().foreground.muted()),
    )
    .child(title)
}

fn settings_description(text: SharedString, cx: &App) -> zz_gpui::Div {
    div()
        .text_size(crate::rems_from_px(SETTINGS_DESCRIPTION_SIZE))
        .text_color(cx.theme().foreground.muted())
        .child(text)
}

const REVEAL: Duration = Duration::from_millis(280);
const REVEAL_STAGGER: Duration = Duration::from_millis(30);
const REVEAL_DROP: f32 = 10.0;
const DISCLOSURE_PRESS_INSET: f32 = 6.0;

/// A group heading that shows or hides the rows after it, as a standalone
/// row in [`settings_virtual_column`]. Its chevron turns as it opens and
/// closes, and a touch lights it.
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
        handler: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
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

/// `row`, the `order`th row the disclosure `id` shows while `expanded`,
/// fading in and settling into place while the disclosure opens.
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

/// Group heading used as a standalone row in [`settings_virtual_column`].
pub fn settings_list_group_header(title: &'static str, cx: &App) -> zz_gpui::Div {
    div()
        .pt(px(10.0))
        .child(settings_group_header(title.into(), cx))
}

const SETTINGS_STACK_PADDING: f32 = 12.0;

/// A run of settings sharing one surface, divided by hairlines. Fill it with
/// [`SettingEntry`].
#[derive(IntoElement)]
pub struct SettingsStack {
    title: Option<SharedString>,
    note: Option<SharedString>,
    entries: Vec<SettingEntry>,
}

impl SettingsStack {
    /// An untitled stack, for a run that needs no heading of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: None,
            note: None,
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn titled(title: impl Into<SharedString>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::new()
        }
    }

    /// A line under the title saying why the stack's rows are off. Keep
    /// settings help on the rows, with [`SettingEntry::description`].
    #[must_use]
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Append an entry. The stack assigns its [`StackPosition`] on render.
    #[must_use]
    pub fn child(mut self, entry: SettingEntry) -> Self {
        self.entries.push(entry);
        self
    }

    #[must_use]
    pub fn children(mut self, entries: impl IntoIterator<Item = SettingEntry>) -> Self {
        self.entries.extend(entries);
        self
    }
}

impl Default for SettingsStack {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for SettingsStack {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        let last = self.entries.len().saturating_sub(1);
        let rows = self
            .entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| entry.position(StackPosition::at(index, last)));

        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .when_some(self.title, |this, title| {
                this.child(settings_group_header(title, cx))
            })
            .when_some(self.note, |this, note| {
                this.child(settings_description(note, cx).px(px(2.0)))
            })
            .child(div().flex().flex_col().w_full().children(rows))
    }
}

/// Where a [`SettingEntry`] sits in its run: which corners it rounds, and
/// whether a rule separates it from the entry above. [`SettingsStack`] assigns
/// this; a virtualized page has to assign it by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StackPosition {
    /// The only entry in its run: rounds all four corners.
    Only,
    /// Rounds the top corners, and draws no rule above.
    First,
    #[default]
    Middle,
    /// Rounds the bottom corners.
    Last,
}

impl StackPosition {
    /// The position implied by which ends of the run an entry is on. This is
    /// the form a virtualized page needs, where a run is bounded by whatever is
    /// not an entry rather than by a comparable index.
    #[must_use]
    pub const fn new(is_first: bool, is_last: bool) -> Self {
        match (is_first, is_last) {
            (true, true) => Self::Only,
            (true, false) => Self::First,
            (false, true) => Self::Last,
            (false, false) => Self::Middle,
        }
    }

    /// The position of item `index` in a run whose last index is `last`.
    #[must_use]
    pub const fn at(index: usize, last: usize) -> Self {
        Self::new(index == 0, index == last)
    }

    /// Whether the run stops here. The entry rounds its bottom corners, and a
    /// virtualized page owes it the gap before whatever comes next.
    #[must_use]
    pub const fn ends_run(self) -> bool {
        matches!(self, Self::Only | Self::Last)
    }

    const fn rounds_top(self) -> bool {
        matches!(self, Self::Only | Self::First)
    }

    const fn rounds_bottom(self) -> bool {
        self.ends_run()
    }

    const fn rules_above(self) -> bool {
        matches!(self, Self::Middle | Self::Last)
    }
}

/// One setting inside a [`SettingsStack`]: its title, and a description when
/// one helps, on the left, its control centered at the right edge, and room beneath for a full-width control. Each
/// entry draws its own share of the run's surface. See [`StackPosition`].
#[derive(IntoElement)]
pub struct SettingEntry {
    title: SharedString,
    description: Option<SharedString>,
    title_icon: Option<Icon>,
    title_actions: Option<AnyElement>,
    control: Option<AnyElement>,
    disabled: bool,
    position: StackPosition,
    children: Vec<AnyElement>,
    on_click: Option<(ElementId, EntryClick)>,
}

impl SettingEntry {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            title_icon: None,
            title_actions: None,
            control: None,
            disabled: false,
            position: StackPosition::Middle,
            children: Vec::new(),
            on_click: None,
        }
    }

    /// A short line under the title: live state, a unit or range the control
    /// does not show, or what a terse title means. Most rows need none.
    #[must_use]
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Make the whole row a pressable link, like a settings index row.
    #[must_use]
    pub fn on_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some((id.into(), Rc::new(handler)));
        self
    }

    /// A state glyph drawn before the title; see [`SettingCopy::title_icon`].
    #[must_use]
    pub fn title_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.title_icon = Some(icon.into());
        self
    }

    /// Which end of its run the entry sits on. [`SettingsStack`] sets this;
    /// call it directly only on a virtualized page.
    #[must_use]
    pub fn position(mut self, position: StackPosition) -> Self {
        self.position = position;
        self
    }

    /// Widgets that sit beside the title rather than with the control.
    #[must_use]
    pub fn title_actions(mut self, actions: impl IntoElement) -> Self {
        self.title_actions = Some(actions.into_any_element());
        self
    }

    /// **Must be bounded.** The control keeps its natural width while the copy
    /// column shrinks, so one that grows with its content squeezes the copy to
    /// a character per line. Give it a width, or a menu trigger.
    #[must_use]
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    /// Dim the row and make it inert, for a setting the current configuration
    /// has no effect on.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ParentElement for SettingEntry {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingEntry {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled;
        let position = self.position;
        let coarse = crate::touch::CoarsePointer::get(cx);
        let heading = if coarse {
            let description = self
                .description
                .map(|description| settings_description(description, cx));
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap(px(8.0))
                                .when_some(self.title_icon, |this, icon| {
                                    this.child(icon.with_size(crate::Size::Medium))
                                })
                                .child(div().text_size(crate::rems_from_px(13.0)).child(self.title))
                                .when_some(self.title_actions, zz_gpui::ParentElement::child),
                        )
                        .when_some(self.control, zz_gpui::ParentElement::child),
                )
                .children(description)
        } else {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    SettingCopy::new(self.title)
                        .when_some(self.description, SettingCopy::description)
                        .when_some(self.title_icon, SettingCopy::title_icon)
                        .when_some(self.title_actions, SettingCopy::title_actions),
                )
                .when_some(self.control, zz_gpui::ParentElement::child)
        };

        let theme = cx.theme();
        let card = theme.layer(roles::SETTINGS_CARD, PaintState::Rest);
        let (inset, cast): (Vec<_>, Vec<_>) =
            card.shadow.iter().cloned().partition(|shadow| shadow.inset);
        let radius = card.radius;
        let ends = move |this: zz_gpui::Div| {
            this.when(position.rounds_top(), |this| this.rounded_t(radius))
                .when(position.rounds_bottom(), |this| this.rounded_b(radius))
        };

        let body = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(10.0))
            .px(px(SETTINGS_STACK_PADDING))
            .py(px(11.0))
            .when(disabled, |this| {
                this.opacity(0.5)
                    .child(div().absolute().inset_0().occlude())
            })
            .child(heading)
            .children(self.children);

        let press = self.on_click.as_ref().map(|(id, _)| {
            let corner = |rounds: bool| if rounds { radius } else { px(0.0) };
            crate::touch::press_highlight(
                ElementId::from((id.clone(), "press")),
                None,
                crate::touch::PressCorners::Each(Corners {
                    top_left: corner(position.rounds_top()),
                    top_right: corner(position.rounds_top()),
                    bottom_left: corner(position.rounds_bottom()),
                    bottom_right: corner(position.rounds_bottom()),
                }),
            )
            .inset_0()
        });
        let surface = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .layer(&Layer {
                shadow: inset,
                radius: px(0.0),
                ..card
            })
            .map(ends)
            .when(!position.rounds_top(), zz_gpui::Styled::border_t_0)
            .when(!position.rounds_bottom(), zz_gpui::Styled::border_b_0)
            .children(press)
            .when(position.rules_above(), |this| {
                this.child(
                    div().flex_none().px(px(SETTINGS_STACK_PADDING)).child(
                        Separator::horizontal()
                            .surface_role(roles::LIST_RULE)
                            .h(RULE_WIDTH),
                    ),
                )
            })
            .child(body);

        let entry = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .when(!cast.is_empty(), |this| {
                let extent = px(8.0);
                this.child(
                    div()
                        .absolute()
                        .left(-extent)
                        .right(-extent)
                        .top(if position.rounds_top() {
                            -extent
                        } else {
                            px(0.0)
                        })
                        .bottom(if position.rounds_bottom() {
                            -extent
                        } else {
                            px(0.0)
                        })
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .left(extent)
                                .right(extent)
                                .top(if position.rounds_top() {
                                    extent
                                } else {
                                    -extent
                                })
                                .bottom(if position.rounds_bottom() {
                                    extent
                                } else {
                                    -extent
                                })
                                .map(ends)
                                .shadow(cast),
                        ),
                )
            })
            .child(surface);
        match self.on_click.filter(|_| !disabled) {
            Some((id, handler)) => entry
                .id(id)
                .cursor_pointer()
                .on_click(move |event, window, cx| handler(event, window, cx))
                .into_any_element(),
            None => entry.into_any_element(),
        }
    }
}

#[derive(IntoElement)]
pub struct SettingCopy {
    title: SharedString,
    description: Option<SharedString>,
    title_icon: Option<Icon>,
    title_actions: Option<AnyElement>,
}

impl SettingCopy {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            title_icon: None,
            title_actions: None,
        }
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A state glyph drawn before the title, sized to the title's line. Color
    /// it before passing.
    #[must_use]
    pub fn title_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.title_icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn title_actions(mut self, actions: impl IntoElement) -> Self {
        self.title_actions = Some(actions.into_any_element());
        self
    }
}

impl RenderOnce for SettingCopy {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(relative(0.7))
            .min_w_0()
            .gap(px(2.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .when_some(self.title_icon, |this, icon| {
                        this.child(icon.with_size(crate::Size::Medium))
                    })
                    .child(div().text_size(crate::rems_from_px(13.0)).child(self.title))
                    .when_some(self.title_actions, zz_gpui::ParentElement::child),
            )
            .when_some(self.description, |this, description| {
                this.child(settings_description(description, cx))
            })
    }
}

/// The ground of a settings card, for the controls mounted on it, so their
/// fills step up from the card instead of dissolving into it.
#[must_use]
pub fn settings_card_ground(cx: &App) -> Ground {
    cx.theme().ground_of(roles::SETTINGS_CARD)
}

/// A preview well in Settings, around `fill`, the color of what it shows:
/// [`roles::SETTINGS_PREVIEW`]'s edge, corners and shadow, clipping what it
/// holds.
#[must_use]
pub fn settings_preview(fill: Hsla, cx: &App) -> zz_gpui::Div {
    let layer = cx.theme().layer(roles::SETTINGS_PREVIEW, PaintState::Rest);
    div().overflow_hidden().layer(&Layer {
        fill,
        glass: None,
        ..layer
    })
}

pub fn settings_provenance_badge(label: impl Into<SharedString>) -> Badge {
    Badge::new(label).emphasis(Emphasis::Outline)
}

#[derive(Clone)]
pub struct SettingsSelectItem {
    title: SharedString,
    value: String,
    font: bool,
}

impl SettingsSelectItem {
    pub fn new(title: impl Into<SharedString>, value: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            value: value.into(),
            font: false,
        }
    }

    /// Draw the row in the font family its value names.
    #[must_use]
    pub fn preview_font(mut self) -> Self {
        self.font = true;
        self
    }
}

impl SelectItem for SettingsSelectItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn font_family(&self) -> Option<SharedString> {
        self.font.then(|| self.value.clone().into())
    }
}

pub fn panes_page(
    preview: panes_preview::PanesPreview,
    layout: impl IntoIterator<Item = SettingEntry>,
    pip: [SettingEntry; 1],
    focus: [SettingEntry; 2],
    frame: [SettingEntry; 1],
    cx: &App,
) -> zz_gpui::Div {
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
