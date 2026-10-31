use ala::{App, Window};
use penna::{settings::appearance::value_slider, slider::Slider};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzRow {
    ProgramColors,
    AppIcon,
    WindowBackgroundBlur,
    WindowBackgroundOpacity,
    WindowCornerRadius,
    UseSystemTitlebar,
}

impl ZzRow {
    pub const fn title(self) -> &'static str {
        match self {
            Self::ProgramColors => "Program colors",
            Self::AppIcon => "App Icon",
            Self::WindowBackgroundBlur => "Window blur",
            Self::WindowBackgroundOpacity => "Window blur opacity",
            Self::WindowCornerRadius => "Window corner radius",
            Self::UseSystemTitlebar => "System titlebar",
        }
    }
}

pub type AppearancePageItem = penna::settings::appearance::AppearancePageItem<ZzRow>;

pub fn appearance_page_items(
    macos: bool,
    has_window_blur: bool,
    advanced: bool,
) -> Vec<AppearancePageItem> {
    let mut items = penna::settings::appearance::appearance_page_items(advanced);
    let sidebar = items
        .iter()
        .position(|item| matches!(item, AppearancePageItem::Advanced { .. }))
        .unwrap_or(items.len());
    items.splice(
        sidebar..sidebar,
        [
            AppearancePageItem::Group { title: "Sidebar" },
            AppearancePageItem::App(ZzRow::ProgramColors),
        ],
    );
    if !advanced {
        return items;
    }
    if macos
        && let Some(zoom) = items
            .iter()
            .position(|item| *item == AppearancePageItem::UiZoom)
    {
        items.insert(zoom + 1, AppearancePageItem::App(ZzRow::AppIcon));
    }
    if has_window_blur {
        items.extend([
            AppearancePageItem::App(ZzRow::WindowBackgroundBlur),
            AppearancePageItem::App(ZzRow::WindowBackgroundOpacity),
        ]);
    }
    if cfg!(target_os = "linux") {
        items.extend([
            AppearancePageItem::App(ZzRow::WindowCornerRadius),
            AppearancePageItem::App(ZzRow::UseSystemTitlebar),
        ]);
    }
    items
}

pub fn window_opacity_slider(
    opacity: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    value_slider(
        Slider::new(
            "settings-window-opacity",
            ZzRow::WindowBackgroundOpacity.title(),
            opacity,
            0.0..=1.0,
            on_change,
        )
        .step(0.01)
        .display(|opacity| format!("{:.0}%", opacity * 100.0).into()),
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zz_adds_the_sidebar_group_and_the_app_icon_to_the_kit_page() {
        use AppearancePageItem as Item;
        let mut expected = vec![
            Item::Title,
            Item::Group { title: "Theme" },
            Item::ThemeMode,
            Item::Themes,
            Item::Accent,
            Item::TintHue,
            Item::Tint,
            Item::Group { title: "Sidebar" },
            Item::App(ZzRow::ProgramColors),
            Item::Advanced { expanded: true },
            Item::CornerRadius,
            Item::ChromeContrast,
            Item::UiFontFamily,
            Item::UiZoom,
            Item::App(ZzRow::AppIcon),
            Item::Animations,
            Item::AnimationSpeed,
            Item::App(ZzRow::WindowBackgroundBlur),
            Item::App(ZzRow::WindowBackgroundOpacity),
        ];
        if cfg!(target_os = "linux") {
            expected.extend([
                Item::App(ZzRow::WindowCornerRadius),
                Item::App(ZzRow::UseSystemTitlebar),
            ]);
        }
        assert_eq!(appearance_page_items(true, true, true), expected);

        expected.retain(|item| *item != Item::App(ZzRow::AppIcon));
        assert_eq!(appearance_page_items(false, true, true), expected);

        let advanced = expected
            .iter()
            .position(|item| *item == Item::Advanced { expanded: true })
            .unwrap_or_default();
        expected.truncate(advanced);
        expected.push(Item::Advanced { expanded: false });
        assert_eq!(appearance_page_items(true, true, false), expected);

        let without_blur = appearance_page_items(false, false, true);
        assert!(!without_blur.contains(&Item::App(ZzRow::WindowBackgroundBlur)));
        assert!(!without_blur.contains(&Item::App(ZzRow::WindowBackgroundOpacity)));
    }
}
