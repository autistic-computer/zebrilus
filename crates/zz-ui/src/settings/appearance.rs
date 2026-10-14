use super::{SettingsSelectItem, StackPosition};
use crate::scroll::{GUTTER_WIDTH, Scrollbar, ScrollbarShow};
use crate::select::{SelectItem as _, SelectState};
use crate::{ActiveTheme as _, Colorize as _, StyledExt as _, ThemeColor, ThemeMode};
use std::{rc::Rc, sync::Arc};
use zz_gpui::{
    AnyElement, App, Entity, IntoElement, ScrollHandle, SharedString, Window, div, prelude::*, px,
};

const THEME_PREVIEW_WIDTH: f32 = 84.0;
const THEME_PREVIEW_HEIGHT: f32 = 56.0;
const THEME_PREVIEW_SIDEBAR_WIDTH: f32 = 20.0;

const PAGE_ROW_GAP: f32 = 8.0;
const TILE_GAP: f32 = 6.0;
const STRIP_INSET: f32 = super::SETTINGS_STACK_PADDING;
const TILE_FRAME_PADDING: f32 = 2.0;
const TILE_RING: f32 = 2.0;
const TILE_FRAME: f32 = TILE_FRAME_PADDING + TILE_RING;
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

#[derive(Clone, Copy, PartialEq)]
pub enum AppearancePageItem {
    Description,
    Group {
        title: &'static str,
        description: Option<&'static str>,
    },
    ThemeMode,
    InterfaceTheme,
    Advanced {
        expanded: bool,
    },
    UiFontFamily,
    UiZoom,
    AppIcon,
    ChromeContrast,
    Animations,
    WidgetCornerRadius,
    OutlineWidth,
    ShadowStrength,
    RowInset,
    Primary,
    Glass,
    WindowBackgroundBlur,
    #[cfg(target_os = "linux")]
    WindowCornerRadius,
    #[cfg(target_os = "linux")]
    UseSystemTitlebar,
}

impl AppearancePageItem {
    pub const fn is_entry(self) -> bool {
        !matches!(
            self,
            Self::Description | Self::Group { .. } | Self::Advanced { .. }
        )
    }

    /// The material settings that override the active theme, in the order
    /// the page lists them.
    pub const MATERIALS: [Self; 6] = [
        Self::WidgetCornerRadius,
        Self::OutlineWidth,
        Self::ShadowStrength,
        Self::RowInset,
        Self::Primary,
        Self::Glass,
    ];

    /// The title and description of a material setting.
    pub const fn material_text(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::WidgetCornerRadius => ("Corner radius", "How round corners are, from 0 to 25px."),
            Self::OutlineWidth => (
                "Outline",
                "The width of every outline the theme draws, from 0 (none) to 2px.",
            ),
            Self::ShadowStrength => (
                "Shadow strength",
                "How dark the theme's shadows are. 0 turns them off, 2 doubles them.",
            ),
            Self::RowInset => (
                "Row inset",
                "How far list rows float in from the edges. At 0 they become full-width bands.",
            ),
            Self::Primary => (
                "Primary buttons",
                "What the main action of a view is drawn in.",
            ),
            Self::Glass => (
                "Glass",
                "What glass surfaces are made of. Theme keeps the theme's own.",
            ),
            _ => return None,
        })
    }

    /// The choices of a material setting that is a choice, as title and
    /// value. The values are what theme files and `zz/config` spell.
    pub const fn material_choices(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Primary => &[("Text color", "foreground"), ("Accent", "accent")],
            Self::Glass => &[
                ("Theme", "theme"),
                ("Off", "off"),
                ("Regular", "regular"),
                ("Clear", "clear"),
                ("Frosted", "frosted"),
                ("Bubble", "bubble"),
                ("Smoked", "smoked"),
            ],
            _ => &[],
        }
    }
}

/// The Interface page: the mode and the theme up front, every other knob
/// behind the Advanced heading, listed only while `advanced` is expanded.
pub fn appearance_page_items(
    macos: bool,
    has_window_blur: bool,
    advanced: bool,
) -> Vec<AppearancePageItem> {
    let mut items = vec![
        AppearancePageItem::Description,
        AppearancePageItem::Group {
            title: "Theme",
            description: None,
        },
        AppearancePageItem::ThemeMode,
        AppearancePageItem::InterfaceTheme,
        AppearancePageItem::Advanced { expanded: advanced },
    ];
    if !advanced {
        return items;
    }
    items.extend([AppearancePageItem::UiFontFamily, AppearancePageItem::UiZoom]);
    if macos {
        items.push(AppearancePageItem::AppIcon);
    }
    items.extend([
        AppearancePageItem::ChromeContrast,
        AppearancePageItem::Animations,
    ]);
    if has_window_blur {
        items.push(AppearancePageItem::WindowBackgroundBlur);
    }
    #[cfg(target_os = "linux")]
    items.extend([
        AppearancePageItem::WindowCornerRadius,
        AppearancePageItem::UseSystemTitlebar,
    ]);
    items.push(AppearancePageItem::Group {
        title: "Materials",
        description: Some(
            "Change what the theme's corners, outlines, shadows and glass look like. Which piece uses which is up to the theme. Picking a theme resets these.",
        ),
    });
    items.extend(AppearancePageItem::MATERIALS);
    items
}

pub fn appearance_page(
    items: Vec<AppearancePageItem>,
    mut render: impl FnMut(AppearancePageItem, StackPosition, &mut Window, &mut App) -> AnyElement
    + 'static,
) -> super::SettingsVirtualColumn {
    super::settings_virtual_column(
        "settings-appearance",
        items.len(),
        move |index, window, cx| {
            let Some(item) = items.get(index).copied() else {
                return div().into_any_element();
            };
            let position = run_position(&items, index, AppearancePageItem::is_entry);
            let row = match item {
                AppearancePageItem::Description => {
                    super::settings_page_description(super::SettingsSection::Appearance, cx)
                        .into_any_element()
                }
                AppearancePageItem::Group { title, description } => {
                    super::settings_list_group_header(title, description, cx).into_any_element()
                }
                _ => render(item, position, window, cx),
            };
            page_row(row, !item.is_entry() || position.ends_run())
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
        crate::roles::SETTINGS_TILE,
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

/// A row of picker tiles that scrolls sideways once it outgrows the page and
/// bleeds to the row's edges. Clicking a tile or pressing the left and right
/// arrows while the strip has focus reports the tile's index to `on_select`.
#[derive(IntoElement)]
pub struct PickerStrip {
    id: &'static str,
    selected: usize,
    tiles: Vec<(SharedString, AnyElement)>,
    on_select: Option<Rc<dyn Fn(usize, &mut Window, &mut App)>>,
}

impl PickerStrip {
    pub fn new(id: &'static str, selected: usize) -> Self {
        Self {
            id,
            selected,
            tiles: Vec::new(),
            on_select: None,
        }
    }

    #[must_use]
    pub fn tile(mut self, label: impl Into<SharedString>, preview: impl IntoElement) -> Self {
        self.tiles.push((label.into(), preview.into_any_element()));
        self
    }

    #[must_use]
    pub fn tiles<L: Into<SharedString>, E: IntoElement>(
        mut self,
        tiles: impl IntoIterator<Item = (L, E)>,
    ) -> Self {
        for (label, preview) in tiles {
            self = self.tile(label, preview);
        }
        self
    }

    #[must_use]
    pub fn on_select(mut self, on_select: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(on_select));
        self
    }
}

impl RenderOnce for PickerStrip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll = window
            .use_keyed_state((self.id, 0usize), cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();
        let focus = window
            .use_keyed_state((self.id, 1usize), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let focused = focus.is_focused(window);
        let count = self.tiles.len();
        let selected = self.selected.min(count.saturating_sub(1));
        let on_select = self.on_select;
        let select = Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
            if let Some(on_select) = &on_select {
                on_select(index, window, cx);
            }
        });
        let id = self.id;
        let edge = || {
            div().flex_none().w(px(STRIP_INSET + TILE_FRAME
                - TILE_FRAME_PADDING
                - TILE_HALO
                - TILE_GAP))
        };
        let key_scroll = scroll.clone();
        let key_select = Rc::clone(&select);
        let tiles = div()
            .id(id)
            .track_focus(&focus)
            .track_scroll(&scroll)
            .flex()
            .gap(px(TILE_GAP))
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .on_key_down(move |event, window, cx| {
                let next = match event.keystroke.key.as_str() {
                    "left" => selected.checked_sub(1),
                    "right" => (selected + 1 < count).then_some(selected + 1),
                    _ => return,
                };
                cx.stop_propagation();
                if let Some(next) = next {
                    key_scroll.scroll_to_item(next + 1);
                    key_select(next, window, cx);
                }
            })
            .child(edge())
            .children(
                self.tiles
                    .into_iter()
                    .enumerate()
                    .map(|(index, (label, preview))| {
                        let select = Rc::clone(&select);
                        tile(
                            format!("{id}-{index}").into(),
                            label,
                            preview,
                            index == selected,
                            focused && index == selected,
                            cx,
                        )
                        .on_click(move |_, window, cx| select(index, window, cx))
                    }),
            )
            .child(edge());

        let coarse = crate::touch::CoarsePointer::get(cx);
        div()
            .flex()
            .flex_col()
            .mx(px(-STRIP_INSET))
            .when(!coarse, |strip| strip.mb(px(-8.0)))
            .child(
                div()
                    .relative()
                    .child(tiles)
                    .child(crate::compact::yield_back_swipe((id, 3usize), &scroll)),
            )
            .when(!coarse, |strip| {
                strip.child(
                    div()
                        .relative()
                        .flex_none()
                        .h(GUTTER_WIDTH)
                        .mx(px(STRIP_INSET))
                        .child(
                            Scrollbar::horizontal(&scroll)
                                .id((id, 2usize))
                                .scrollbar_show(ScrollbarShow::Always),
                        ),
                )
            })
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
    let popover = kit.style(crate::Kind::Popover.role());
    let surface_radius = (materials.radius / 3.0).min(10.0);
    let row_radius = surface_radius / 2.0;
    let inset = materials.row_inset.min(3.0);
    let frosted = popover.fill == crate::Fill::Glass
        && materials.glass.is_some_and(|glass| glass.blur > px(0.0));
    let (selection, selected_text) = match kit.style(crate::Kind::Row.role()).selected {
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
            kit.casts_shadow(crate::Kind::Popover.role()) && materials.float.strength > 0.0,
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
    use std::cell::Cell;
    use zz_gpui::{
        Context, Modifiers, Render, ScrollDelta, ScrollWheelEvent, TestAppContext,
        VisualTestContext, point,
    };

    type Picked = Rc<Cell<Option<(u8, usize)>>>;

    struct StripsTest {
        picked: Picked,
    }

    fn strip(id: &'static str, which: u8, first: &'static str, picked: Picked) -> PickerStrip {
        PickerStrip::new(id, 1)
            .tiles((0..8).map(move |index| {
                (
                    "tile",
                    div().w(px(84.0)).h(px(56.0)).when(index == 0, |this| {
                        this.debug_selector(move || first.to_string())
                    }),
                )
            }))
            .on_select(move |index, _, _| picked.set(Some((which, index))))
    }

    impl Render for StripsTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(300.0))
                .h(px(400.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .px(px(STRIP_INSET))
                .child(
                    div()
                        .debug_selector(|| "first-strip-container".to_string())
                        .child(strip(
                            "first-strip",
                            0,
                            "first-strip-tile",
                            self.picked.clone(),
                        )),
                )
                .child(strip(
                    "second-strip",
                    1,
                    "second-strip-tile",
                    self.picked.clone(),
                ))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[zz_gpui::test]
    fn strips_scroll_and_navigate_independently(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let picked: Picked = Rc::default();
        let (_, cx) = cx.add_window_view(|_, _| StripsTest {
            picked: picked.clone(),
        });
        let cx: &mut VisualTestContext = cx;
        draw(cx);

        let first_before = cx.debug_bounds("first-strip-tile").unwrap();
        let second_before = cx.debug_bounds("second-strip-tile").unwrap();
        assert_eq!(first_before.origin.x, px(STRIP_INSET + TILE_FRAME));
        cx.simulate_event(ScrollWheelEvent {
            position: first_before.center(),
            delta: ScrollDelta::Pixels(point(px(-120.0), px(0.0))),
            ..Default::default()
        });
        draw(cx);
        assert!(cx.debug_bounds("first-strip-tile").unwrap().origin.x < first_before.origin.x);
        assert_eq!(
            cx.debug_bounds("second-strip-tile").unwrap().origin.x,
            second_before.origin.x
        );
        cx.simulate_event(ScrollWheelEvent {
            position: second_before.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
            ..Default::default()
        });
        draw(cx);
        assert_eq!(
            cx.debug_bounds("second-strip-tile").unwrap().origin.x,
            second_before.origin.x
        );

        cx.simulate_click(second_before.center(), Modifiers::default());
        draw(cx);
        assert_eq!(picked.get(), Some((1, 0)));
        cx.simulate_keystrokes("right");
        assert_eq!(picked.get(), Some((1, 2)));
        cx.simulate_keystrokes("left");
        assert_eq!(picked.get(), Some((1, 0)));

        let first_scrolled = cx.debug_bounds("first-strip-tile").unwrap();
        let first_strip = cx.debug_bounds("first-strip-container").unwrap();
        cx.simulate_click(
            point(first_strip.left() + px(4.0), first_strip.bottom() - px(1.0)),
            Modifiers::default(),
        );
        draw(cx);
        assert!(cx.debug_bounds("first-strip-tile").unwrap().left() > first_scrolled.left());
        assert_eq!(picked.get(), Some((1, 0)));
    }
}
