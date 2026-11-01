pub use penna::roles::{
    ARROW_PAD, BADGE_ICON, BAR_PANE, BAR_RULE, BUTTON_ROW, BUTTON_SEND, BUTTON_TITLE, CARD_BUBBLE,
    CARD_CODE, CARD_FIGURE, CARD_IMAGE, CHIP_BAR, CHIP_DRAG, CHIP_FLOATING, CHIP_MARKER,
    CHROME_BUTTON, FIELD_ADDRESS, FIELD_COMPOSER, FLOATING_PANE, FLOATING_PIP, ICON_BUTTON_BAR,
    ICON_BUTTON_FLOATING, ICON_BUTTON_ROW, ICON_BUTTON_TOGGLE, INLINE_PANEL, LIST_RULE,
    MENU_ITEM_CARD, MENU_ITEM_TINT, PHONE_ADD, PHONE_BAR, PHONE_BUTTON, PHONE_CARD, PHONE_KEY,
    PHONE_PILL, PHONE_PILL_ICON, PHONE_ROW, PICKER, ROW_CURSOR, ROW_NAV, SETTINGS_CARD,
    SETTINGS_PREVIEW, SETTINGS_TILE, SIDEBAR, SIDEBAR_ROW, SIDEBAR_RULE, TAB_PAGE, TAB_TRAY,
    TAB_WINDOW, TILE_DROP, TILE_MINI, TILE_STACK, TITLEBAR, TREE_RULE, WINDOW, WINDOW_CONTROLS,
};
use penna::{Kind, SurfaceRole};

pub const PANE: SurfaceRole = Kind::Pane.role();

pub fn all() -> impl Iterator<Item = SurfaceRole> {
    penna::roles::all()
}

pub fn find(id: &str) -> Option<SurfaceRole> {
    penna::roles::find(id)
}

pub fn members(group: SurfaceRole) -> impl Iterator<Item = SurfaceRole> {
    penna::roles::members(group)
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
    }

    #[test]
    fn every_role_gets_a_css_rule_in_every_builtin_theme() {
        for (id, theme) in crate::interface_theme::builtin_themes() {
            for mode in [crate::ThemeMode::Light, crate::ThemeMode::Dark] {
                let kit = penna::theme_css::kit_theme(theme, mode);
                let css = penna::theme_css::stylesheet(&kit, &penna::roles::TONED);
                for role in all() {
                    let selector = format!("[data-penna=\"{}\"]", role.id);
                    assert!(css.contains(&selector), "{id} {mode:?} {}", role.id);
                }
                assert!(!css.contains("NaN"), "{id} {mode:?}");
            }
        }
    }
}
