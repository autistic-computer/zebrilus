pub mod about;
pub mod appearance;

use std::{cell::Cell, rc::Rc};

use web_time::Duration;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Ground, IconName, Layer,
    PaintState, RULE_WIDTH, RuleLook, Selectable as _, Sizable as _, StyledExt as _, Surface,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    icon::Icon,
    roles,
    scroll::ScrollableElement as _,
    select::SelectItem,
    separator::Separator,
};
use ala::{
    AnyElement, App, Bounds, Corners, ElementId, FocusHandle, Hsla, IntoElement, ListAlignment,
    ListSizingBehavior, ListState, ParentElement, Pixels, RenderOnce, ScrollHandle, SharedString,
    Styled as _, Window, div, list, prelude::*, px, relative,
};

type EntryClick = Rc<dyn Fn(&ala::ClickEvent, &mut Window, &mut App)>;

pub const SETTINGS_NAVIGATION_ROW_HEIGHT: f32 = 32.0;

pub fn settings_navigation_button(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    icon: impl Into<Icon>,
    selected: bool,
    cx: &App,
) -> Button {
    Button::new(id)
        .w_full()
        .px(px(8.0))
        .ghost()
        .surface_role(roles::SIDEBAR_ROW)
        .on(cx.theme().ground_of(Surface::new(roles::SIDEBAR)))
        .icon(icon)
        .selected(selected)
        .label(title)
        .child(div().flex_1())
        .when(selected, crate::StyledExt::font_medium)
}

pub fn settings_navigation_back_button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .w_full()
        .px(px(8.0))
        .ghost()
        .surface_role(roles::SIDEBAR_ROW)
        .icon(IconName::ArrowLeft)
        .label("Back")
        .child(div().flex_1())
}

pub fn settings_navigation_row(button: Button) -> ala::Div {
    div()
        .flex()
        .flex_none()
        .h(px(SETTINGS_NAVIGATION_ROW_HEIGHT))
        .items_center()
        .child(button)
}

pub fn settings_navigation_group_label(title: impl Into<SharedString>, cx: &App) -> ala::Div {
    div()
        .px(px(8.0))
        .pt(px(12.0))
        .pb(px(2.0))
        .text_sm()
        .text_color(cx.theme().foreground.muted())
        .map(crate::StyledExt::font_medium)
        .child(title.into())
}

pub fn settings_navigation_group_gap() -> ala::Div {
    div().flex_none().h(px(12.0))
}

#[derive(Clone, Debug)]
pub struct NavSection {
    pub title: SharedString,
    pub icon: IconName,
    pub group: usize,
}

type NavPick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[must_use]
pub fn settings_navigation(
    id: impl Into<ElementId>,
    sections: impl IntoIterator<Item = NavSection>,
    selected: Option<usize>,
) -> SettingsNavigation {
    SettingsNavigation {
        id: id.into(),
        sections: sections.into_iter().collect(),
        selected,
        on_pick: None,
        on_back: None,
    }
}

#[derive(IntoElement)]
pub struct SettingsNavigation {
    id: ElementId,
    sections: Vec<NavSection>,
    selected: Option<usize>,
    on_pick: Option<NavPick>,
    on_back: Option<EntryClick>,
}

impl SettingsNavigation {
    #[must_use]
    pub fn on_pick(mut self, on_pick: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_pick = Some(Rc::new(on_pick));
        self
    }

    #[must_use]
    pub fn on_back(
        mut self,
        on_back: impl Fn(&ala::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_back = Some(Rc::new(on_back));
        self
    }
}

impl RenderOnce for SettingsNavigation {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let back = self.on_back.map(|on_back| {
            settings_navigation_row(
                settings_navigation_back_button("back")
                    .on_click(move |event, window, cx| on_back(event, window, cx)),
            )
        });
        let mut rows = Vec::with_capacity(self.sections.len() * 2);
        let mut group = None;
        for (index, section) in self.sections.into_iter().enumerate() {
            if group.is_some_and(|group| group != section.group) {
                rows.push(settings_navigation_group_gap().into_any_element());
            }
            group = Some(section.group);
            let on_pick = self.on_pick.clone();
            rows.push(
                settings_navigation_button(
                    ("penna-settings-section", index),
                    section.title,
                    section.icon,
                    self.selected == Some(index),
                    cx,
                )
                .when_some(on_pick, |button, on_pick| {
                    button.on_click(move |_, window, cx| on_pick(index, window, cx))
                })
                .into_any_element(),
            );
        }
        div()
            .id(self.id)
            .flex()
            .flex_col()
            .w_full()
            .list_inset(cx)
            .children(back)
            .children(rows)
    }
}

const SETTINGS_CONTENT_MAX_WIDTH: f32 = 760.0;
pub const SETTINGS_PAGE_PADDING: f32 = 14.0;

pub fn settings_page_content() -> ala::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .max_w(px(SETTINGS_CONTENT_MAX_WIDTH))
        .mx_auto()
}

#[derive(Clone, Debug, Default)]
pub struct SettingsFold(Rc<Cell<f32>>);

impl SettingsFold {
    #[must_use]
    pub fn scrolled(&self) -> f32 {
        self.0.get()
    }
}

pub const LARGE_TITLE_HEIGHT: f32 = 52.0;

pub fn large_title(title: impl Into<SharedString>, cx: &App) -> ala::Div {
    div()
        .debug_selector(|| "large-title".to_owned())
        .flex()
        .flex_none()
        .items_end()
        .h(px(LARGE_TITLE_HEIGHT))
        .px(px(16.0))
        .pb(px(4.0))
        .text_size(px(34.0))
        .line_height(px(41.0))
        .font_bold()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.into()),
        )
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

fn settings_large_title(title: SharedString, cx: &App) -> ala::Div {
    large_title(title, cx)
        .px(px(2.0))
        .mt(px(-SETTINGS_PAGE_PADDING))
}

pub fn settings_page_heading(title: impl Into<SharedString>, cx: &App) -> ala::Div {
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SettingsBottomInset(pub Pixels);

impl ala::Global for SettingsBottomInset {}

impl SettingsBottomInset {
    #[must_use]
    pub fn get(cx: &App) -> Pixels {
        cx.try_global::<Self>().map_or(px(0.0), |inset| inset.0)
    }
}

#[must_use]
pub fn settings_scroll_column(id: &'static str) -> SettingsScrollColumn {
    SettingsScrollColumn {
        id,
        children: Vec::new(),
        large_title: None,
    }
}

#[derive(IntoElement)]
pub struct SettingsScrollColumn {
    id: &'static str,
    children: Vec<AnyElement>,
    large_title: Option<(SharedString, SettingsFold)>,
}

impl SettingsScrollColumn {
    #[must_use]
    pub fn large_title(mut self, title: impl Into<SharedString>, fold: &SettingsFold) -> Self {
        self.large_title = Some((title.into(), fold.clone()));
        self
    }
}

impl ala::ParentElement for SettingsScrollColumn {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsScrollColumn {
    fn render(self, window: &mut ala::Window, cx: &mut App) -> impl IntoElement {
        let handle = window
            .use_keyed_state(ala::ElementId::Name(self.id.into()), cx, |_, _| {
                ala::ScrollHandle::default()
            })
            .read(cx)
            .clone();
        let title = self.large_title.map(|(title, fold)| {
            fold.0.set((-f32::from(handle.offset().y)).max(0.0));
            settings_large_title(title, cx)
        });
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
                            .children(title)
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
        large_title: None,
    }
}

#[derive(IntoElement)]
pub struct SettingsVirtualColumn {
    id: &'static str,
    item_count: usize,
    render_item: SettingsItemRenderer,
    row_focus: Vec<Option<FocusHandle>>,
    large_title: Option<(SharedString, SettingsFold)>,
}

impl SettingsVirtualColumn {
    #[must_use]
    pub fn row_focus(mut self, handles: impl IntoIterator<Item = Option<FocusHandle>>) -> Self {
        self.row_focus = handles.into_iter().collect();
        self
    }

    #[must_use]
    pub fn large_title(mut self, title: impl Into<SharedString>, fold: &SettingsFold) -> Self {
        self.large_title = Some((title.into(), fold.clone()));
        self
    }
}

impl RenderOnce for SettingsVirtualColumn {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state_key = ElementId::Name(format!("{}-list-state", self.id).into());
        let lead = usize::from(self.large_title.is_some());
        let mut focus = vec![None; lead];
        focus.extend(self.row_focus);
        focus.resize(self.item_count + lead, None);
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

        let title = self.large_title.map(|(title, fold)| {
            let top = list_state.logical_scroll_top();
            fold.0.set(if top.item_ix == 0 {
                f32::from(top.offset_in_item)
            } else {
                f32::INFINITY
            });
            title
        });
        let mut render_item = self.render_item;
        let rows = list(list_state.clone(), move |index, window, cx| {
            let row = match (&title, index.checked_sub(lead)) {
                (_, Some(index)) => render_item(index, window, cx),
                (Some(title), None) => settings_large_title(title.clone(), cx).into_any_element(),
                (None, None) => div().into_any_element(),
            };
            div()
                .flex()
                .w_full()
                .px(px(SETTINGS_PAGE_PADDING))
                .child(settings_page_content().flex_none().child(row))
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

pub const SETTINGS_GROUP_TITLE_SIZE: f32 = 13.0;
pub const SETTINGS_GROUP_TITLE_LINE: f32 = 20.0;
const SETTINGS_DESCRIPTION_SIZE: f32 = 12.0;

pub fn settings_group_header(title: SharedString, cx: &App) -> ala::Div {
    crate::StyledExt::font_semibold(
        div()
            .px(px(2.0))
            .text_size(crate::rems_from_px(SETTINGS_GROUP_TITLE_SIZE))
            .line_height(px(SETTINGS_GROUP_TITLE_LINE))
            .text_color(cx.theme().foreground.muted()),
    )
    .child(title)
}

pub fn settings_description(text: SharedString, cx: &App) -> ala::Div {
    div()
        .text_size(crate::rems_from_px(SETTINGS_DESCRIPTION_SIZE))
        .text_color(cx.theme().foreground.muted())
        .child(text)
}

pub fn settings_list_group_header(title: &'static str, cx: &App) -> ala::Div {
    div()
        .pt(px(10.0))
        .child(settings_group_header(title.into(), cx))
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
            .layer(Surface::new(roles::SETTINGS_CARD), PaintState::Rest)
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
                            .top(crate::icon::cap_center_drop(
                                px(SETTINGS_GROUP_TITLE_SIZE),
                                px(SETTINGS_GROUP_TITLE_SIZE),
                                cx,
                            ))
                            .child(
                                crate::motion::Disclosure::new(
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
    let turned = crate::motion::turned(id, expanded, cx);
    if !expanded || turned.is_none() {
        return row;
    }
    let delay = REVEAL_STAGGER * u32::try_from(order).unwrap_or(u32::MAX).min(12);
    let shown = crate::motion::eased(turned, delay, REVEAL, window, cx);
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

pub const SETTINGS_STACK_PADDING: f32 = 12.0;

#[derive(IntoElement)]
pub struct SettingsStack {
    title: Option<SharedString>,
    note: Option<SharedString>,
    entries: Vec<SettingEntry>,
}

impl SettingsStack {
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

    #[must_use]
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

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
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StackPosition {
    Only,
    First,
    #[default]
    Middle,
    Last,
}

impl StackPosition {
    #[must_use]
    pub const fn new(is_first: bool, is_last: bool) -> Self {
        match (is_first, is_last) {
            (true, true) => Self::Only,
            (true, false) => Self::First,
            (false, true) => Self::Last,
            (false, false) => Self::Middle,
        }
    }

    #[must_use]
    pub const fn at(index: usize, last: usize) -> Self {
        Self::new(index == 0, index == last)
    }

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

    #[must_use]
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ala::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some((id.into(), Rc::new(handler)));
        self
    }

    #[must_use]
    pub fn title_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.title_icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn position(mut self, position: StackPosition) -> Self {
        self.position = position;
        self
    }

    #[must_use]
    pub fn title_actions(mut self, actions: impl IntoElement) -> Self {
        self.title_actions = Some(actions.into_any_element());
        self
    }

    #[must_use]
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

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
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
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
                                .when_some(self.title_actions, ala::ParentElement::child),
                        )
                        .when_some(self.control, ala::ParentElement::child),
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
                .when_some(self.control, ala::ParentElement::child)
        };

        let theme = cx.theme();
        let card = theme.layer(Surface::new(roles::SETTINGS_CARD), PaintState::Rest);
        let (inset, cast): (Vec<_>, Vec<_>) =
            card.shadow.iter().cloned().partition(|shadow| shadow.inset);
        let radius = card.radius;
        let ends = move |this: ala::Div| {
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
            .when(!position.rounds_top(), ala::Styled::border_t_0)
            .when(!position.rounds_bottom(), ala::Styled::border_b_0)
            .children(press)
            .when(position.rules_above(), |this| {
                this.child(
                    div()
                        .flex_none()
                        .px(px(SETTINGS_STACK_PADDING))
                        .child(Separator::horizontal().look(RuleLook::List).h(RULE_WIDTH)),
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
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
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
                    .when_some(self.title_actions, ala::ParentElement::child),
            )
            .when_some(self.description, |this, description| {
                this.child(settings_description(description, cx))
            })
    }
}

#[must_use]
pub fn settings_card_ground(cx: &App) -> Ground {
    cx.theme().ground_of(Surface::new(roles::SETTINGS_CARD))
}

#[must_use]
pub fn settings_preview(fill: Hsla, cx: &App) -> ala::Div {
    let layer = cx
        .theme()
        .layer(Surface::new(roles::SETTINGS_PREVIEW), PaintState::Rest);
    div().overflow_hidden().layer(&Layer {
        fill,
        glass: None,
        ..layer
    })
}

pub fn settings_provenance_badge(label: impl Into<SharedString>) -> Badge {
    Badge::new(label).emphasis(Emphasis::Outline)
}

pub fn settings_reset_button(
    id: impl Into<ala::ElementId>,
    tooltip: impl Into<SharedString>,
    enabled: bool,
) -> Button {
    let slop = crate::touch::control_slop(crate::Size::Small);
    Button::new(id)
        .small()
        .compact()
        .ghost()
        .surface_role(crate::SurfaceRole::FLAT_ICON_BUTTON)
        .hit_slop(slop, slop)
        .icon(crate::IconName::Undo2)
        .tooltip(tooltip)
        .disabled(!enabled)
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

#[cfg(test)]
mod scroll_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Context, Render, TestAppContext, VisualTestContext};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

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
        input: ala::Entity<crate::input::InputState>,
        virtual_rows: bool,
    }

    impl Render for RevealTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let input = self.input.clone();
            let focus = ala::Focusable::focus_handle(self.input.read(cx), cx);
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
            let focus = ala::Focusable::focus_handle(view.read(cx).input.read(cx), cx);
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

    #[ala::test]
    fn a_focused_field_scrolls_into_a_list_page(cx: &mut TestAppContext) {
        let field = reveal_field(cx, true);
        assert!(field.top() >= px(0.0) && field.bottom() <= px(220.0));
    }

    #[ala::test]
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

    #[ala::test]
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

        cx.simulate_event(ala::ScrollWheelEvent {
            position: ala::point(px(100.0), px(100.0)),
            delta: ala::ScrollDelta::Pixels(ala::point(px(0.0), px(-100_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("virtual-settings-row-99").is_some());
        assert!(cx.debug_bounds("virtual-settings-row-0").is_none());
    }

    #[ala::test]
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

    #[ala::test]
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
        cx.simulate_event(ala::ScrollWheelEvent {
            position: ala::point(px(100.0), px(100.0)),
            delta: ala::ScrollDelta::Pixels(ala::point(px(0.0), px(-10_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let last = cx.debug_bounds("inset-row-9").expect("last row");
        assert!(last.bottom() <= px(220.0 - 40.0));
    }

    #[ala::test]
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
