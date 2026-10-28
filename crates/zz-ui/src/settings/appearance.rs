use super::{SettingsSelectItem, StackPosition};
use crate::button::{Button, ButtonVariants as _};
use crate::interface_theme::{
    ACCENTS, BUILTIN_THEMES, InterfaceTheme, MAX_CORNER_RADIUS, ThemeOverrides,
};
use crate::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use crate::segmented::SegmentedControl;
use crate::select::{SelectItem as _, SelectState};
use crate::slider::Slider;
use crate::{ActiveTheme as _, Colorize as _, IconName, StyledExt as _, ThemeColor, ThemeMode};
use std::{rc::Rc, sync::Arc};
use zz_client::appearance::ThemeModeSetting;
use zz_gpui::{
    Anchor, AnyElement, App, Context, Entity, FocusHandle, IntoElement, SharedString, Window, div,
    prelude::*, px,
};

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

/// The id of the Interface page's Advanced heading.
pub const ADVANCED_DISCLOSURE: &str = "settings-appearance-advanced";

#[derive(Clone, Copy, PartialEq)]
pub enum AppearancePageItem {
    Title,
    Group {
        title: &'static str,
    },
    ThemeMode,
    Accent,
    TintHue,
    Tint,
    Themes,
    ProgramColors,
    Advanced {
        expanded: bool,
    },
    CornerRadius,
    ChromeContrast,
    UiFontFamily,
    UiZoom,
    AppIcon,
    Animations,
    AnimationSpeed,
    WindowBackgroundBlur,
    WindowBackgroundOpacity,
    #[cfg(target_os = "linux")]
    WindowCornerRadius,
    #[cfg(target_os = "linux")]
    UseSystemTitlebar,
}

impl AppearancePageItem {
    pub const fn is_entry(self) -> bool {
        !matches!(
            self,
            Self::Title | Self::Group { .. } | Self::Advanced { .. }
        )
    }

    /// The title of a row every client draws the same way.
    pub const fn title(self) -> Option<&'static str> {
        Some(match self {
            Self::Accent => "Accent",
            Self::TintHue => "Tint",
            Self::Tint => "Tint strength",
            Self::ProgramColors => "Program colors",
            Self::Themes => "Theme",
            Self::CornerRadius => "Corner radius",
            Self::ChromeContrast => "Contrast",
            Self::WindowBackgroundOpacity => "Window blur opacity",
            Self::AnimationSpeed => "Animation speed",
            Self::UiZoom => "UI zoom",
            _ => return None,
        })
    }
}

/// The Interface page: the mode and how the theme looks up front, every
/// other knob behind the Advanced heading, listed only while `advanced` is
/// expanded.
pub fn appearance_page_items(
    macos: bool,
    has_window_blur: bool,
    advanced: bool,
) -> Vec<AppearancePageItem> {
    let mut items = vec![
        AppearancePageItem::Title,
        AppearancePageItem::Group { title: "Theme" },
        AppearancePageItem::ThemeMode,
        AppearancePageItem::Themes,
        AppearancePageItem::Accent,
        AppearancePageItem::TintHue,
        AppearancePageItem::Tint,
        AppearancePageItem::Group { title: "Sidebar" },
        AppearancePageItem::ProgramColors,
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
    ]);
    if macos {
        items.push(AppearancePageItem::AppIcon);
    }
    items.extend([
        AppearancePageItem::Animations,
        AppearancePageItem::AnimationSpeed,
    ]);
    if has_window_blur {
        items.extend([
            AppearancePageItem::WindowBackgroundBlur,
            AppearancePageItem::WindowBackgroundOpacity,
        ]);
    }
    #[cfg(target_os = "linux")]
    items.extend([
        AppearancePageItem::WindowCornerRadius,
        AppearancePageItem::UseSystemTitlebar,
    ]);
    items
}

/// The look the Theme rows show and pick from: the themes a client offers
/// (the built-in ones first, then saved ones), which one the overrides sit
/// on, and the overrides.
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

    /// Whether the overrides sit on a saved theme rather than a built-in.
    pub fn custom(&self) -> bool {
        self.selected >= BUILTIN_THEMES.len()
    }

    /// The theme on screen with `change` made to the overrides first.
    pub fn composed(&self, change: impl FnOnce(&mut ThemeOverrides)) -> InterfaceTheme {
        let mut overrides = self.overrides.clone();
        change(&mut overrides);
        let mut theme = self.base().clone();
        overrides.apply_to(&mut theme);
        theme
    }
}

pub fn theme_mode_control(
    selected: ThemeModeSetting,
    on_pick: impl Fn(ThemeModeSetting, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SegmentedControl {
    let modes = ThemeModeSetting::ALL;
    SegmentedControl::new("settings-theme-mode", modes.map(ThemeModeSetting::title))
        .selected(modes.iter().position(|mode| *mode == selected).unwrap_or(0))
        .on(super::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(modes[index], window, cx))
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

fn accent_label(accent: zz_gpui::Hsla, label: SharedString, cx: &App) -> zz_gpui::Div {
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
                .line_height(zz_gpui::relative(1.0))
                .child(label),
        )
}

pub fn theme_picker(
    picks: &ThemePicks,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    more: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    cx: &App,
) -> zz_gpui::Div {
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
        .child(
            Button::new("settings-theme-more")
                .ghost()
                .icon(IconName::Ellipsis)
                .tooltip("More")
                .on(ground)
                .dropdown_menu_with_anchor(Anchor::TopRight, more),
        )
}

pub const SWATCH: f32 = 18.0;
const SHEET_SWATCH: f32 = 30.0;
const SWATCH_RING_GAP: f32 = 2.0;
const SWATCH_RING: f32 = 1.5;

/// The theme's own accent, the named accents and `custom`, a color picker
/// for any other, as swatches [`SWATCH`] across. Reports `None` for the theme's own and
/// the name of a named one.
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
) -> zz_gpui::Div {
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
    let swatch = |id: &'static str, name: SharedString, color: zz_gpui::Hsla| {
        let on_pick = Rc::clone(&on_pick);
        ringed(
            selected == id,
            ring,
            radius,
            div()
                .id(SharedString::from(format!("settings-accent-{id}")))
                .role(zz_gpui::Role::Button)
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
    ring: zz_gpui::Hsla,
    radius: zz_gpui::Pixels,
    swatch: impl IntoElement,
) -> zz_gpui::Div {
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
            zz_gpui::transparent_black()
        })
        .child(swatch)
}

const SLIDER_WIDTH: f32 = 180.0;
const HUE_STOPS: u16 = 12;

/// The tint hue as a slider over a rainbow, with a dot in the hue it picks.
pub fn tint_hue_slider(
    hue: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
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

/// How strongly the tint applies, as a slider with its percentage.
pub fn tint_slider(
    tint: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
    value_slider(
        Slider::new("settings-tint", "Tint strength", tint, 0.0..=1.0, on_change)
            .step(0.01)
            .display(|tint| format!("{:.0}%", tint * 100.0).into()),
        cx,
    )
}

pub fn window_opacity_slider(
    opacity: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
    value_slider(
        Slider::new(
            "settings-window-opacity",
            "Window blur opacity",
            opacity,
            0.0..=1.0,
            on_change,
        )
        .step(0.01)
        .display(|opacity| format!("{:.0}%", opacity * 100.0).into()),
        cx,
    )
}

/// The corner radius as a slider with its length.
pub fn corner_radius_slider(
    radius: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
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

/// The contrast, from 1 to 2, as a slider with its percentage.
pub fn contrast_slider(
    contrast: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
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

/// The animation speed, from 25% to 200%, as a slider with its percentage.
pub fn animation_speed_slider(
    speed: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
    value_slider(
        Slider::new(
            "settings-animation-speed",
            "Animation speed",
            speed,
            crate::interface_theme::MIN_ANIMATION_SPEED
                ..=crate::interface_theme::MAX_ANIMATION_SPEED,
            on_change,
        )
        .step(0.05)
        .display(|speed| format!("{:.0}%", speed * 100.0).into()),
        cx,
    )
}

/// A scale from 50% to 300%, like UI zoom, as a slider with its percentage.
pub fn percent_slider(
    id: &'static str,
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
    value_slider(percent(id, label, value, on_change), cx)
}

/// UI zoom as a [`percent_slider`] that applies once the drag lets go:
/// zooming while dragging rescales the slider under the finger. `focus` is
/// the row's handle; its touch sheet holds it, so the settings list keeps the
/// row, and the sheet, painted when the new zoom moves it off screen.
pub fn zoom_slider(
    value: f32,
    focus: &FocusHandle,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Div {
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

fn value_slider(slider: Slider, cx: &App) -> zz_gpui::Div {
    if crate::touch::CoarsePointer::get(cx) {
        return div().w_full().child(slider);
    }
    let shown = div()
        .text_size(crate::rems_from_px(12.0))
        .text_color(cx.theme().foreground.muted())
        .child(slider.shown());
    slider_row(slider, shown)
}

fn slider_row(slider: Slider, trailing: impl IntoElement) -> zz_gpui::Div {
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
fn hue_swatch(hue: f32) -> zz_gpui::Hsla {
    crate::oklch(0.72, 0.13, hue)
}

pub fn appearance_page(
    items: Vec<AppearancePageItem>,
    mut render: impl FnMut(AppearancePageItem, StackPosition, &mut Window, &mut App) -> AnyElement
    + 'static,
) -> super::SettingsVirtualColumn {
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
                    super::settings_page_title(super::SettingsSection::Appearance, cx)
                        .into_any_element()
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

pub struct AvailableFonts(pub Arc<dyn zz_gpui::PlatformTextSystem>);

impl zz_gpui::Global for AvailableFonts {}

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

pub fn picker_tile(
    id: SharedString,
    label: impl Into<SharedString>,
    preview: impl IntoElement,
    selected: bool,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    tile(id, label, preview, selected, false, cx)
}

/// A picker tile drawn as [`crate::roles::SETTINGS_TILE`], its corners
/// concentric with the preview it frames. The chosen one takes the role's
/// selected highlight; the keyboard focus halo stays a 1px ring outside it.
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
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let frame = cx.theme().layer(
        crate::Surface::new(crate::roles::SETTINGS_TILE).on(super::settings_card_ground(cx)),
        if selected {
            crate::PaintState::Selected
        } else {
            crate::PaintState::Rest
        },
    );
    div()
        .id(zz_gpui::ElementId::Name(id))
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
                    zz_gpui::transparent_black()
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

/// The radius of a box `inset` outside one rounded by `radius`, so the two
/// stay concentric. Square corners stay square.
fn around(radius: zz_gpui::Pixels, inset: f32) -> zz_gpui::Pixels {
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
) -> zz_gpui::Div {
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

/// A window with a menu over it, drawn in `theme`'s colors for `mode` and
/// the way its look draws a menu, for the theme tiles.
#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another theme's colors and surfaces, not the live one's"
)]
pub fn interface_theme_preview(
    theme: &crate::interface_theme::InterfaceTheme,
    mode: ThemeMode,
    _: &App,
) -> zz_gpui::Div {
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
                menu.shadow(vec![zz_gpui::BoxShadow {
                    color: colors.shadow_color,
                    offset: zz_gpui::point(px(0.0), px(3.0)),
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

/// Text on `colors`' accent, as a solid primary button letters it.
fn accent_ink(colors: &ThemeColor) -> zz_gpui::Hsla {
    let mut theme = crate::Theme::from(colors);
    theme.primary = crate::PrimaryStyle::Accent;
    theme
        .paint(crate::Tone::Primary, crate::Emphasis::Solid)
        .text
}

/// The window mockup the mode and theme tiles share, painted from `colors`.
#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another palette's colors, not the live one's"
)]
pub fn palette_preview(colors: &ThemeColor, cx: &App) -> zz_gpui::Div {
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
fn theme_preview_contents(colors: &ThemeColor) -> zz_gpui::Div {
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
    left: zz_gpui::Hsla,
    right: zz_gpui::Hsla,
    split: f32,
) -> zz_gpui::Background {
    zz_gpui::linear_gradient(
        90.0,
        zz_gpui::linear_color_stop(left, split),
        zz_gpui::linear_color_stop(right, split),
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
}
