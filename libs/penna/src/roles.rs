use std::sync::OnceLock;

use crate::{Fill, Highlight, Ink, Kind, Knobs, Shadow, Shape, StylePatch, SurfaceRole};

const P: StylePatch = StylePatch::EMPTY;

pub const WINDOW: SurfaceRole = SurfaceRole::new(
    "window",
    Kind::Panel,
    "Window",
    "The edge and drop shadow of the window frame the app draws on Linux",
)
.with(P.outline(true).shadow(Shadow::Float))
.with_knobs(Knobs::FIXED);
pub const TITLEBAR: SurfaceRole = SurfaceRole::new(
    "titlebar",
    Kind::Bar,
    "Title bar",
    "The strip across the top of the window",
)
.with_knobs(Knobs::FIXED);
pub const CHROME_BUTTON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "chrome.button",
    "Chrome buttons",
    "Icon buttons on the window chrome, such as the sidebar toggle and settings",
)
.with(P.ink(Ink::Rest));
pub const WINDOW_CONTROLS: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "titlebar.window-controls",
    "Window controls",
    "Minimize, maximize and close on Windows and Linux",
)
.with(
    P.shape(Shape::Square)
        .hover(Highlight::Tint)
        .selected(Highlight::None),
);
pub const SIDEBAR: SurfaceRole = SurfaceRole::new(
    "sidebar",
    Kind::Panel,
    "Sidebar",
    "The navigation column at the window's leading edge, also when it slides over the content",
)
.with_knobs(Knobs::FIXED);
pub const SIDEBAR_ROW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "sidebar.row",
    "Sidebar rows",
    "Rows in the sidebar, and the Settings sections",
)
.with(P.ink(Ink::Rest));
pub const SETTINGS_CARD: SurfaceRole = SurfaceRole::new(
    "settings.card",
    Kind::Card,
    "Setting groups",
    "The cards that group settings",
);
pub const SETTINGS_PREVIEW: SurfaceRole = SurfaceRole::new(
    "settings.preview",
    Kind::Card,
    "Previews",
    "Wells in Settings that show what a setting draws",
)
.with(P.shadow(Shadow::None));
pub const SETTINGS_TILE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "settings.tile",
    "Picker tiles",
    "Pickable tiles in Settings, such as the theme and mode tiles",
)
.with(P.fill(Fill::None).outline(false).shadow(Shadow::None));
pub const BAR_RULE: SurfaceRole = SurfaceRole::new(
    "rule.bar",
    Kind::Rule,
    "Bar dividers",
    "Lines under the title bar and other bars, and over footers",
);
pub const SIDEBAR_RULE: SurfaceRole = SurfaceRole::new(
    "rule.sidebar",
    Kind::Rule,
    "Sidebar edge",
    "The line between the sidebar and the content",
);
pub const LIST_RULE: SurfaceRole = SurfaceRole::new(
    "rule.list",
    Kind::Rule,
    "List dividers",
    "Lines between menu groups, setting rows and stacked cards",
);

pub const INLINE_PANEL: SurfaceRole = SurfaceRole::new(
    "inline-panel",
    Kind::Floating,
    "Inline panels",
    "Panels raised inside a view, such as question cards, menus and failure alerts",
)
.with_knobs(SurfaceRole::DIALOG.knobs());
pub const PHONE_PILL: SurfaceRole = SurfaceRole::new(
    "phone.pill",
    Kind::Button,
    "Phone pill",
    "Pills floating at the bottom of a phone screen, such as the ones the keys sit in over the keyboard",
);
pub const PHONE_KEY: SurfaceRole = SurfaceRole::new(
    "phone.key",
    Kind::Button,
    "Keys",
    "Keys in the pill over the phone's keyboard",
)
.with(P.fill(Fill::None).outline(false).shadow(Shadow::None));
pub const ARROW_PAD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "phone.arrow-pad",
    "Arrow pad",
    "The directions in the phone's arrow pad, lit while held",
);
pub const PHONE_ROW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "phone.row",
    "Touch rows",
    "Rows in phone drawers and touch lists",
)
.with(P.selected(Highlight::None));
pub const PICKER: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::DIALOG,
    "picker",
    "Pickers",
    "Dialogs that search a list and pick from it, such as a file picker",
);

pub const CHROME: [SurfaceRole; 18] = [
    WINDOW,
    SIDEBAR,
    TITLEBAR,
    SETTINGS_CARD,
    SETTINGS_PREVIEW,
    CHROME_BUTTON,
    WINDOW_CONTROLS,
    SIDEBAR_ROW,
    SETTINGS_TILE,
    BAR_RULE,
    SIDEBAR_RULE,
    LIST_RULE,
    INLINE_PANEL,
    PHONE_PILL,
    PHONE_KEY,
    ARROW_PAD,
    PHONE_ROW,
    PICKER,
];

static APP: OnceLock<&'static [SurfaceRole]> = OnceLock::new();
static RETIRED: OnceLock<&'static [&'static str]> = OnceLock::new();

pub fn register(roles: &'static [SurfaceRole]) {
    let _ = APP.set(roles);
}

pub fn retire(ids: &'static [&'static str]) {
    let _ = RETIRED.set(ids);
}

pub fn is_retired(id: &str) -> bool {
    RETIRED.get().is_some_and(|ids| ids.contains(&id))
}

pub fn all() -> impl Iterator<Item = SurfaceRole> {
    let app = APP.get().copied().unwrap_or_default();
    SurfaceRole::KIT
        .into_iter()
        .chain(
            CHROME
                .into_iter()
                .filter(|role| !app.iter().any(|placed| placed.id == role.id)),
        )
        .chain(app.iter().copied())
}

pub fn find(id: &str) -> Option<SurfaceRole> {
    all().find(|role| role.id == id)
}

pub fn members(group: SurfaceRole) -> impl Iterator<Item = SurfaceRole> {
    all().filter(move |role| role.group() == Some(group))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_spelled_like_a_theme_file() {
        let ids = all().map(|role| role.id).collect::<Vec<_>>();
        for (index, id) in ids.iter().enumerate() {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c == '.'),
                "{id}"
            );
            assert!(!ids[..index].contains(id), "{id} is named twice");
        }
    }

    #[test]
    fn every_role_hangs_from_its_kind_through_known_groups() {
        for kind in Kind::ALL {
            assert_eq!(find(kind.name()), Some(kind.role()));
        }
        for role in all() {
            assert!(
                !role.label.is_empty() && !role.about.is_empty(),
                "{}",
                role.id
            );
            for group in role.lineage().skip(1) {
                assert_eq!(
                    find(group.id),
                    Some(group),
                    "{} is in an unknown group",
                    role.id
                );
                assert_eq!(group.kind, role.kind, "{}", role.id);
            }
            assert_eq!(role.lineage().last(), Some(role.kind.role()));
        }
        for role in CHROME {
            assert!(!role.is_kind(), "{} is a kind", role.id);
        }
    }

    #[test]
    fn members_are_the_roles_one_step_under_a_group() {
        let icon_buttons = members(SurfaceRole::ICON_BUTTON).collect::<Vec<_>>();
        assert!(icon_buttons.contains(&CHROME_BUTTON));
        assert!(icon_buttons.contains(&WINDOW_CONTROLS));
        assert!(members(CHROME_BUTTON).next().is_none());
    }
}
