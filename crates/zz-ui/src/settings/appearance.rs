#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzRow {
    TerminalColors,
    ProgramColors,
    AppIcon,
    WindowBackgroundBlur,
    WindowCornerRadius,
    UseSystemTitlebar,
}

impl ZzRow {
    pub const fn title(self) -> &'static str {
        match self {
            Self::TerminalColors => "Terminal colors",
            Self::ProgramColors => "Program colors",
            Self::AppIcon => "App Icon",
            Self::WindowBackgroundBlur => "Window blur",
            Self::WindowCornerRadius => "Window corner radius",
            Self::UseSystemTitlebar => "System titlebar",
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
            AppearancePageItem::App(ZzRow::TerminalColors),
            AppearancePageItem::Group { title: "Sidebar" },
            AppearancePageItem::App(ZzRow::ProgramColors),
        ],
    );
    let window = [
        macos.then_some(ZzRow::AppIcon),
        has_window_blur.then_some(ZzRow::WindowBackgroundBlur),
        cfg!(target_os = "linux").then_some(ZzRow::WindowCornerRadius),
        cfg!(target_os = "linux").then_some(ZzRow::UseSystemTitlebar),
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
            Item::Group { title: "Terminal" },
            Item::App(ZzRow::TerminalColors),
            Item::Group { title: "Sidebar" },
            Item::App(ZzRow::ProgramColors),
            Item::Group { title: "Size" },
            Item::TextSize,
            Item::Density,
            Item::Group {
                title: "Accessibility",
            },
            Item::IncreaseContrast,
            Item::ReduceTransparency,
            Item::Motion,
            Item::Group { title: "Window" },
            Item::App(ZzRow::AppIcon),
            Item::App(ZzRow::WindowBackgroundBlur),
        ];
        if cfg!(target_os = "linux") {
            expected.extend([
                Item::App(ZzRow::WindowCornerRadius),
                Item::App(ZzRow::UseSystemTitlebar),
            ]);
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
