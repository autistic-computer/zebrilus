//! Every distinct piece of zz's UI a theme can style. Each role names the
//! group it belongs to, a kit kind or a group within one, and how its
//! built-in look differs from that group's, so the Default theme draws zz
//! as it always has; a theme file changes any of them by id.

use zz_gpui_kit::{Fill, Highlight, Kind, Knobs, Shadow, Shape, StylePatch, SurfaceRole};

const P: StylePatch = StylePatch::EMPTY;

pub const WINDOW: SurfaceRole = SurfaceRole::new(
    "window",
    Kind::Panel,
    "Window",
    "The edge and drop shadow of the window frame zz draws on Linux",
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
    "Icon buttons on the window chrome: sidebar toggle, settings, list view, search, window overflow",
);
pub const WINDOW_TAB: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "titlebar.window-tab",
    "Window tabs",
    "One pill per window in the title bar",
);
pub const STATUS_CHIP: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "titlebar.status-chip",
    "Status chips",
    "Running agents, the update offer and the host name in the title bar",
)
.with(P.selected(Highlight::Tint))
.with_knobs(Kind::Label.knobs().pressable());
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
    "The session sidebar, also when it slides over the panes",
)
.with_knobs(Knobs::FIXED);
pub const SIDEBAR_ROW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "sidebar.row",
    "Sidebar rows",
    "Tree rows for hosts, sessions, windows and panes, and the Settings sections",
);
pub const SIDEBAR_ENTRY: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "sidebar.entry",
    "Sidebar entries",
    "Two-line window rows in the flat sidebar",
);
pub const SIDEBAR_CURSOR: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ROW,
    "sidebar.cursor",
    "Sidebar cursor",
    "The flat sidebar row the keyboard is on",
)
.with(
    P.outline(true)
        .shadow(Shadow::Contact)
        .hover(Highlight::Tint)
        .selected(Highlight::Accent),
);
pub const HOST_ACTION: SurfaceRole = SurfaceRole::new(
    "sidebar.host-action",
    Kind::Button,
    "Host actions",
    "Connect, Retry and Upgrade on a host in the flat sidebar",
)
.with(P.outline(false).shadow(Shadow::None));
pub const ROW_ACTION: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "sidebar.row-action",
    "Row actions",
    "The +, ×, … and layout buttons that show on a hovered row",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const PANE_STACK: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "pane-stack",
    "Pane stacks",
    "Stacked pane tiles in flat rows and window tabs, and on the plate they fan out on",
)
.with(
    P.shadow(Shadow::None)
        .hover(Highlight::Tint)
        .selected(Highlight::Tint),
)
.capsule(true);

pub const PANE: SurfaceRole = Kind::Pane.role();
pub const FLOATING_PANE: SurfaceRole = SurfaceRole::new(
    "pane.float",
    Kind::Floating,
    "Floating panes",
    "tmux floating and popup panes, and the confirm prompt",
)
.with(P.selected(Highlight::Ring))
.with_knobs(SurfaceRole::DIALOG.knobs());
pub const PANE_HEADER: SurfaceRole = SurfaceRole::new(
    "pane.header",
    Kind::Bar,
    "Pane headers",
    "The title row of terminal and agent panes, and the browser's tabs and toolbar",
)
.with_knobs(Knobs::FIXED);
pub const PANE_HEADER_BUTTON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "pane.header.button",
    "Pane header buttons",
    "Split, close and drag in pane headers",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const PANE_CHIP: SurfaceRole = SurfaceRole::new(
    "pane.chip",
    Kind::Floating,
    "Pane chips",
    "Status over a pane: Dead, Waiting, Sync, Copy mode, the agent's status, the vim mode",
)
.with(P.shadow(Shadow::Contact).selected(Highlight::Tint))
.with_knobs(Kind::Floating.knobs().pressable())
.capsule(true);
pub const DISPLAY_PANES: SurfaceRole = SurfaceRole::new(
    "pane.display-panes",
    Kind::Floating,
    "Pane numbers",
    "The big chips display-panes shows on every pane",
)
.with(P.hover(Highlight::Ring).selected(Highlight::Ring))
.with_knobs(Kind::Floating.knobs().pressable());
pub const FIND_BAR: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::POPOVER,
    "pane.find-bar",
    "Find bars",
    "The search bar at the top of a terminal",
);
pub const DROP_ZONE: SurfaceRole = SurfaceRole::new(
    "pane.drop-zone",
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
pub const DRAG_CHIP: SurfaceRole = SurfaceRole::new(
    "pane.drag-chip",
    Kind::Floating,
    "Drag chips",
    "The chip that follows the pointer while you drag a pane",
)
.capsule(true);

pub const AGENT_BUBBLE: SurfaceRole = SurfaceRole::new(
    "agent.bubble",
    Kind::Card,
    "Message bubbles",
    "Your messages in the agent pane",
);
pub const AGENT_COMPOSER: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TEXT_AREA,
    "agent.composer",
    "Composer",
    "The box you write to the agent in",
);
pub const AGENT_SEND: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "agent.send",
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
pub const AGENT_PICKER: SurfaceRole = SurfaceRole::new(
    "agent.picker",
    Kind::Field,
    "Agent pickers",
    "The mode, model and folder pickers under the composer",
)
.with(
    P.fill(Fill::None)
        .outline(false)
        .hover(Highlight::Lift)
        .selected(Highlight::Lift),
);
pub const AGENT_TRAY: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "agent.tray",
    "Tray tabs",
    "The plan and tasks chips over the composer that open their tray",
);
pub const AGENT_ATTACHMENT: SurfaceRole = SurfaceRole::new(
    "agent.attachment",
    Kind::Card,
    "Attachments",
    "Image tiles in messages and the composer",
);
pub const AGENT_ACTION: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "agent.action",
    "Message actions",
    "Copy, rewind and open on messages, code blocks and diagrams, shown under the pointer",
)
.with(P.hover(Highlight::None).selected(Highlight::None));
pub const AGENT_OPTION: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "agent.option",
    "Answer options",
    "The choices in question and permission cards",
);
pub const AGENT_JUMP: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "agent.jump",
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
    Kind::Floating,
    "Inline panels",
    "Panels raised inside a pane: question cards, the plan and tasks tray, the slash menu, side answers, failure alerts",
)
.with_knobs(SurfaceRole::DIALOG.knobs());

pub const CODE: SurfaceRole = SurfaceRole::new(
    "code",
    Kind::Card,
    "Code",
    "Code blocks and inline code in messages",
)
.with(P.outline(false).shadow(Shadow::None));
pub const FIGURE: SurfaceRole = SurfaceRole::new(
    "figure",
    Kind::Card,
    "Figures",
    "Diagram frames and tables in messages",
)
.with(P.fill(Fill::None));

pub const BROWSER_TAB: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TAB,
    "browser.tab",
    "Browser tabs",
    "Tabs in a browser pane",
);
pub const BROWSER_BUTTON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::ICON_BUTTON,
    "browser.button",
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
pub const BROWSER_SUGGESTION: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "browser.suggestion",
    "Recent pages",
    "Recent pages on a blank browser page",
)
.with(P.fill(Fill::Tint));
pub const BROWSER_EMPTY_ICON: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "browser.empty-icon",
    "Blank page icon",
    "The disc behind the globe on a blank browser page",
)
.with(P.shape(Shape::Pill).outline(false));

pub const SETTINGS_CARD: SurfaceRole = SurfaceRole::new(
    "settings.card",
    Kind::Card,
    "Setting groups",
    "The cards that group settings",
);
pub const SETTINGS_TILE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "settings.tile",
    "Picker tiles",
    "Theme, mode and app icon tiles in Settings",
)
.with(P.fill(Fill::None).outline(false).shadow(Shadow::None));
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
);
pub const PHONE_PILL: SurfaceRole = SurfaceRole::new(
    "phone.pill",
    Kind::Button,
    "Phone pill",
    "The pane pill in the phone's bottom bar",
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
    "Back, close, settings and keyboard on the phone",
);
pub const PHONE_KEY: SurfaceRole = SurfaceRole::new(
    "phone.key",
    Kind::Button,
    "Keys",
    "Keys in the phone's key row",
);
pub const ARROW_PAD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "phone.arrow-pad",
    "Arrow pad",
    "The directions in the phone's arrow pad, lit while held",
);
pub const PHONE_CARD: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::TILE,
    "phone.card",
    "Pane cards",
    "Pane thumbnails in the phone overview, and the New pane tile",
)
.with(P.fill(Fill::Tint));
pub const PHONE_ROW: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::MENU_ITEM,
    "phone.row",
    "Touch rows",
    "Rows in phone sheets, the bindings list and the Settings index",
)
.with(P.selected(Highlight::None));
pub const GRABBER: SurfaceRole = SurfaceRole::GRABBER;

pub const PALETTE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::POPOVER,
    "palette",
    "Command palette",
    "The command palette and the path picker",
);
pub const PICKER: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::DIALOG,
    "picker",
    "Pickers",
    "Agent history, the file picker, choose-tree and choose-buffer",
);
pub const PICKER_NOTICE: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "picker.notice",
    "Picker notices",
    "Strips across a picker that ask before something happens, like deleting a session",
)
.with(P.outline(false).shape(Shape::Square));

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

/// zz's own roles, after the kit's. The theme creator lists every role
/// under its kind and group, in this order.
const ZZ: &[SurfaceRole] = &[
    WINDOW,
    SIDEBAR,
    TITLEBAR,
    PANE_HEADER,
    PHONE_BAR,
    AGENT_BUBBLE,
    CODE,
    FIGURE,
    AGENT_ATTACHMENT,
    SETTINGS_CARD,
    SETTINGS_PREVIEW,
    PALETTE,
    FIND_BAR,
    PICKER,
    INLINE_PANEL,
    FLOATING_PANE,
    PANE_CHIP,
    DISPLAY_PANES,
    DRAG_CHIP,
    DROP_ZONE,
    HOST_ACTION,
    AGENT_OPTION,
    PHONE_PILL,
    PHONE_KEY,
    CHROME_BUTTON,
    WINDOW_CONTROLS,
    ROW_ACTION,
    PANE_HEADER_BUTTON,
    AGENT_SEND,
    AGENT_JUMP,
    AGENT_ACTION,
    BROWSER_BUTTON,
    PHONE_BUTTON,
    BROWSER_SUGGESTION,
    PHONE_ROW,
    ARROW_PAD,
    SIDEBAR_ROW,
    SIDEBAR_ENTRY,
    SIDEBAR_CURSOR,
    WINDOW_TAB,
    BROWSER_TAB,
    AGENT_TRAY,
    SETTINGS_TILE,
    PHONE_CARD,
    PANE_STACK,
    AGENT_COMPOSER,
    AGENT_PICKER,
    ADDRESS_BAR,
    STATUS_CHIP,
    PHONE_PILL_ICON,
    BROWSER_EMPTY_ICON,
    PICKER_NOTICE,
    BAR_RULE,
    SIDEBAR_RULE,
    SPLIT_RULE,
    LIST_RULE,
    TREE_RULE,
];

/// Every role a theme file may name: the kit's kinds and groups, then zz's.
pub fn all() -> impl Iterator<Item = SurfaceRole> {
    SurfaceRole::KIT.into_iter().chain(ZZ.iter().copied())
}

/// The role a theme file calls `id`.
pub fn find(id: &str) -> Option<SurfaceRole> {
    all().find(|role| role.id == id)
}

/// The roles directly in `group`, in the order the theme creator lists them.
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
        for role in ZZ {
            assert!(!role.is_kind(), "{} is a kind", role.id);
        }
    }
}
