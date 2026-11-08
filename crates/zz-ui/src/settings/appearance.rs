#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzRow {
    TerminalPreview,
    LightTheme,
    DarkTheme,
    TerminalColors,
    BackgroundOpacity,
    TerminalContrast,
    ProgramColors,
    AppIcon,
    WindowBackgroundBlur,
    WindowCornerRadius,
}

impl ZzRow {
    pub const fn title(self) -> &'static str {
        match self {
            Self::TerminalPreview => "Preview",
            Self::LightTheme => "Light theme",
            Self::DarkTheme => "Dark theme",
            Self::TerminalColors => "Window follows the theme",
            Self::BackgroundOpacity => "Background opacity (%)",
            Self::TerminalContrast => "Terminal text contrast",
            Self::ProgramColors => "Program colors",
            Self::AppIcon => "App Icon",
            Self::WindowBackgroundBlur => "Window blur",
            Self::WindowCornerRadius => "Window corner radius",
        }
    }
}

pub type AppearancePageItem = penna::blocks::appearance::AppearancePageItem<ZzRow>;

pub fn appearance_page_items(macos: bool, has_window_blur: bool) -> Vec<AppearancePageItem> {
    let mut items = penna::blocks::appearance::appearance_page_items();
    let size = items
        .iter()
        .position(|item| {
            *item
                == AppearancePageItem::Group {
                    title: penna::blocks::appearance::SIZE,
                }
        })
        .unwrap_or(items.len());
    items.splice(
        size..size,
        [
            AppearancePageItem::Group { title: "Terminal" },
            AppearancePageItem::App(ZzRow::TerminalPreview),
            AppearancePageItem::App(ZzRow::LightTheme),
            AppearancePageItem::App(ZzRow::DarkTheme),
            AppearancePageItem::App(ZzRow::TerminalColors),
            AppearancePageItem::App(ZzRow::BackgroundOpacity),
            AppearancePageItem::Group { title: "Sidebar" },
            AppearancePageItem::App(ZzRow::ProgramColors),
        ],
    );
    if let Some(contrast) = items
        .iter()
        .position(|item| *item == AppearancePageItem::IncreaseContrast)
    {
        items.insert(
            contrast + 1,
            AppearancePageItem::App(ZzRow::TerminalContrast),
        );
    }
    let window = [
        macos.then_some(ZzRow::AppIcon),
        has_window_blur.then_some(ZzRow::WindowBackgroundBlur),
        cfg!(target_os = "linux").then_some(ZzRow::WindowCornerRadius),
    ];
    if window.iter().any(Option::is_some) {
        items.push(AppearancePageItem::Group { title: "Window" });
        items.extend(window.into_iter().flatten().map(AppearancePageItem::App));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zz_adds_sidebar_and_window_groups_around_the_kit_groups() {
        use AppearancePageItem as Item;
        let mut expected = vec![
            Item::Title,
            Item::Group { title: "Look" },
            Item::ThemeMode,
            Item::Accent,
            Item::TintHue,
            Item::Tint,
            Item::CornerRadius,
            Item::Controls,
            Item::Glass,
            Item::UiFontFamily,
            Item::MonoFontFamily,
            Item::Group { title: "Terminal" },
            Item::App(ZzRow::TerminalPreview),
            Item::App(ZzRow::LightTheme),
            Item::App(ZzRow::DarkTheme),
            Item::App(ZzRow::TerminalColors),
            Item::App(ZzRow::BackgroundOpacity),
            Item::Group { title: "Sidebar" },
            Item::App(ZzRow::ProgramColors),
            Item::Group { title: "Size" },
            Item::TextSize,
            Item::Density,
            Item::Group {
                title: "Accessibility",
            },
            Item::IncreaseContrast,
            Item::App(ZzRow::TerminalContrast),
            Item::ReduceTransparency,
            Item::Motion,
            Item::Group { title: "Window" },
            Item::App(ZzRow::AppIcon),
            Item::App(ZzRow::WindowBackgroundBlur),
        ];
        if cfg!(target_os = "linux") {
            expected.push(Item::App(ZzRow::WindowCornerRadius));
        }
        assert_eq!(appearance_page_items(true, true), expected);

        expected.retain(|item| *item != Item::App(ZzRow::AppIcon));
        assert_eq!(appearance_page_items(false, true), expected);

        if !cfg!(target_os = "linux") {
            let window = expected.len() - 2;
            expected.truncate(window);
            assert_eq!(appearance_page_items(false, false), expected);
        }
    }
}
