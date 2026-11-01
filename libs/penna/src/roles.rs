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
pub const TREE_RULE: SurfaceRole = SurfaceRole::new(
    "rule.tree",
    Kind::Rule,
    "Tree guides",
    "The lines that join a tree's children",
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
pub const PHONE_BAR: SurfaceRole = SurfaceRole::new(
    "phone.bar",
    Kind::Bar,
    "Phone bars",
    "The phone's 44pt top bars over pages and sheets",
);
pub const PHONE_PILL_ICON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "phone.pill.icon",
    "Pill icon",
    "The icon tile at the start of the phone pill",
)
.with(P.outline(false));
pub const PHONE_BUTTON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "phone.button",
    "Phone buttons",
    "Icon buttons on phone bars and pills, such as back and close",
);
pub const PHONE_CARD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "phone.card",
    "Phone tiles",
    "Tiles in a phone grid",
)
.with(P.fill(Fill::Tint));
pub const PHONE_ADD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "phone.add",
    "Add tile",
    "The add tile at the end of a phone grid",
)
.with(P.fill(Fill::Tint));
pub const PICKER: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::DIALOG,
    "picker",
    "Pickers",
    "Dialogs that search a list and pick from it, such as a file picker",
);

pub const TAB_WINDOW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "tab.window",
    "Window tabs",
    "One pill per window in the title bar",
)
.with(P.ink(Ink::Rest));
pub const TAB_PAGE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "tab.page",
    "Browser tabs",
    "Tabs in a browser pane",
);
pub const TAB_TRAY: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "tab.tray",
    "Tray tabs",
    "The plan and tasks chips over the composer that open their tray",
);
pub const CHIP_BAR: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "chip.bar",
    "Status chips",
    "The update offer and the host name in the title bar",
)
.with(P.selected(Highlight::Tint))
.with_knobs(Kind::Label.knobs().pressable());
pub const CHIP_FLOATING: SurfaceRole = SurfaceRole::new(
    "chip.floating",
    Kind::Floating,
    "Pane chips",
    "Status over a pane: Dead, Waiting, Sync, Copy mode, the vim mode",
)
.with(P.shadow(Shadow::Contact).selected(Highlight::Tint))
.with_knobs(Kind::Floating.knobs().pressable())
.capsule(true);
pub const CHIP_MARKER: SurfaceRole = SurfaceRole::new(
    "chip.marker",
    Kind::Floating,
    "Pane numbers",
    "The big chips display-panes shows on every pane",
)
.with(P.hover(Highlight::Ring).selected(Highlight::Ring))
.with_knobs(Kind::Floating.knobs().pressable());
pub const CHIP_DRAG: SurfaceRole = SurfaceRole::new(
    "chip.drag",
    Kind::Floating,
    "Drag chips",
    "The chip that follows the pointer while you drag a pane",
)
.capsule(true);
pub const BADGE_ICON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "badge.icon",
    "Blank page icon",
    "The disc behind the globe on a blank browser page",
)
.with(P.shape(Shape::Pill).outline(false));
pub const ROW_NAV: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "row.nav",
    "Sidebar entries",
    "Two-line window rows in the flat sidebar",
)
.with(P.ink(Ink::Rest));
pub const ROW_CURSOR: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "row.cursor",
    "Sidebar cursor",
    "The flat sidebar row the keyboard is on",
)
.with(
    P.outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint)
        .selected(Highlight::Accent),
);
pub const BUTTON_ROW: SurfaceRole = SurfaceRole::new(
    "button.row",
    Kind::Button,
    "Host actions",
    "Connect, Retry and Upgrade on a host in the flat sidebar",
)
.with(P.outline(false).shadow(Shadow::None));
pub const ICON_BUTTON_ROW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "icon-button.row",
    "Row actions",
    "The + and × buttons that show on a hovered row",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const ICON_BUTTON_BAR: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "icon-button.bar",
    "Pane header buttons",
    "Split and the other buttons in pane headers",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const ICON_BUTTON_TOGGLE: SurfaceRole = SurfaceRole::under(
    &ICON_BUTTON_BAR,
    "icon-button.toggle",
    "Flip buttons",
    "The button in the header of a pane running Claude Code or Codex that switches it between the CLI and the chat, tinted while a switch waits for the agent to be idle",
)
.with(P.hover(Highlight::None).selected(Highlight::Tint));
pub const ICON_BUTTON_FLOATING: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "icon-button.floating",
    "Jump to latest",
    "The button over the transcript that scrolls to the end",
)
.with(
    P.fill(Fill::Solid)
        .outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint),
);
pub const BUTTON_SEND: SurfaceRole = SurfaceRole::new(
    "button.send",
    Kind::Button,
    "Composer buttons",
    "Send, stop and + in the chat composer, filled in every theme",
)
.with(
    P.fill(Fill::Solid)
        .outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint)
        .selected(Highlight::Tint),
);
pub const FLOATING_PANE: SurfaceRole = SurfaceRole::new(
    "floating.pane",
    Kind::Floating,
    "Floating panes",
    "tmux floating and popup panes, and the confirm prompt",
)
.with(P.selected(Highlight::Ring))
.with_knobs(SurfaceRole::DIALOG.knobs());
pub const FLOATING_PIP: SurfaceRole = SurfaceRole::under(
    &FLOATING_PANE,
    "floating.pip",
    "PiPs",
    "Floating panes in the GUI: any pane over the window, casting the float shadow",
)
.with(P.shadow(Shadow::Float));
pub const MENU_ITEM_CARD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "menu-item.card",
    "New pane picker",
    "Terminal, Browser and Editor in a freshly split pane",
);
pub const MENU_ITEM_TINT: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "menu-item.tint",
    "Recent pages",
    "Recent pages on a blank browser page",
)
.with(P.fill(Fill::Tint));
pub const TILE_STACK: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "tile.stack",
    "Pane stacks",
    "Stacked pane tiles in flat rows and window tabs, and on the plate they fan out on",
)
.with(
    P.outline(false)
        .shadow(Shadow::None)
        .hover(Highlight::Tint)
        .selected(Highlight::Tint),
)
.capsule(true);
pub const TILE_DROP: SurfaceRole = SurfaceRole::new(
    "tile.drop",
    Kind::Floating,
    "Drop zones",
    "Where a dragged pane will land, and the veil over panes while you drag",
)
.with(
    P.fill(Fill::Tint)
        .shadow(Shadow::None)
        .hover(Highlight::Tint)
        .selected(Highlight::Tint),
)
.with_knobs(Kind::Floating.knobs().pressable());
pub const TILE_MINI: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "tile.mini",
    "Panes in a window tile",
    "The panes drawn where they split inside a window's tile in the phone overview, ringed on the one the window opens to",
)
.with(P.fill(Fill::None).selected(Highlight::Ring));
pub const BAR_PANE: SurfaceRole = SurfaceRole::new(
    "bar.pane",
    Kind::Bar,
    "Pane headers",
    "The title row of terminal panes, and the browser's tabs and toolbar",
)
.with_knobs(Knobs::FIXED);
pub const BUTTON_TITLE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::QUIET_BUTTON,
    "button.title",
    "Pane titles",
    "The title in a pane header: drag it to move the pane, its X closes it",
)
.with(P.hover(Highlight::Tint).selected(Highlight::Tint));
pub const CARD_BUBBLE: SurfaceRole = SurfaceRole::new(
    "card.bubble",
    Kind::Card,
    "Message bubbles",
    "Your messages in an agent chat",
);
pub const CARD_CODE: SurfaceRole = SurfaceRole::new(
    "card.code",
    Kind::Card,
    "Code",
    "Code blocks, diagrams and inline code in messages",
)
.with(P.outline(false).shadow(Shadow::None));
pub const CARD_FIGURE: SurfaceRole =
    SurfaceRole::new("card.figure", Kind::Card, "Tables", "Tables in messages")
        .with(P.fill(Fill::None));
pub const CARD_IMAGE: SurfaceRole = SurfaceRole::new(
    "card.image",
    Kind::Card,
    "Attachments",
    "Image tiles in messages and the composer",
);
pub const FIELD_COMPOSER: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TEXT_AREA,
    "field.composer",
    "Composer",
    "The pill you write to an agent in",
)
.with(P.shape(Shape::Pill));
pub const FIELD_ADDRESS: SurfaceRole = SurfaceRole::new(
    "field.address",
    Kind::Field,
    "Address bar",
    "The URL field of a browser pane",
)
.with(P.fill(Fill::None).outline(false));

pub const RECIPES: [SurfaceRole; 31] = [
    TAB_WINDOW,
    TAB_PAGE,
    TAB_TRAY,
    CHIP_BAR,
    CHIP_FLOATING,
    CHIP_MARKER,
    CHIP_DRAG,
    BADGE_ICON,
    ROW_NAV,
    ROW_CURSOR,
    BUTTON_ROW,
    ICON_BUTTON_ROW,
    ICON_BUTTON_BAR,
    ICON_BUTTON_TOGGLE,
    ICON_BUTTON_FLOATING,
    BUTTON_SEND,
    FLOATING_PANE,
    FLOATING_PIP,
    MENU_ITEM_CARD,
    MENU_ITEM_TINT,
    TILE_STACK,
    TILE_DROP,
    TILE_MINI,
    BAR_PANE,
    BUTTON_TITLE,
    CARD_BUBBLE,
    CARD_CODE,
    CARD_FIGURE,
    CARD_IMAGE,
    FIELD_COMPOSER,
    FIELD_ADDRESS,
];

pub const TONED: [SurfaceRole; 1] = [BUTTON_SEND];

const RETIRED_IDS: [&str; 59] = [
    "agent.status",
    "agent.picker",
    "chat.bubble",
    "chat.bubble.mine",
    "phone.driver-card",
    "drawer.driver",
    "rule.chat",
    "pane.header.lens",
    "titlebar.window-tab",
    "browser.tab",
    "agent.tray",
    "titlebar.status-chip",
    "pane.chip",
    "pane.display-panes",
    "pane.drag-chip",
    "browser.empty-icon",
    "sidebar.entry",
    "sidebar.cursor",
    "sidebar.host-action",
    "sidebar.row-action",
    "agent.action",
    "pane.header.button",
    "pane.header.flip",
    "browser.button",
    "agent.jump",
    "agent.send",
    "pane.float",
    "pane.pip",
    "pane.find-bar",
    "palette",
    "pane.picker",
    "agent.option",
    "browser.suggestion",
    "drawer.hosts",
    "drawer.add-host",
    "drawer.new-pane",
    "drawer.tabs",
    "drawer.bindings",
    "pane-stack",
    "pane.drop-zone",
    "phone.mini",
    "pane.header",
    "pane.header.title",
    "rule.split",
    "agent.bubble",
    "code",
    "figure",
    "agent.attachment",
    "agent.composer",
    "browser.address",
    "picker.notice",
    "board",
    "board.screen",
    "board.chip",
    "board.found",
    "banner",
    "status",
    "computer-card",
    "first-run.step",
];

pub const CHROME: [SurfaceRole; 24] = [
    WINDOW,
    SIDEBAR,
    TITLEBAR,
    PHONE_BAR,
    SETTINGS_CARD,
    SETTINGS_PREVIEW,
    CHROME_BUTTON,
    WINDOW_CONTROLS,
    PHONE_BUTTON,
    SIDEBAR_ROW,
    SETTINGS_TILE,
    PHONE_CARD,
    PHONE_ADD,
    BAR_RULE,
    SIDEBAR_RULE,
    LIST_RULE,
    TREE_RULE,
    INLINE_PANEL,
    PHONE_PILL,
    PHONE_PILL_ICON,
    PHONE_KEY,
    ARROW_PAD,
    PHONE_ROW,
    PICKER,
];

static APP: OnceLock<&'static [SurfaceRole]> = OnceLock::new();
static RETIRED: OnceLock<&'static [&'static str]> = OnceLock::new();

pub fn register(roles: &'static [SurfaceRole]) {
    if APP.get().is_none() {
        check_app_ids(roles);
    }
    set_once(&APP, roles, "penna::roles::register");
}

pub(crate) fn registered() -> bool {
    APP.get().is_some()
}

pub fn retire(ids: &'static [&'static str]) {
    set_once(&RETIRED, ids, "penna::roles::retire");
}

pub(crate) fn set_once<T: PartialEq + ?Sized>(
    slot: &OnceLock<&'static T>,
    value: &'static T,
    caller: &str,
) {
    let set = *slot.get_or_init(|| value);
    assert!(
        std::ptr::eq(set, value) || set == value,
        "{caller} was already called with a different list; register once at startup, \
         and pass the same list if you call it again"
    );
}

fn spelled_like_a_theme_file(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c == '.')
}

fn check_app_ids(app: &[SurfaceRole]) {
    for (index, role) in app.iter().enumerate() {
        let id = role.id;
        assert!(
            spelled_like_a_theme_file(id),
            "app role `{id}` can't be named in a theme file; spell role ids with lowercase \
             letters, `-` and `.`"
        );
        assert!(
            SurfaceRole::KIT.iter().all(|kit| kit.id != id),
            "app role `{id}` takes the id of a penna kit role; give it an id of its own, \
             such as `app.{id}`"
        );
        assert!(
            CHROME
                .iter()
                .all(|chrome| chrome.id != id || chrome == role),
            "app role `{id}` changes a penna chrome role that penna's widgets draw; list \
             penna's role unchanged and restyle it in a theme layer, or give it an id of its \
             own, such as `app.{id}`"
        );
        assert!(
            app[..index].iter().all(|other| other.id != id),
            "app role `{id}` is in the list twice; each role needs its own id"
        );
    }
}

pub fn is_retired(id: &str) -> bool {
    RETIRED_IDS.contains(&id) || RETIRED.get().is_some_and(|ids| ids.contains(&id))
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
        .chain(RECIPES)
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
            assert!(spelled_like_a_theme_file(id), "{id}");
            assert!(!ids[..index].contains(id), "{id} is named twice");
        }
        check_app_ids(&CHROME);
    }

    #[test]
    fn a_list_registers_once_and_again_only_unchanged() {
        static FIRST: [&str; 2] = ["one", "two"];
        static SAME: [&str; 2] = ["one", "two"];
        let cell = OnceLock::new();
        set_once(&cell, &FIRST[..], "register");
        set_once(&cell, &FIRST[..], "register");
        set_once(&cell, &SAME[..], "register");
        let changed = std::panic::catch_unwind(|| set_once(&cell, &FIRST[..1], "register"));
        let message = *changed.unwrap_err().downcast::<String>().unwrap();
        assert!(
            message.starts_with("register was already called with a different list"),
            "{message}"
        );
    }

    #[test]
    fn app_ids_that_clash_or_misspell_are_refused_by_name() {
        let refusal = |roles: &[SurfaceRole]| {
            *std::panic::catch_unwind(|| check_app_ids(roles))
                .unwrap_err()
                .downcast::<String>()
                .unwrap()
        };
        let card = |id| SurfaceRole::new(id, Kind::Card, "Cards", "Cards the app draws");
        let message = refusal(&[card("button")]);
        assert!(
            message.starts_with("app role `button` takes the id of a penna kit role"),
            "{message}"
        );
        let message = refusal(&[card("Note Big")]);
        assert!(
            message.starts_with("app role `Note Big` can't be named in a theme file"),
            "{message}"
        );
        let message = refusal(&[card("note"), card("note")]);
        assert!(message.contains("`note` is in the list twice"), "{message}");
        let message = refusal(&[card("sidebar").with(StylePatch::EMPTY.shadow(Shadow::Contact))]);
        assert!(
            message.starts_with("app role `sidebar` changes a penna chrome role"),
            "{message}"
        );
        check_app_ids(&[card("note"), card("note.big"), WINDOW, SIDEBAR]);
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
