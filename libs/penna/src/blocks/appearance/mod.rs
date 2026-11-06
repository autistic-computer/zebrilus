use crate::blocks::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use crate::blocks::settings::{SettingEntry, SettingsSelectItem, StackPosition};
use crate::components::segmented::SegmentedControl;
use crate::components::select::{Select, SelectEvent, SelectItem as _, SelectState};
use crate::components::slider::Slider;
use crate::components::switch::Switch;
use crate::foundation::interface_theme::{
    ACCENTS, Appearance, AppearanceChange, Controls, Glass, corner_radius_at, corner_share,
};
use crate::foundation::motion::Motion;
use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Density, Space, StyledExt as _,
    ThemeColor, ThemeMode,
};
use ala::{
    AnyElement, App, Entity, FocusHandle, IntoElement, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use std::{cell::RefCell, convert::Infallible, rc::Rc, sync::Arc};

const THEME_PREVIEW_WIDTH: f32 = 84.0;
const THEME_PREVIEW_HEIGHT: f32 = 56.0;
const THEME_PREVIEW_SIDEBAR_WIDTH: f32 = 20.0;

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
        .when(ends_run, |row| row.pb(Space::GAP_RELATED))
        .child(element)
        .into_any_element()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppearancePageItem<T = Infallible> {
    Title,
    Group { title: &'static str },
    ThemeMode,
    Accent,
    TintHue,
    Tint,
    CornerRadius,
    Controls,
    Glass,
    UiFontFamily,
    TextSize,
    Density,
    IncreaseContrast,
    ReduceTransparency,
    Motion,
    App(T),
}

impl<T: Copy> AppearancePageItem<T> {
    pub const fn is_entry(self) -> bool {
        !matches!(self, Self::Title | Self::Group { .. })
    }

    pub const fn title(self) -> Option<&'static str> {
        Some(match self {
            Self::ThemeMode => "Mode",
            Self::Accent => "Accent",
            Self::TintHue => "Tint",
            Self::Tint => "Tint strength",
            Self::Controls => "Controls",
            Self::Glass => "Glass",
            Self::CornerRadius => "Corner radius",
            Self::IncreaseContrast => "Increase contrast",
            Self::ReduceTransparency => "Reduce transparency",
            Self::UiFontFamily => "UI font",
            Self::TextSize => "Text size",
            Self::Density => "Density",
            Self::Motion => "Motion",
            Self::Title | Self::Group { .. } | Self::App(_) => return None,
        })
    }
}

pub const LOOK: &str = "Look";
pub const SIZE: &str = "Size";
pub const ACCESSIBILITY: &str = "Accessibility";

pub fn appearance_page_items<T>() -> Vec<AppearancePageItem<T>> {
    vec![
        AppearancePageItem::Title,
        AppearancePageItem::Group { title: LOOK },
        AppearancePageItem::ThemeMode,
        AppearancePageItem::Accent,
        AppearancePageItem::TintHue,
        AppearancePageItem::Tint,
        AppearancePageItem::CornerRadius,
        AppearancePageItem::Controls,
        AppearancePageItem::Glass,
        AppearancePageItem::UiFontFamily,
        AppearancePageItem::Group { title: SIZE },
        AppearancePageItem::TextSize,
        AppearancePageItem::Density,
        AppearancePageItem::Group {
            title: ACCESSIBILITY,
        },
        AppearancePageItem::IncreaseContrast,
        AppearancePageItem::ReduceTransparency,
        AppearancePageItem::Motion,
    ]
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
        .on(crate::blocks::settings::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(THEME_MODES[index].0, window, cx))
}

pub const DENSITIES: [(Density, &str); 3] = [
    (Density::Compact, "Compact"),
    (Density::Regular, "Regular"),
    (Density::Comfortable, "Comfortable"),
];

pub fn density_control(
    selected: Density,
    on_pick: impl Fn(Density, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SegmentedControl {
    SegmentedControl::new("settings-density", DENSITIES.map(|(_, title)| title))
        .selected(
            DENSITIES
                .iter()
                .position(|(density, _)| *density == selected)
                .unwrap_or(0),
        )
        .on(crate::blocks::settings::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(DENSITIES[index].0, window, cx))
}

pub const CONTROL_SHAPES: [(Controls, &str); 2] =
    [(Controls::Rounded, "Rounded"), (Controls::Pill, "Pill")];

pub fn controls_control(
    selected: Controls,
    on_pick: impl Fn(Controls, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SegmentedControl {
    SegmentedControl::new("settings-controls", CONTROL_SHAPES.map(|(_, title)| title))
        .selected(
            CONTROL_SHAPES
                .iter()
                .position(|(controls, _)| *controls == selected)
                .unwrap_or(0),
        )
        .on(crate::blocks::settings::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(CONTROL_SHAPES[index].0, window, cx))
}

pub fn motion_control(
    selected: Motion,
    on_pick: impl Fn(Motion, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SegmentedControl {
    SegmentedControl::new("settings-motion", Motion::ALL.map(Motion::title))
        .selected(
            Motion::ALL
                .iter()
                .position(|motion| *motion == selected)
                .unwrap_or(1),
        )
        .on(crate::blocks::settings::settings_card_ground(cx))
        .on_select(move |index, window, cx| on_pick(Motion::ALL[index], window, cx))
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
    selected: &str,
    mode: ThemeMode,
    custom: crate::blocks::color_picker::ColorPicker,
    on_pick: impl Fn(Option<&'static str>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    let ink = ThemeColor::for_mode(mode).foreground;
    let on_pick = Rc::new(on_pick);
    let ring = cx.theme().foreground;
    let radius = cx.theme().swatch_radius();
    let coarse = crate::blocks::touch::CoarsePointer::get(cx);
    let size = if coarse { SHEET_SWATCH } else { SWATCH };
    let swatch = |id: &'static str, name: SharedString, color: ala::Hsla| {
        let on_pick = Rc::clone(&on_pick);
        ringed(
            selected == id,
            ring,
            radius,
            div()
                .author_id(SharedString::from(format!("settings-accent-{id}")))
                .role(ala::Role::Button)
                .aria_label(name.clone())
                .size(px(size))
                .rounded(radius)
                .bg(color)
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    crate::components::tooltip::Tooltip::new(name.clone()).build(window, cx)
                })
                .on_click(move |_, window, cx| on_pick(Some(id), window, cx)),
        )
    };
    let named = ACCENTS.iter().filter_map(|name| {
        let color = crate::foundation::interface_theme::accent_color(name, mode)?;
        Some(swatch(name, accent_title(name), color))
    });
    let row = div()
        .flex()
        .flex_none()
        .flex_wrap()
        .items_center()
        .when(coarse, |row| row.w_full().justify_between())
        .gap(Space::S4)
        .child(swatch("ink", "Ink".into(), ink))
        .children(named);
    if coarse {
        let chosen = match selected {
            "ink" => "Ink".into(),
            name if ACCENTS.contains(&name) => accent_title(name),
            hex => hex.to_uppercase().into(),
        };
        let color = crate::foundation::interface_theme::accent_color(selected, mode).unwrap_or(ink);
        return div().w_full().child(custom.presets(chosen, color, row));
    }
    let custom_selected = selected != "ink" && !ACCENTS.contains(&selected);
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
    if crate::blocks::touch::CoarsePointer::get(cx) {
        return div()
            .flex()
            .items_center()
            .gap(Space::S10)
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
    corner_slider("settings-corner-radius", "Corner radius", radius, on_change, cx)
}

pub fn corner_slider(
    id: &'static str,
    label: &'static str,
    radius: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new(
            id,
            label,
            corner_share(radius),
            0.0..=1.0,
            move |share, window, cx| on_change(corner_radius_at(share), window, cx),
        )
        .step(0.01)
        .display(|share| format!("{:.0}%", share * 100.0).into()),
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
            "settings-increase-contrast",
            "Increase contrast",
            (contrast - 1.0).clamp(0.0, 1.0),
            0.0..=1.0,
            move |more, window, cx| on_change(1.0 + more, window, cx),
        )
        .step(0.01)
        .display(|more| format!("{:.0}%", more * 100.0).into()),
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
        percent("settings-zoom", "Text size", value, on_change)
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
    if crate::blocks::touch::CoarsePointer::get(cx) {
        return div().w_full().child(slider);
    }
    let shown = div()
        .type_style(crate::TextStyle::Small, cx)
        .text_color(cx.theme().foreground.muted())
        .child(slider.shown());
    slider_row(slider, shown)
}

fn slider_row(slider: Slider, trailing: impl IntoElement) -> ala::Div {
    div()
        .flex()
        .items_center()
        .gap(Space::S10)
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
) -> crate::blocks::settings::SettingsVirtualColumn {
    let title = title.into();
    crate::blocks::settings::settings_virtual_column(
        "settings-appearance",
        items.len(),
        move |index, window, cx| {
            let Some(item) = items.get(index).copied() else {
                return div().into_any_element();
            };
            let position = run_position(&items, index, AppearancePageItem::is_entry);
            let row = match item {
                AppearancePageItem::Title => {
                    crate::blocks::settings::settings_page_heading(title.clone(), cx)
                        .into_any_element()
                }
                AppearancePageItem::Group { title } => {
                    crate::blocks::settings::settings_list_group_header(title, cx)
                        .into_any_element()
                }
                _ => render(item, position, window, cx),
            };
            page_row(row, !item.is_entry() || position.ends_run())
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
            _accent: subscription,
        }
    }

    pub fn zoom_focus(&self) -> &FocusHandle {
        &self.zoom
    }

    pub fn sync(&self, appearance: &Appearance, window: &mut Window, cx: &mut App) {
        let accent = custom_accent(appearance);
        self.accent
            .update(cx, |picker, cx| picker.set_color(accent, window, cx));
        if let Some((select, _)) = &*self.ui_font.borrow() {
            let font = appearance.ui_font().to_owned();
            select.update(cx, |select, cx| {
                select.set_selected_value(&font, window, cx);
            });
        }
    }

    fn ui_font_select(
        &self,
        appearance: &Appearance,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<SelectState<Vec<SettingsSelectItem>>> {
        if let Some((select, _)) = &*self.ui_font.borrow() {
            return select.clone();
        }
        let select = ui_font_select(Some(appearance.ui_font()), window, cx);
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
        .and_then(|accent| {
            crate::foundation::interface_theme::accent_color(accent, ThemeMode::Dark)
        })
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
        AppearancePageItem::Controls => entry.control(controls_control(
            appearance.controls(),
            move |shape, window, cx| on_change(AppearanceChange::Controls(shape), window, cx),
            cx,
        )),
        AppearancePageItem::Glass => entry.control(
            Switch::new("settings-glass")
                .checked(appearance.glass() != Glass::None)
                .on_click(move |on, window, cx| {
                    let glass = if *on { Glass::Liquid } else { Glass::None };
                    on_change(AppearanceChange::Glass(glass), window, cx);
                }),
        ),
        AppearancePageItem::Accent => {
            let selected = appearance.accent();
            let custom = ColorPicker::new(&controls.accent, ala::transparent_black())
                .id("settings-accent-custom")
                .label(title)
                .swatch(px(SWATCH));
            let picker = controls.accent.clone();
            let swatches = accent_swatches(
                &selected,
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
            let hue = appearance.neutral().hue;
            let slider = tint_hue_slider(hue, controls.change(AppearanceChange::TintHue), cx);
            placed(entry, slider, stacked)
        }
        AppearancePageItem::Tint => {
            let tint = appearance.neutral().amount;
            let slider = tint_slider(tint, controls.change(AppearanceChange::Tint), cx);
            placed(entry, slider, stacked)
        }
        AppearancePageItem::CornerRadius => {
            let slider = corner_radius_slider(
                appearance.corner_radius(),
                controls.change(AppearanceChange::Radius),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::IncreaseContrast => {
            let slider = contrast_slider(
                appearance.contrast,
                controls.change(AppearanceChange::Contrast),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::ReduceTransparency => entry.control(
            Switch::new("settings-reduce-transparency")
                .checked(appearance.reduce_transparency)
                .on_click(move |reduce, window, cx| {
                    on_change(AppearanceChange::ReduceTransparency(*reduce), window, cx);
                }),
        ),
        AppearancePageItem::UiFontFamily => {
            let select = controls.ui_font_select(appearance, window, cx);
            entry.control(
                div().w(px(200.0)).max_w_full().flex_none().child(
                    Select::new(&select)
                        .title(title)
                        .placeholder("System default")
                        .on(crate::blocks::settings::settings_card_ground(cx)),
                ),
            )
        }
        AppearancePageItem::TextSize => {
            let slider = zoom_slider(
                appearance.zoom,
                &controls.zoom,
                controls.change(AppearanceChange::Zoom),
                cx,
            );
            placed(entry, slider, stacked)
        }
        AppearancePageItem::Density => {
            let control = density_control(
                appearance.density,
                move |density, window, cx| {
                    on_change(AppearanceChange::Density(density), window, cx);
                },
                cx,
            );
            if stacked {
                entry.child(div().flex().child(control))
            } else {
                entry.control(control)
            }
        }
        AppearancePageItem::Motion => entry.control(motion_control(
            appearance.motion,
            move |motion, window, cx| on_change(AppearanceChange::Motion(motion), window, cx),
            cx,
        )),
        AppearancePageItem::Title
        | AppearancePageItem::Group { .. }
        | AppearancePageItem::App(_) => {
            return None;
        }
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
        crate::Surface::new(crate::foundation::roles::SETTINGS_TILE)
            .on(crate::blocks::settings::settings_card_ground(cx)),
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
        .gap(Space::S6)
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
                .type_style(crate::TextStyle::Caption, cx)
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
    fn an_apps_own_rows_are_entries_without_a_kit_title() {
        type Item = AppearancePageItem<u8>;
        let items = [
            Item::Group { title: "Group" },
            Item::App(1),
            Item::App(2),
            Item::Group { title: "Other" },
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
            let mut items = appearance_page_items::<u8>();
            items.push(Item::App(1));
            for item in items {
                let row = kit_row(item, &appearance, &controls, false, window, cx);
                let kit_entry = item.is_entry() && !matches!(item, Item::App(_));
                assert_eq!(row.is_some(), kit_entry, "{item:?}");
                assert_eq!(item.title().is_some(), kit_entry, "{item:?}");
            }
            controls.change(AppearanceChange::Contrast)(1.5, window, cx);
        });
        assert_eq!(*changes.borrow(), [AppearanceChange::Contrast(1.5)]);
    }
}
