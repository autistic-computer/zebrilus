#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzRow {
    ProgramColors,
    AppIcon,
}

impl ZzRow {
    pub const fn title(self) -> &'static str {
        match self {
            Self::ProgramColors => "Program colors",
            Self::AppIcon => "App Icon",
        }
    }
}

pub type AppearancePageItem = penna::settings::appearance::AppearancePageItem<ZzRow>;

pub fn appearance_page_items(
    macos: bool,
    has_window_blur: bool,
    advanced: bool,
) -> Vec<AppearancePageItem> {
    let mut items = penna::settings::appearance::appearance_page_items(has_window_blur, advanced);
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
    if macos
        && let Some(zoom) = items
            .iter()
            .position(|item| *item == AppearancePageItem::UiZoom)
    {
        items.insert(zoom + 1, AppearancePageItem::App(ZzRow::AppIcon));
    }
    items
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
            Item::WindowBackgroundBlur,
            Item::WindowBackgroundOpacity,
        ];
        #[cfg(target_os = "linux")]
        expected.extend([Item::WindowCornerRadius, Item::UseSystemTitlebar]);
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
    }
}
