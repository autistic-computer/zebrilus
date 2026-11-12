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
    WindowCornerRadius,
    GlassLab,
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
            Self::WindowCornerRadius => "Window corner radius",
            Self::GlassLab => "Glass lab",
        }
    }
}

pub type AppearancePageItem = penna::blocks::appearance::AppearancePageItem<ZzRow>;

pub fn appearance_page_items() -> Vec<AppearancePageItem> {
    use AppearancePageItem as Item;
    vec![
        Item::Title,
        Item::Group { title: "Theme" },
        Item::ThemeMode,
        Item::Accent,
        Item::TintHue,
        Item::Tint,
        Item::Group { title: "Terminal" },
        Item::App(ZzRow::TerminalPreview),
        Item::App(ZzRow::LightTheme),
        Item::App(ZzRow::DarkTheme),
        Item::App(ZzRow::TerminalColors),
        Item::App(ZzRow::BackgroundOpacity),
        Item::Group { title: "Sidebar" },
        Item::App(ZzRow::ProgramColors),
        Item::Group { title: "Contrast" },
        Item::IncreaseContrast,
        Item::App(ZzRow::TerminalContrast),
    ]
}

pub fn interface_page_items(macos: bool) -> Vec<AppearancePageItem> {
    use AppearancePageItem as Item;
    let mut items = vec![
        Item::Title,
        Item::Group { title: "Material" },
        Item::Glass,
        Item::Controls,
        Item::CornerRadius,
        Item::Group { title: "Text" },
        Item::UiFontFamily,
        Item::MonoFontFamily,
        Item::TextSize,
        Item::Density,
        Item::Group { title: "Behavior" },
        Item::Pointer,
        Item::Motion,
    ];
    let window = [
        macos.then_some(ZzRow::AppIcon),
        cfg!(target_os = "linux").then_some(ZzRow::WindowCornerRadius),
    ];
    if window.iter().any(Option::is_some) {
        items.push(Item::Group { title: "Window" });
        items.extend(window.into_iter().flatten().map(Item::App));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_pages_hold_every_kit_row_once() {
        use AppearancePageItem as Item;
        let kit = penna::blocks::appearance::appearance_page_items::<ZzRow>()
            .into_iter()
            .filter(|item| item.is_entry())
            .collect::<Vec<_>>();
        let pages = [appearance_page_items(), interface_page_items(true)].concat();
        for item in &kit {
            assert_eq!(
                pages.iter().filter(|shown| *shown == item).count(),
                1,
                "{item:?}"
            );
        }
        let mut window = vec![Item::Group { title: "Window" }, Item::App(ZzRow::AppIcon)];
        if cfg!(target_os = "linux") {
            window.push(Item::App(ZzRow::WindowCornerRadius));
        }
        assert!(interface_page_items(true).ends_with(&window));
        if !cfg!(target_os = "linux") {
            assert_eq!(interface_page_items(false).last(), Some(&Item::Motion));
        }
    }
}
