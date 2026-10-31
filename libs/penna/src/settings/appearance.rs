use super::{SettingEntry, SettingsSelectItem, StackPosition};
use crate::button::{Button, ButtonVariants as _};
use crate::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use crate::interface_theme::{
    ACCENTS, Appearance, AppearanceChange, DEFAULT_TINT_HUE, InterfaceTheme, MAX_CORNER_RADIUS,
    SavedThemes, ThemeOverrides,
};
use crate::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use crate::segmented::SegmentedControl;
use crate::select::{Select, SelectEvent, SelectItem as _, SelectState};
use crate::slider::Slider;
use crate::switch::Switch;
use crate::{ActiveTheme as _, Colorize as _, IconName, StyledExt as _, ThemeColor, ThemeMode};
use ala::{
    Anchor, AnyElement, App, Context, Entity, FocusHandle, IntoElement, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use std::{cell::RefCell, convert::Infallible, rc::Rc, sync::Arc};

const THEME_PREVIEW_WIDTH: f32 = 84.0;
const THEME_PREVIEW_HEIGHT: f32 = 56.0;
const THEME_PREVIEW_SIDEBAR_WIDTH: f32 = 20.0;

const PAGE_ROW_GAP: f32 = 8.0;
const TILE_FRAME_PADDING: f32 = 2.0;
const TILE_RING: f32 = 2.0;
const TILE_HALO_GAP: f32 = 1.0;
const TILE_HALO: f32 = TILE_RING + TILE_HALO_GAP + 1.0;

pub fn run_position<T: Copy>(
    items: &[T],
    index: usize,
    is_entry: impl Fn(T) -> bool,
) -> StackPosition {
    let bounded = |neighbour: Option<usize>| {
        neighbour.is_none_or(|at| items.get(at).copied().is_none_or(|it| !is_entry(it)))
    };
    StackPosition::new(bounded(index.checked_sub(1)), bounded(Some(index + 1)))
}

pub fn page_row(element: AnyElement, ends_run: bool) -> AnyElement {
    div()
        .w_full()
        .pb(px(if ends_run { PAGE_ROW_GAP } else { 0.0 }))
        .child(element)
        .into_any_element()
}

pub const ADVANCED_DISCLOSURE: &str = "settings-appearance-advanced";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppearancePageItem<T = Infallible> {
    Title,
    Group { title: &'static str },
    ThemeMode,
    Accent,
    TintHue,
    Tint,
    Themes,
    Advanced { expanded: bool },
    CornerRadius,
    ChromeContrast,
    UiFontFamily,
    UiZoom,
    Animations,
    AnimationSpeed,
    App(T),
}

impl<T: Copy> AppearancePageItem<T> {
    pub const fn is_entry(self) -> bool {
        !matches!(
            self,
            Self::Title | Self::Group { .. } | Self::Advanced { .. }
        )
    }

    pub const fn title(self) -> Option<&'static str> {
        Some(match self {
            Self::ThemeMode => "Mode",
            Self::Accent => "Accent",
            Self::TintHue => "Tint",
            Self::Tint => "Tint strength",
            Self::Themes => "Theme",
            Self::CornerRadius => "Corner radius",
            Self::ChromeContrast => "Contrast",
            Self::AnimationSpeed => "Animation speed",
            Self::UiFontFamily => "UI font",
            Self::UiZoom => "UI zoom",
            Self::Animations => "Animations",
            Self::Title | Self::Group { .. } | Self::Advanced { .. } | Self::App(_) => return None,
        })
    }
}

pub fn appearance_page_items<T>(advanced: bool) -> Vec<AppearancePageItem<T>> {
    let mut items = vec![
        AppearancePageItem::Title,
        AppearancePageItem::Group { title: "Theme" },
        AppearancePageItem::ThemeMode,
        AppearancePageItem::Themes,
        AppearancePageItem::Accent,
        AppearancePageItem::TintHue,
        AppearancePageItem::Tint,
        AppearancePageItem::Advanced { expanded: advanced },
    ];
    if !advanced {
        return items;
    }
    items.extend([
        AppearancePageItem::CornerRadius,
        AppearancePageItem::ChromeContrast,
        AppearancePageItem::UiFontFamily,
        AppearancePageItem::UiZoom,
        AppearancePageItem::Animations,
        AppearancePageItem::AnimationSpeed,
    ]);
    items
}

pub struct ThemePicks<'a> {
    pub themes: Vec<(SharedString, &'a InterfaceTheme)>,
    pub selected: usize,
    pub overrides: &'a ThemeOverrides,
}

impl ThemePicks<'_> {
    fn base(&self) -> &InterfaceTheme {
        self.themes.get(self.selected).map_or_else(
            || crate::interface_theme::default_theme(),
            |(_, theme)| *theme,
        )
    }

    pub fn composed(&self, change: impl FnOnce(&mut ThemeOverrides)) -> InterfaceTheme {
        let mut overrides = self.overrides.clone();
        change(&mut overrides);
        let mut theme = self.base().clone();
        overrides.apply_to(&mut theme);
        theme
    }
}

pub const THEME_MODES: [(Option<ThemeMode>, &str); 3] = [
    (None, "System"),
    (Some(ThemeMode::Light), "Light"),
    (Some(ThemeMode::Dark), "Dark"),
];

pub fn theme_mode_control(
    selected: Option<ThemeMode>,
    on_pick: impl Fn(Option<ThemeMode>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SegmentedControl {
    SegmentedControl::new("settings-theme-mode", THEME_MODES.map(|(_, title)| title))
        .selected(
            THEME_MODES
                .iter()
                .position(|(mode, _)| *mode == selected)
                .unwrap_or(0),
        )
        .on(super::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(THEME_MODES[index].0, window, cx))
}

pub fn theme_labels(picks: &ThemePicks) -> Vec<SharedString> {
    let names = picks
        .themes
        .iter()
        .map(|(id, theme)| {
            if theme.name.is_empty() {
                id.to_string()
            } else {
                theme.name.clone()
            }
        })
        .collect::<Vec<_>>();
    names
        .iter()
        .zip(&picks.themes)
        .enumerate()
        .map(|(index, (name, (id, _)))| {
            if names[..index].contains(name) {
                format!("{name} ({id})").into()
            } else {
                name.clone().into()
            }
        })
        .collect()
}

const ACCENT_DOT: f32 = 10.0;

fn accent_label(accent: ala::Hsla, label: SharedString, cx: &App) -> ala::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .flex_none()
                .size(px(ACCENT_DOT))
                .rounded(cx.theme().swatch_radius().min(px(ACCENT_DOT / 2.0)))
                .bg(accent),
        )
        .child(
            div()
                .flex_none()
                .line_height(ala::relative(1.0))
                .child(label),
        )
}

type ThemeMenu = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

pub fn theme_picker(
    picks: &ThemePicks,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    more: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    cx: &App,
) -> ala::Div {
    picker(picks, on_pick, Some(Rc::new(more)), cx)
}

fn picker(
    picks: &ThemePicks,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    more: Option<ThemeMenu>,
    cx: &App,
) -> ala::Div {
    let theme_mode = cx.theme().mode;
    let live = cx.theme().accent;
    let selected = picks.selected;
    let themes = theme_labels(picks)
        .into_iter()
        .zip(&picks.themes)
        .enumerate()
        .map(|(index, (label, (_, theme)))| {
            let accent = if index == selected {
                live
            } else {
                theme.colors(theme_mode).accent
            };
            (label, accent)
        })
        .collect::<Vec<_>>();
    let current = themes
        .get(selected)
        .map(|(label, _)| label.clone())
        .unwrap_or_default();
    let on_pick = Rc::new(on_pick);
    let ground = super::settings_card_ground(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .child(
            Button::new("settings-theme")
                .child(accent_label(live, current, cx))
                .dropdown_caret(true)
                .on(ground)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    themes
                        .iter()
                        .enumerate()
                        .fold(menu, |menu, (index, (label, accent))| {
                            let on_pick = Rc::clone(&on_pick);
                            let (label, accent) = (label.clone(), *accent);
                            menu.item(
                                PopupMenuItem::element(move |_, _, cx| {
                                    accent_label(accent, label.clone(), cx)
                                })
                                .checked(index == selected)
                                .on_click(move |_, window, cx| on_pick(index, window, cx)),
                            )
                        })
                }),
        )
        .children(more.map(|more| {
            Button::new("settings-theme-more")
                .ghost()
                .icon(IconName::Ellipsis)
                .tooltip("More")
                .on(ground)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                    more(menu, window, cx)
                })
        }))
}

pub const SWATCH: f32 = 18.0;
const SHEET_SWATCH: f32 = 30.0;
const SWATCH_RING_GAP: f32 = 2.0;
const SWATCH_RING: f32 = 1.5;

#[allow(
    clippy::disallowed_methods,
    reason = "a swatch shows the accent it picks, not the live one"
)]
pub fn accent_swatches(
    picks: &ThemePicks,
    mode: ThemeMode,
    custom: crate::color_picker::ColorPicker,
    on_pick: impl Fn(Option<&'static str>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    let selected = picks.overrides.accent.as_deref().unwrap_or("theme");
    let own = picks
        .composed(|overrides| overrides.accent = None)
        .colors(mode)
        .accent;
    let on_pick = Rc::new(on_pick);
    let ring = cx.theme().foreground;
    let radius = cx.theme().swatch_radius();
    let coarse = crate::touch::CoarsePointer::get(cx);
    let size = if coarse { SHEET_SWATCH } else { SWATCH };
    let swatch = |id: &'static str, name: SharedString, color: ala::Hsla| {
        let on_pick = Rc::clone(&on_pick);
        ringed(
            selected == id,
            ring,
            radius,
            div()
                .id(SharedString::from(format!("settings-accent-{id}")))
                .role(ala::Role::Button)
                .aria_label(name.clone())
                .size(px(size))
                .rounded(radius)
                .bg(color)
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    crate::tooltip::Tooltip::new(name.clone()).build(window, cx)
                })
                .on_click(move |_, window, cx| {
                    on_pick((id != "theme").then_some(id), window, cx);
                }),
        )
    };
    let named = ACCENTS.iter().filter_map(|name| {
        let color = crate::interface_theme::accent_color(name, mode)?;
        Some(swatch(name, accent_title(name), color))
    });
    let row = div()
        .flex()
        .flex_none()
        .flex_wrap()
        .items_center()
        .when(coarse, |row| row.w_full().justify_between())
        .gap(px(4.0))
        .child(swatch("theme", "Theme".into(), own))
        .children(named);
    if coarse {
        let chosen = match selected {
            "theme" => "Theme".into(),
            name if ACCENTS.contains(&name) => accent_title(name),
            hex => hex.to_uppercase().into(),
        };
        let color = crate::interface_theme::accent_color(selected, mode).unwrap_or(own);
        return div().w_full().child(custom.presets(chosen, color, row));
    }
    let custom_selected = selected != "theme" && !ACCENTS.contains(&selected);
    row.child(ringed(custom_selected, ring, radius, custom))
}

fn accent_title(name: &str) -> SharedString {
    let mut title = name.to_string();
    title[..1].make_ascii_uppercase();
    title.into()
}

#[allow(
    clippy::disallowed_methods,
    reason = "the chosen swatch's ring, the same width in every theme"
)]
fn ringed(
    selected: bool,
    ring: ala::Hsla,
    radius: ala::Pixels,
    swatch: impl IntoElement,
) -> ala::Div {
    let inset = px(SWATCH_RING_GAP + SWATCH_RING);
    div()
        .flex_none()
        .p(px(SWATCH_RING_GAP))
        .rounded(if radius > px(0.0) {
            radius + inset
        } else {
            px(0.0)
        })
        .border(px(SWATCH_RING))
        .border_color(if selected {
            ring
        } else {
            ala::transparent_black()
        })
        .child(swatch)
}

const SLIDER_WIDTH: f32 = 180.0;
const HUE_STOPS: u16 = 12;

pub fn tint_hue_slider(
    hue: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    let stops =
        (0..=HUE_STOPS).map(|stop| hue_swatch(f32::from(stop) * 360.0 / f32::from(HUE_STOPS)));
    let slider = Slider::new("settings-tint-hue", "Tint hue", hue, 0.0..=360.0, on_change)
        .step(1.0)
        .track(stops)
        .display(|hue| format!("{hue:.0}°").into());
    let dot = div()
        .flex_none()
        .size(px(SWATCH))
        .rounded_full()
        .bg(hue_swatch(hue));
    if crate::touch::CoarsePointer::get(cx) {
        return div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().flex_1().min_w_0().child(slider))
            .child(dot);
    }
    slider_row(slider, dot)
}

pub fn tint_slider(
    tint: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new("settings-tint", "Tint strength", tint, 0.0..=1.0, on_change)
            .step(0.01)
            .display(|tint| format!("{:.0}%", tint * 100.0).into()),
        cx,
    )
}

pub fn corner_radius_slider(
    radius: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new(
            "settings-corner-radius",
            "Corner radius",
            radius,
            0.0..=MAX_CORNER_RADIUS,
            on_change,
        )
        .step(1.0)
        .display(|radius| format!("{radius:.0}px").into()),
        cx,
    )
}

pub fn contrast_slider(
    contrast: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new(
            "settings-contrast",
            "Contrast",
            contrast,
            1.0..=2.0,
            on_change,
        )
        .step(0.05)
        .display(|contrast| format!("{:.0}%", contrast * 100.0).into()),
        cx,
    )
}

pub fn animation_speed_slider(
    speed: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new(
            "settings-animation-speed",
            "Animation speed",
            speed,
            crate::motion::MIN_ANIMATION_SPEED..=crate::motion::MAX_ANIMATION_SPEED,
            on_change,
        )
        .step(0.05)
        .display(|speed| format!("{:.0}%", speed * 100.0).into()),
        cx,
    )
}

pub fn percent_slider(
    id: &'static str,
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(percent(id, label, value, on_change), cx)
}

pub fn zoom_slider(
    value: f32,
    focus: &FocusHandle,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        percent("settings-zoom", "UI zoom", value, on_change)
            .commit_on_release()
            .track_focus(focus),
        cx,
    )
}

fn percent(
    id: &'static str,
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> Slider {
    Slider::new(id, label, value, 0.5..=3.0, on_change)
        .step(0.05)
        .display(|value| format!("{:.0}%", value * 100.0).into())
}

pub fn value_slider(slider: Slider, cx: &App) -> ala::Div {
    if crate::touch::CoarsePointer::get(cx) {
        return div().w_full().child(slider);
    }
    let shown = div()
        .text_size(crate::rems_from_px(12.0))
        .text_color(cx.theme().foreground.muted())
        .child(slider.shown());
    slider_row(slider, shown)
}

fn slider_row(slider: Slider, trailing: impl IntoElement) -> ala::Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(div().w(px(SLIDER_WIDTH)).child(slider))
        .child(div().flex_none().w(px(SWATCH * 2.0)).child(trailing))
}

#[allow(
    clippy::disallowed_methods,
    reason = "the hue picker shows every hue, not a theme color"
)]
fn hue_swatch(hue: f32) -> ala::Hsla {
    crate::oklch(0.72, 0.13, hue)
}

pub fn appearance_page<T: Copy + 'static>(
    title: impl Into<SharedString>,
    items: Vec<AppearancePageItem<T>>,
    mut render: impl FnMut(AppearancePageItem<T>, StackPosition, &mut Window, &mut App) -> AnyElement
    + 'static,
) -> super::SettingsVirtualColumn {
    let title = title.into();
    let advanced = items.iter().enumerate().find_map(|(at, item)| match item {
        AppearancePageItem::Advanced { expanded } => Some((at, *expanded)),
        _ => None,
    });
    super::settings_virtual_column(
        "settings-appearance",
        items.len(),
        move |index, window, cx| {
            let Some(item) = items.get(index).copied() else {
                return div().into_any_element();
            };
            let position = run_position(&items, index, AppearancePageItem::is_entry);
            let row = match item {
                AppearancePageItem::Title => {
                    super::settings_page_heading(title.clone(), cx).into_any_element()
                }
                AppearancePageItem::Group { title } => {
                    super::settings_list_group_header(title, cx).into_any_element()
                }
                _ => render(item, position, window, cx),
            };
            let row = page_row(row, !item.is_entry() || position.ends_run());
            match advanced {
                Some((at, expanded)) if index > at => super::settings_disclosure_reveal(
                    ADVANCED_DISCLOSURE,
                    expanded,
                    index - at - 1,
                    row,
                    window,
                    cx,
                ),
                _ => row,
            }
        },
    )
}

pub struct AvailableFonts(pub Arc<dyn ala::PlatformTextSystem>);

impl ala::Global for AvailableFonts {}

pub fn ui_font_select(
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SelectState<Vec<SettingsSelectItem>>> {
    let mut families = cx.try_global::<AvailableFonts>().map_or_else(
        || cx.text_system().all_font_names(),
        |fonts| fonts.0.all_font_names(),
    );
    families.retain(|family| !family.starts_with('.') && !family.trim().is_empty());
    families.sort_by_cached_key(|family| family.to_lowercase());
    families.dedup();
    let items: Vec<_> = std::iter::once(SettingsSelectItem::new("System default", ""))
        .chain(
            families
                .into_iter()
                .map(|family| SettingsSelectItem::new(family.clone(), family).preview_font()),
        )
        .collect();
    let selected = selected.unwrap_or_default();
    let index = items.iter().position(|item| item.value() == selected);
    cx.new(|cx| SelectState::new(items, index.map(crate::IndexPath::new), window, cx))
}

type OnChange = Rc<dyn Fn(AppearanceChange, &mut Window, &mut App)>;

pub struct AppearanceControls {
    accent: Entity<ColorPickerState>,
    ui_font: RefCell<Option<(Entity<SelectState<Vec<SettingsSelectItem>>>, Subscription)>>,
    zoom: FocusHandle,
    on_change: OnChange,
    theme_menu: Option<ThemeMenu>,
    _accent: Subscription,
}

impl AppearanceControls {
    pub fn new(
        appearance: &Appearance,
        on_change: impl Fn(AppearanceChange, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let on_change: OnChange = Rc::new(on_change);
        let accent = cx.new(|cx| ColorPickerState::new(custom_accent(appearance), window, cx));
        let changed = Rc::clone(&on_change);
        let subscription = window.subscribe(
            &accent,
            cx,
            move |_, event: &ColorPickerEvent, window, cx| {
                let ColorPickerEvent::Change(color) = event;
                let accent = color.map(|color| crate::to_hex(color.opaque()));
                changed(AppearanceChange::Accent(accent), window, cx);
            },
        );
        Self {
            accent,
            ui_font: RefCell::default(),
            zoom: cx.focus_handle(),
            on_change,
            theme_menu: None,
            _accent: subscription,
        }
    }

    #[must_use]
    pub fn theme_menu(
        mut self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.theme_menu = Some(Rc::new(menu));
        self
    }

    pub fn zoom_focus(&self) -> &FocusHandle {
        &self.zoom
    }

    pub fn sync(
        &self,
        appearance: &Appearance,
        saved: &SavedThemes,
        window: &mut Window,
        cx: &mut App,
    ) {
        let accent = custom_accent(appearance);
        self.accent
            .update(cx, |picker, cx| picker.set_color(accent, window, cx));
        if let Some((select, _)) = &*self.ui_font.borrow() {
            let font = appearance.ui_font(saved).to_owned();
            select.update(cx, |select, cx| {
                select.set_selected_value(&font, window, cx);
            });
        }
    }

    fn ui_font_select(
        &self,
        appearance: &Appearance,
        saved: &SavedThemes,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<SelectState<Vec<SettingsSelectItem>>> {
        if let Some((select, _)) = &*self.ui_font.borrow() {
            return select.clone();
        }
        let select = ui_font_select(Some(appearance.ui_font(saved)), window, cx);
        let changed = Rc::clone(&self.on_change);
        let subscription = window.subscribe(
            &select,
            cx,
            move |_, event: &SelectEvent<Vec<SettingsSelectItem>>, window, cx| {
                if let SelectEvent::Confirm(Some(family)) = event {
                    changed(AppearanceChange::UiFont(family.clone()), window, cx);
                }
            },
        );
        *self.ui_font.borrow_mut() = Some((select.clone(), subscription));
        select
    }

    fn change(
        &self,
        change: fn(f32) -> AppearanceChange,
    ) -> impl Fn(f32, &mut Window, &mut App) + use<> {
        let on_change = Rc::clone(&self.on_change);
        move |value, window, cx| on_change(change(value), window, cx)
    }
}

fn custom_accent(appearance: &Appearance) -> Option<ala::Hsla> {
    appearance
        .overrides
        .accent
        .as_deref()
        .filter(|accent| !ACCENTS.contains(accent))
        .and_then(|accent| crate::interface_theme::accent_color(accent, ThemeMode::Dark))
}

pub fn theme_picks<'a>(appearance: &'a Appearance, saved: &'a SavedThemes) -> ThemePicks<'a> {
    let themes = crate::interface_theme::builtin_themes()
        .iter()
        .map(|(id, theme)| (SharedString::from(*id), theme))
        .chain(saved.iter().map(|(id, theme)| (id.clone(), theme)))
        .collect::<Vec<_>>();
    let active = appearance.theme_id(saved);
    let selected = themes
        .iter()
        .position(|(id, _)| id.as_str() == active)
        .unwrap_or(0);
    ThemePicks {
        themes,
        selected,
        overrides: &appearance.overrides,
    }
}

fn placed(entry: SettingEntry, control: impl IntoElement, stacked: bool) -> SettingEntry {
    if stacked {
        entry.child(control)
    } else {
        entry.control(control)
    }
}

pub fn kit_row<T: Copy>(
    item: AppearancePageItem<T>,
    appearance: &Appearance,
    saved: &SavedThemes,
    controls: &AppearanceControls,
    stacked: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<SettingEntry> {
    let title = item.title()?;
    let entry = SettingEntry::new(title);
    let on_change = Rc::clone(&controls.on_change);
    Some(match item {
        AppearancePageItem::ThemeMode => entry.control(theme_mode_control(
            appearance.theme_mode(),
            move |mode, window, cx| on_change(AppearanceChange::Mode(mode), window, cx),
            cx,
        )),
        AppearancePageItem::Themes => {
            let picks = theme_picks(appearance, saved);
            let picked = picks
                .themes
                .iter()
                .map(|(id, _)| {
                    let mut next = appearance.clone();
                    next.pick_theme(id, saved);
                    (id.clone(), next.ui_font(saved).to_owned())
                })
                .collect::<Vec<_>>();
            let fonts = controls
                .ui_font
                .borrow()
                .as_ref()
                .map(|(select, _)| select.clone());
            let on_pick = move |index: usize, window: &mut Window, cx: &mut App| {
                let (id, font) = &picked[index];
                if let Some(select) = &fonts {
                    select.update(cx, |select, cx| {
                        select.set_selected_value(font, window, cx);
                    });
                }
                on_change(AppearanceChange::Theme(id.clone()), window, cx);
            };
            entry.control(picker(&picks, on_pick, controls.theme_menu.clone(), cx))
        }
        AppearancePageItem::Accent => {
            let picks = theme_picks(appearance, saved);
            let custom = ColorPicker::new(&controls.accent, ala::transparent_black())
                .label(title)
                .swatch(px(SWATCH));
            let picker = controls.accent.clone();
            let swatches = accent_swatches(
                &picks,
                cx.theme().mode,
                custom,
                move |name, window, cx| {
                    picker.update(cx, |picker, cx| picker.set_color(None, window, cx));
                    on_change(
                        AppearanceChange::Accent(name.map(str::to_owned)),
                        window,
                        cx,
                    );
                },
                cx,
            );
            placed(entry, swatches, stacked)
        }
        AppearancePageItem::TintHue => {
            let hue = appearance.overrides.tint_hue.unwrap_or(DEFAULT_TINT_HUE);
            let slider = tint_hue_slider(hue, controls.change(AppearanceChange::TintHue), cx);
            placed(entry, slider, stacked)
        }
        AppearancePageItem::Tint => {
            let tint = appearance.overrides.tint.unwrap_or(0.0);
            let slider = tint_slider(tint, controls.change(AppearanceChange::Tint), cx);
            placed(entry, slider, stacked)
        }
        AppearancePageItem::CornerRadius => {
            let slider = corner_radius_slider(
                appearance.corner_radius(saved),
                controls.change(AppearanceChange::Radius),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::ChromeContrast => {
            let slider = contrast_slider(
                appearance.contrast,
                controls.change(AppearanceChange::Contrast),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::UiFontFamily => {
            let select = controls.ui_font_select(appearance, saved, window, cx);
            entry.control(
                div().w(px(200.0)).max_w_full().flex_none().child(
                    Select::new(&select)
                        .title(title)
                        .placeholder("System default")
                        .on(super::settings_card_ground(cx)),
                ),
            )
        }
        AppearancePageItem::UiZoom => {
            let slider = zoom_slider(
                appearance.zoom,
                &controls.zoom,
                controls.change(AppearanceChange::Zoom),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::Animations => entry.control(
            Switch::new("settings-animations")
                .checked(appearance.animations)
                .on_click(move |animations, window, cx| {
                    on_change(AppearanceChange::Animations(*animations), window, cx);
                }),
        ),
        AppearancePageItem::AnimationSpeed => {
            let slider = animation_speed_slider(
                appearance.animation_speed,
                controls.change(AppearanceChange::AnimationSpeed),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::Title
        | AppearancePageItem::Group { .. }
        | AppearancePageItem::Advanced { .. }
        | AppearancePageItem::App(_) => return None,
    })
}

pub fn picker_tile(
    id: SharedString,
    label: impl Into<SharedString>,
    preview: impl IntoElement,
    selected: bool,
    cx: &App,
) -> ala::Stateful<ala::Div> {
    tile(id, label, preview, selected, false, cx)
}

#[allow(
    clippy::disallowed_methods,
    reason = "the focus halo is a ring around the tile, 1px in every look"
)]
fn tile(
    id: SharedString,
    label: impl Into<SharedString>,
    preview: impl IntoElement,
    selected: bool,
    focused: bool,
    cx: &App,
) -> ala::Stateful<ala::Div> {
    let frame = cx.theme().layer(
        crate::Surface::new(crate::roles::SETTINGS_TILE).on(super::settings_card_ground(cx)),
        if selected {
            crate::PaintState::Selected
        } else {
            crate::PaintState::Rest
        },
    );
    div()
        .id(ala::ElementId::Name(id))
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .child(
            div()
                .p(px(TILE_HALO - 1.0))
                .rounded(around(frame.radius, TILE_FRAME_PADDING + TILE_HALO))
                .border_1()
                .border_color(if focused {
                    cx.theme().accent
                } else {
                    ala::transparent_black()
                })
                .child(
                    div()
                        .p(px(TILE_FRAME_PADDING))
                        .layer(&crate::Layer {
                            radius: around(frame.radius, TILE_FRAME_PADDING),
                            ..frame
                        })
                        .child(preview),
                ),
        )
        .child(
            div()
                .w_0()
                .min_w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_center()
                .text_size(crate::rems_from_px(11.0))
                .text_color(if selected {
                    cx.theme().foreground
                } else {
                    cx.theme().foreground.muted()
                })
                .child(label.into()),
        )
}

fn around(radius: ala::Pixels, inset: f32) -> ala::Pixels {
    if radius > px(0.0) {
        radius + px(inset)
    } else {
        px(0.0)
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another theme's colors, not the live one's"
)]
pub fn theme_preview(
    mode: Option<ThemeMode>,
    light: &ThemeColor,
    dark: &ThemeColor,
    cx: &App,
) -> ala::Div {
    match mode {
        Some(ThemeMode::Light) => palette_preview(light, cx),
        Some(ThemeMode::Dark) => palette_preview(dark, cx),
        None => {
            let split = THEME_PREVIEW_WIDTH / 2.0;
            div()
                .relative()
                .w(px(THEME_PREVIEW_WIDTH))
                .h(px(THEME_PREVIEW_HEIGHT))
                .rounded(cx.theme().radius)
                .bg(theme_preview_split_background(
                    light.background.raised(2),
                    dark.background,
                    0.5,
                ))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(THEME_PREVIEW_SIDEBAR_WIDTH))
                        .h_full()
                        .w(px(split - THEME_PREVIEW_SIDEBAR_WIDTH))
                        .bg(light.background),
                )
                .child(theme_preview_contents(light))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .h_full()
                        .w(px(split))
                        .overflow_hidden()
                        .child(theme_preview_contents(dark).absolute().top_0().right_0()),
                )
        }
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another theme's colors and surfaces, not the live one's"
)]
pub fn interface_theme_preview(theme: &InterfaceTheme, mode: ThemeMode, _: &App) -> ala::Div {
    let colors = theme.colors(mode);
    let materials = &theme.materials;
    let mut kit = crate::Theme::from(&colors);
    theme.apply(&mut kit);
    let popover = kit.style(crate::SurfaceRole::POPOVER);
    let surface_radius = (materials.radius / 3.0).min(10.0);
    let row_radius = surface_radius / 2.0;
    let inset = materials.row_inset.min(3.0);
    let frosted = popover.fill == crate::Fill::Glass
        && materials.glass.is_some_and(|glass| glass.blur > px(0.0));
    let (selection, selected_text) = match kit.style(crate::SurfaceRole::MENU_ITEM).selected {
        crate::Highlight::Accent => (colors.accent.opaque(), accent_ink(&colors)),
        _ => (colors.background.washed(3), colors.foreground),
    };
    let text = colors.foreground.muted();
    let row = |width: f32, selected: bool| {
        div()
            .h(px(10.0))
            .flex()
            .items_center()
            .px(px(5.0))
            .rounded(px(row_radius))
            .when(selected, |row| row.bg(selection))
            .child(
                div()
                    .w(px(width))
                    .h(px(3.0))
                    .rounded_full()
                    .bg(if selected { selected_text } else { text }),
            )
    };
    let surface = colors.background.raised(2).opaque();
    let menu = div()
        .w(px(44.0))
        .flex()
        .flex_col()
        .py(px(3.0))
        .px(px(inset))
        .when(inset > 0.0, |menu| menu.gap(px(1.0)))
        .rounded(px(surface_radius))
        .bg(if frosted {
            surface.opacity(0.6)
        } else {
            surface
        })
        .when(materials.outline > 0.0 && popover.outline, |menu| {
            menu.border(px(0.5))
                .border_color(colors.foreground.opacity(0.15))
        })
        .when(
            kit.casts_shadow(crate::SurfaceRole::POPOVER) && materials.float.strength > 0.0,
            |menu| {
                menu.shadow(vec![ala::BoxShadow {
                    color: colors.shadow_color,
                    offset: ala::point(px(0.0), px(3.0)),
                    blur_radius: px(8.0),
                    spread_radius: px(-1.0),
                    inset: false,
                }])
            },
        )
        .children([row(24.0, false), row(18.0, true), row(21.0, false)]);
    let (primary, primary_text) = match materials.primary {
        crate::PrimaryStyle::Accent => (colors.accent, accent_ink(&colors)),
        crate::PrimaryStyle::Foreground => (colors.foreground, colors.background),
    };
    let button = div()
        .w(px(20.0))
        .h(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(row_radius.max(2.0)))
        .bg(primary)
        .child(div().w(px(10.0)).h(px(2.0)).rounded_full().bg(primary_text));
    div()
        .w(px(THEME_PREVIEW_WIDTH))
        .h(px(THEME_PREVIEW_HEIGHT))
        .flex()
        .overflow_hidden()
        .rounded(px(surface_radius.max(4.0)))
        .bg(theme_preview_split_background(
            colors.background.raised(2),
            colors.background,
            THEME_PREVIEW_SIDEBAR_WIDTH / THEME_PREVIEW_WIDTH,
        ))
        .child(
            div()
                .w(px(THEME_PREVIEW_SIDEBAR_WIDTH))
                .flex_none()
                .border_r_1()
                .border_color(colors.border()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(4.0))
                .p(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div().flex().items_center().gap(px(3.0)).children(
                                [colors.danger, colors.warning, colors.success]
                                    .map(|light| div().size(px(4.0)).rounded_full().bg(light)),
                            ),
                        )
                        .child(button),
                )
                .child(menu),
        )
}

fn accent_ink(colors: &ThemeColor) -> ala::Hsla {
    let mut theme = crate::Theme::from(colors);
    theme.primary = crate::PrimaryStyle::Accent;
    theme
        .paint(crate::Tone::Primary, crate::Emphasis::Solid)
        .text
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another palette's colors, not the live one's"
)]
pub fn palette_preview(colors: &ThemeColor, cx: &App) -> ala::Div {
    let sidebar_fraction = THEME_PREVIEW_SIDEBAR_WIDTH / THEME_PREVIEW_WIDTH;
    div()
        .w(px(THEME_PREVIEW_WIDTH))
        .h(px(THEME_PREVIEW_HEIGHT))
        .rounded(cx.theme().radius)
        .bg(theme_preview_split_background(
            colors.background.raised(2),
            colors.background,
            sidebar_fraction,
        ))
        .child(theme_preview_contents(colors))
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another palette's colors and lines, not the live one's"
)]
fn theme_preview_contents(colors: &ThemeColor) -> ala::Div {
    let text = colors.foreground.muted();
    let text_bar = move |width: f32| div().w(px(width)).h(px(3.0)).rounded_full().bg(text);
    div()
        .flex()
        .w(px(THEME_PREVIEW_WIDTH))
        .h(px(THEME_PREVIEW_HEIGHT))
        .child(
            div()
                .w(px(THEME_PREVIEW_SIDEBAR_WIDTH))
                .flex_none()
                .border_r_1()
                .border_color(colors.border()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .p(px(7.0))
                .child(
                    div().flex().items_center().gap(px(3.0)).children(
                        [colors.danger, colors.warning, colors.success]
                            .map(|light| div().size(px(4.0)).rounded_full().bg(light)),
                    ),
                )
                .child(text_bar(36.0))
                .child(text_bar(22.0))
                .child(
                    div()
                        .w(px(14.0))
                        .h(px(5.0))
                        .rounded_full()
                        .bg(colors.accent),
                ),
        )
}

fn theme_preview_split_background(
    left: ala::Hsla,
    right: ala::Hsla,
    split: f32,
) -> ala::Background {
    ala::linear_gradient(
        90.0,
        ala::linear_color_stop(left, split),
        ala::linear_color_stop(right, split),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_labels_tell_apart_themes_that_share_a_name() {
        let named = |name: &str| InterfaceTheme {
            name: name.to_owned(),
            ..crate::interface_theme::default_theme().clone()
        };
        let (yaru, copy, macos, unnamed) =
            (named("Yaru"), named("Yaru"), named("macOS"), named(""));
        let overrides = ThemeOverrides::default();
        let picks = ThemePicks {
            themes: vec![
                ("yaru".into(), &yaru),
                ("theme-2".into(), &copy),
                ("theme-1".into(), &macos),
                ("theme-3".into(), &unnamed),
            ],
            selected: 0,
            overrides: &overrides,
        };
        assert_eq!(
            theme_labels(&picks),
            ["Yaru", "Yaru (theme-2)", "macOS", "theme-3"]
        );
    }

    #[test]
    fn advanced_lists_the_other_knobs_only_while_expanded() {
        type Item = AppearancePageItem;
        let collapsed = appearance_page_items::<Infallible>(false);
        assert!(matches!(
            collapsed.last(),
            Some(Item::Advanced { expanded: false })
        ));
        assert!(!collapsed.contains(&Item::CornerRadius));

        let expanded = appearance_page_items::<Infallible>(true);
        let advanced = expanded
            .iter()
            .position(|item| *item == Item::Advanced { expanded: true })
            .expect("Advanced heading");
        assert!(expanded[..advanced].contains(&Item::Themes));
        assert!(expanded[advanced..].contains(&Item::CornerRadius));
        assert_eq!(expanded.last(), Some(&Item::AnimationSpeed));
    }

    #[test]
    fn an_apps_own_rows_are_entries_without_a_kit_title() {
        type Item = AppearancePageItem<u8>;
        let items = [
            Item::Group { title: "Group" },
            Item::App(1),
            Item::App(2),
            Item::Advanced { expanded: false },
        ];
        assert!(Item::App(1).is_entry());
        assert_eq!(Item::App(1).title(), None);
        assert_eq!(
            [1, 2].map(|index| run_position(&items, index, Item::is_entry)),
            [StackPosition::First, StackPosition::Last]
        );
    }

    #[ala::test]
    fn kit_rows_draw_every_kit_entry_and_leave_the_rest_to_the_app(cx: &mut ala::TestAppContext) {
        type Item = AppearancePageItem<u8>;
        cx.update(crate::init);
        let cx = cx.add_empty_window();
        let changes = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&changes);
        cx.update(|window, cx| {
            let appearance = Appearance::default();
            let controls = AppearanceControls::new(
                &appearance,
                move |change, _, _| seen.borrow_mut().push(change),
                window,
                cx,
            );
            let mut items = appearance_page_items::<u8>(true);
            items.push(Item::App(1));
            for item in items {
                let row = kit_row(item, &appearance, &[], &controls, false, window, cx);
                let kit_entry = item.is_entry() && !matches!(item, Item::App(_));
                assert_eq!(row.is_some(), kit_entry, "{item:?}");
                assert_eq!(item.title().is_some(), kit_entry, "{item:?}");
            }
            controls.change(AppearanceChange::Contrast)(1.5, window, cx);
        });
        assert_eq!(*changes.borrow(), [AppearanceChange::Contrast(1.5)]);
    }
}
