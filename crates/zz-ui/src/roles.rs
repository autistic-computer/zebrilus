//! Every distinct piece of zz's UI a theme can style, by region. Each role
//! names the kit kind it is and how its built-in look differs from that
//! kind, so the Default theme draws zz as it always has; a theme file
//! changes any of them by id.

use zz_gpui_kit::{Fill, Highlight, Kind, Shadow, Shape, StylePatch, SurfaceRole};

const P: StylePatch = StylePatch::EMPTY;

pub const WINDOW: SurfaceRole = SurfaceRole::new(
    "window",
    Kind::Panel,
    "Window",
    "The plane everything sits on, solid or blurred",
);
pub const TITLEBAR: SurfaceRole = SurfaceRole::new(
    "titlebar",
    Kind::Bar,
    "Title bar",
    "The strip across the top of the window",
);
pub const CHROME_BUTTON: SurfaceRole = SurfaceRole::new(
    "chrome.button",
    Kind::IconButton,
    "Chrome buttons",
    "Icon buttons on the window chrome: sidebar toggle, settings, list view, search, window overflow",
);
pub const WINDOW_TAB: SurfaceRole = SurfaceRole::new(
    "titlebar.window-tab",
    Kind::Tab,
    "Window tabs",
    "One pill per window in the title bar",
);
pub const STATUS_CHIP: SurfaceRole = SurfaceRole::new(
    "titlebar.status-chip",
    Kind::Chip,
    "Status chips",
    "Running agents, the update offer and the host name in the title bar",
)
.with(P.fill(Fill::Tint).shadow(Shadow::None));
pub const WINDOW_CONTROLS: SurfaceRole = SurfaceRole::new(
    "titlebar.window-controls",
    Kind::IconButton,
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
    "The session sidebar, also when it slides over the panes",
);
pub const SIDEBAR_ROW: SurfaceRole = SurfaceRole::new(
    "sidebar.row",
    Kind::Row,
    "Sidebar rows",
    "Tree rows for hosts, sessions, windows and panes, and the Settings sections",
)
.with(P.hover(Highlight::Lift).selected(Highlight::Lift));
pub const SIDEBAR_ENTRY: SurfaceRole = SurfaceRole::new(
    "sidebar.entry",
    Kind::Row,
    "Sidebar entries",
    "Two-line window rows in the flat sidebar",
)
.with(P.hover(Highlight::Lift).selected(Highlight::Lift));
pub const ROW_ACTION: SurfaceRole = SurfaceRole::new(
    "sidebar.row-action",
    Kind::IconButton,
    "Row actions",
    "The +, ×, … and layout buttons that show on a hovered row",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const PANE_STACK: SurfaceRole = SurfaceRole::new(
    "pane-stack",
    Kind::Tile,
    "Pane stacks",
    "Stacked pane tiles in flat rows and window tabs, and the plate they fan out on",
)
.with(
    P.fill(Fill::Tint)
        .shadow(Shadow::None)
        .selected(Highlight::Tint),
);

pub const PANE: SurfaceRole = Kind::Pane.role();
pub const FLOATING_PANE: SurfaceRole = SurfaceRole::new(
    "pane.float",
    Kind::Pane,
    "Floating panes",
    "tmux floating and popup panes, and the confirm prompt",
)
.with(P.shadow(Shadow::Float));
pub const PANE_HEADER: SurfaceRole = SurfaceRole::new(
    "pane.header",
    Kind::Bar,
    "Pane headers",
    "The title row of terminal and agent panes, and the browser's tabs and toolbar",
);
pub const PANE_HEADER_BUTTON: SurfaceRole = SurfaceRole::new(
    "pane.header.button",
    Kind::IconButton,
    "Pane header buttons",
    "Split, close and drag in pane headers",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const PANE_CHIP: SurfaceRole = SurfaceRole::new(
    "pane.chip",
    Kind::Chip,
    "Pane chips",
    "Status over a pane: Dead, Waiting, Sync, Copy mode, the agent's status, the vim mode",
);
pub const DISPLAY_PANES: SurfaceRole = SurfaceRole::new(
    "pane.display-panes",
    Kind::Chip,
    "Pane numbers",
    "The big chips display-panes shows on every pane",
)
.with(
    P.fill(Fill::Glass)
        .shadow(Shadow::Float)
        .hover(Highlight::Ring)
        .selected(Highlight::Ring),
);
pub const FIND_BAR: SurfaceRole = SurfaceRole::new(
    "pane.find-bar",
    Kind::Popover,
    "Find bars",
    "The search bar at the top of a terminal",
);
pub const DROP_ZONE: SurfaceRole = SurfaceRole::new(
    "pane.drop-zone",
    Kind::Tile,
    "Drop zones",
    "Where a dragged pane will land, and the veil over panes while you drag",
)
.with(P.fill(Fill::Tint).shadow(Shadow::None));
pub const DRAG_CHIP: SurfaceRole = SurfaceRole::new(
    "pane.drag-chip",
    Kind::Chip,
    "Drag chips",
    "The chip that follows the pointer while you drag a pane",
)
.with(P.fill(Fill::Glass).shadow(Shadow::Float));

pub const AGENT_BUBBLE: SurfaceRole = SurfaceRole::new(
    "agent.bubble",
    Kind::Card,
    "Message bubbles",
    "Your messages in the agent pane",
);
pub const AGENT_COMPOSER: SurfaceRole = SurfaceRole::new(
    "agent.composer",
    Kind::Field,
    "Composer",
    "The box you write to the agent in",
);
pub const AGENT_SEND: SurfaceRole = SurfaceRole::new(
    "agent.send",
    Kind::IconButton,
    "Send",
    "Send, queue and stop in the composer",
)
.with(
    P.fill(Fill::Solid)
        .outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint)
        .selected(Highlight::Tint),
);
pub const AGENT_CONTROL: SurfaceRole = SurfaceRole::new(
    "agent.control",
    Kind::Button,
    "Agent controls",
    "Mode, model and folder pickers, and the plan, tasks and queue chips",
)
.with(SurfaceRole::QUIET_BUTTON.patch);
pub const AGENT_ATTACHMENT: SurfaceRole = SurfaceRole::new(
    "agent.attachment",
    Kind::Tile,
    "Attachments",
    "Image tiles in messages and the composer",
);
pub const AGENT_TRACE: SurfaceRole = SurfaceRole::new(
    "agent.trace",
    Kind::Row,
    "Agent steps",
    "Tool and thought rows in an agent's work, and their summary",
)
.with(P.selected(Highlight::Tint));
pub const AGENT_OPTION: SurfaceRole = SurfaceRole::new(
    "agent.option",
    Kind::Tile,
    "Answer options",
    "The choices in question and permission cards",
)
.with(
    P.fill(Fill::Tint)
        .outline(false)
        .shadow(Shadow::None)
        .hover(Highlight::Tint)
        .selected(Highlight::Accent),
);
pub const AGENT_JUMP: SurfaceRole = SurfaceRole::new(
    "agent.jump",
    Kind::IconButton,
    "Jump to latest",
    "The button over the transcript that scrolls to the end",
)
.with(
    P.fill(Fill::Solid)
        .outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint),
);
pub const INLINE_PANEL: SurfaceRole = SurfaceRole::new(
    "inline-panel",
    Kind::Card,
    "Inline panels",
    "Panels raised inside a pane: question cards, the plan and tasks tray, the slash menu, side answers, failure alerts",
)
.with(P.fill(Fill::Glass).shadow(Shadow::Float));

pub const CODE: SurfaceRole = SurfaceRole::new(
    "code",
    Kind::Card,
    "Code",
    "Code blocks and inline code in messages",
)
.with(P.fill(Fill::Tint).outline(false).shadow(Shadow::None));
pub const FIGURE: SurfaceRole = SurfaceRole::new(
    "figure",
    Kind::Card,
    "Figures",
    "Diagram frames and tables in messages",
);

pub const BROWSER_TAB: SurfaceRole = SurfaceRole::new(
    "browser.tab",
    Kind::Tab,
    "Browser tabs",
    "Tabs in a browser pane",
);
pub const BROWSER_BUTTON: SurfaceRole = SurfaceRole::new(
    "browser.button",
    Kind::IconButton,
    "Browser buttons",
    "Back, forward, reload, the picker, more and new tab",
);
pub const ADDRESS_BAR: SurfaceRole = SurfaceRole::new(
    "browser.address",
    Kind::Field,
    "Address bar",
    "The URL field of a browser pane",
)
.with(P.fill(Fill::None).outline(false));
pub const BROWSER_SUGGESTION: SurfaceRole = SurfaceRole::new(
    "browser.suggestion",
    Kind::Row,
    "Recent pages",
    "Recent pages on a blank browser page",
)
.with(P.fill(Fill::Tint));

pub const SETTINGS_CARD: SurfaceRole = SurfaceRole::new(
    "settings.card",
    Kind::Card,
    "Setting groups",
    "The cards that group settings",
);
pub const SETTINGS_ROW: SurfaceRole = SurfaceRole::new(
    "settings.row",
    Kind::Row,
    "Setting rows",
    "One setting in a group, and its press on touch",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const SETTINGS_TILE: SurfaceRole = SurfaceRole::new(
    "settings.tile",
    Kind::Tile,
    "Picker tiles",
    "Theme, mode and app icon tiles in Settings",
)
.with(
    P.fill(Fill::None)
        .outline(false)
        .shadow(Shadow::None)
        .selected(Highlight::Ring),
);
pub const SETTINGS_PREVIEW: SurfaceRole = SurfaceRole::new(
    "settings.preview",
    Kind::Card,
    "Previews",
    "The pane, title bar, terminal and config previews in Settings",
)
.with(P.shadow(Shadow::None));

pub const PHONE_BAR: SurfaceRole = SurfaceRole::new(
    "phone.bar",
    Kind::Bar,
    "Phone bars",
    "The phone's bottom bar, the key strip and the 44pt top bars",
)
.with(P.fill(Fill::Solid));
pub const PHONE_PILL: SurfaceRole = SurfaceRole::new(
    "phone.pill",
    Kind::Button,
    "Phone pill",
    "The pane pill in the phone's bottom bar",
)
.with(P.fill(Fill::Glass));
pub const PHONE_BUTTON: SurfaceRole = SurfaceRole::new(
    "phone.button",
    Kind::IconButton,
    "Phone buttons",
    "Back, close, settings and keyboard on the phone",
);
pub const PHONE_KEY: SurfaceRole = SurfaceRole::new(
    "phone.key",
    Kind::Keycap,
    "Keys",
    "Keys in the phone's key row and the arrow pad",
)
.with(P.fill(Fill::Solid).shadow(Shadow::Contact));
pub const PHONE_CARD: SurfaceRole = SurfaceRole::new(
    "phone.card",
    Kind::Tile,
    "Pane cards",
    "Pane thumbnails in the phone overview, and the New pane tile",
);
pub const PHONE_ROW: SurfaceRole = SurfaceRole::new(
    "phone.row",
    Kind::Row,
    "Touch rows",
    "Rows in phone sheets, the bindings list and the Settings index",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const GRABBER: SurfaceRole = SurfaceRole::new(
    "grabber",
    Kind::Badge,
    "Grabbers",
    "The handles on the phone bar and on sheets",
)
.with(P.shape(Shape::Pill).outline(false));

pub const PALETTE: SurfaceRole = SurfaceRole::new(
    "palette",
    Kind::Popover,
    "Command palette",
    "The command palette and the path picker",
);
pub const PICKER: SurfaceRole = SurfaceRole::new(
    "picker",
    Kind::Dialog,
    "Pickers",
    "Agent history, the file picker, choose-tree and choose-buffer",
);

pub const BAR_RULE: SurfaceRole = SurfaceRole::new(
    "rule.bar",
    Kind::Rule,
    "Bar dividers",
    "Lines under the title bar and pane headers, over the phone bar and footers",
);
pub const SIDEBAR_RULE: SurfaceRole = SurfaceRole::new(
    "rule.sidebar",
    Kind::Rule,
    "Sidebar edge",
    "The line between the sidebar and the panes",
);
pub const SPLIT_RULE: SurfaceRole = SurfaceRole::new(
    "rule.split",
    Kind::Rule,
    "Split lines",
    "Lines between panes when they have no gaps",
);
pub const LIST_RULE: SurfaceRole = SurfaceRole::new(
    "rule.list",
    Kind::Rule,
    "List dividers",
    "Lines between menu groups, setting rows and question cards",
);
pub const TREE_RULE: SurfaceRole = SurfaceRole::new(
    "rule.tree",
    Kind::Rule,
    "Tree guides",
    "The lines that join a tree's children",
);

/// The roles by region, in the order the theme creator lists them.
pub const REGIONS: &[(&str, &[SurfaceRole])] = &[
    (
        "Window and title bar",
        &[
            WINDOW,
            TITLEBAR,
            CHROME_BUTTON,
            WINDOW_TAB,
            STATUS_CHIP,
            WINDOW_CONTROLS,
        ],
    ),
    (
        "Sidebar",
        &[SIDEBAR, SIDEBAR_ROW, SIDEBAR_ENTRY, ROW_ACTION, PANE_STACK],
    ),
    (
        "Panes",
        &[
            PANE,
            FLOATING_PANE,
            PANE_HEADER,
            PANE_HEADER_BUTTON,
            PANE_CHIP,
            DISPLAY_PANES,
            FIND_BAR,
            DROP_ZONE,
            DRAG_CHIP,
        ],
    ),
    (
        "Agent",
        &[
            AGENT_BUBBLE,
            AGENT_COMPOSER,
            AGENT_SEND,
            AGENT_CONTROL,
            AGENT_ATTACHMENT,
            AGENT_TRACE,
            AGENT_OPTION,
            AGENT_JUMP,
            INLINE_PANEL,
        ],
    ),
    ("Reading", &[CODE, FIGURE]),
    (
        "Browser",
        &[BROWSER_TAB, BROWSER_BUTTON, ADDRESS_BAR, BROWSER_SUGGESTION],
    ),
    (
        "Settings",
        &[
            SETTINGS_CARD,
            SETTINGS_ROW,
            SETTINGS_TILE,
            SETTINGS_PREVIEW,
        ],
    ),
    (
        "Phone",
        &[
            PHONE_BAR,
            PHONE_PILL,
            PHONE_BUTTON,
            PHONE_KEY,
            PHONE_CARD,
            PHONE_ROW,
            GRABBER,
        ],
    ),
    ("Overlays", &[PALETTE, PICKER]),
    (
        "Dividers",
        &[BAR_RULE, SIDEBAR_RULE, SPLIT_RULE, LIST_RULE, TREE_RULE],
    ),
    ("Kinds", &KINDS),
];

/// The kit's roles other than panes, which the Panes region lists.
const KINDS: [SurfaceRole; 19] = [
    Kind::Bar.role(),
    Kind::Panel.role(),
    Kind::Card.role(),
    Kind::Tile.role(),
    Kind::Row.role(),
    Kind::Tab.role(),
    Kind::Chip.role(),
    Kind::Button.role(),
    SurfaceRole::QUIET_BUTTON,
    Kind::IconButton.role(),
    Kind::Field.role(),
    Kind::Badge.role(),
    Kind::Keycap.role(),
    Kind::Popover.role(),
    Kind::Dialog.role(),
    Kind::Sheet.role(),
    Kind::Toast.role(),
    Kind::Tooltip.role(),
    Kind::Rule.role(),
];

/// Every role a theme file may name.
pub fn all() -> impl Iterator<Item = SurfaceRole> {
    REGIONS.iter().flat_map(|(_, roles)| roles.iter().copied())
}

/// The role a theme file calls `id`.
pub fn find(id: &str) -> Option<SurfaceRole> {
    all().find(|role| role.id == id)
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
    fn every_kind_is_a_role_and_places_are_not_kinds() {
        for kind in Kind::ALL {
            assert_eq!(find(kind.name()), Some(kind.role()));
        }
        for role in SurfaceRole::KIT {
            assert_eq!(find(role.id), Some(role));
        }
        for (region, roles) in REGIONS {
            if *region == "Kinds" {
                continue;
            }
            for role in *roles {
                assert!(!role.is_kind() || *role == PANE, "{} is a kind", role.id);
                assert!(!role.label.is_empty() && !role.about.is_empty());
            }
        }
    }
}
