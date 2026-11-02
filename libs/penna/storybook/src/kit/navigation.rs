use std::rc::Rc;

use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ListSizingBehavior, ParentElement as _,
    Styled as _, Window, div, px, uniform_list,
};
use penna::{
    ActiveTheme as _, Colorize as _, Icon, IconName, RowLook, StyledExt as _, h_flex,
    list::{
        ListItem,
        tree::{IndentGuideColors, IndentGuides},
    },
    roles,
    settings::{NavSection, settings_navigation},
    sidebar::SIDEBAR_DEFAULT_WIDTH,
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "navigation",
    name: "Navigation",
    group: "Kit",
    summary: "Sidebar navigation: the settings section list and the guides that join a tree's rows.",
    sections: &[
        Section {
            id: "settings-navigation",
            name: "Settings navigation",
            summary: "settings_navigation draws the section list from titles, icons and group numbers: a gap where the group changes, the picked section selected, and a Back row first when on_back is set.",
            build: |_, cx| stateless(settings_lists, cx),
        },
        Section {
            id: "tree-guides",
            name: "Tree guides",
            summary: "IndentGuides drawn as a uniform_list decoration in the rule.tree role. Only the guide of the active row's nearest ancestor lights up.",
            build: |_, cx| stateless(tree_guides, cx),
        },
    ],
};

const SECTIONS: [(&str, IconName, usize); 8] = [
    ("General", IconName::Settings, 0),
    ("Appearance", IconName::Palette, 0),
    ("Notifications", IconName::Bell, 0),
    ("Keyboard", IconName::Keyboard, 1),
    ("Files", IconName::Folder, 1),
    ("Account", IconName::User, 2),
    ("Advanced", IconName::Cpu, 2),
    ("About", IconName::Info, 2),
];

fn sections() -> impl Iterator<Item = NavSection> {
    SECTIONS.iter().map(|(title, icon, group)| NavSection {
        title: (*title).into(),
        icon: icon.clone(),
        group: *group,
    })
}

fn sidebar(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .w_full()
        .max_w(px(SIDEBAR_DEFAULT_WIDTH))
        .py(px(8.0))
        .surface(roles::SIDEBAR, cx)
        .framed(cx)
        .child(content)
        .into_any_element()
}

fn settings_lists(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "with Back, Appearance picked",
            sidebar(
                "settings-back",
                settings_navigation("settings-nav-back", sections(), Some(1))
                    .on_pick(|_, _, _| {})
                    .on_back(|_, _, _| {}),
                cx,
            ),
        )
        .state(
            "without Back, nothing picked",
            sidebar(
                "settings-plain",
                settings_navigation("settings-nav-plain", sections(), None).on_pick(|_, _, _| {}),
                cx,
            ),
        )
        .into_any_element()
}

const TREE_ROWS: u8 = 11;
const TREE: [(u8, IconName, &str); TREE_ROWS as usize] = [
    (0, IconName::HardDrive, "workstation"),
    (1, IconName::Layers, "main"),
    (2, IconName::AppWindow, "editor"),
    (3, IconName::File, "lib.rs"),
    (3, IconName::File, "theme.rs"),
    (3, IconName::Globe, "docs"),
    (2, IconName::AppWindow, "notes"),
    (3, IconName::File, "todo.md"),
    (1, IconName::Layers, "scratch"),
    (2, IconName::AppWindow, "shell"),
    (3, IconName::File, "build.log"),
];

const ROW_HEIGHT: f32 = 28.0;
const INDENT: f32 = 16.0;
const ROW_INSET: f32 = 12.0;
const ICON_SLOT: f32 = 16.0;

fn tree(id: &'static str, active_row: Option<usize>, cx: &App) -> AnyElement {
    let depths: Rc<[usize]> = TREE
        .iter()
        .map(|(depth, _, _)| usize::from(*depth))
        .collect();
    let rows = uniform_list(id, TREE.len(), move |range, _, cx| {
        let muted = cx.theme().foreground.muted();
        range
            .map(|index| {
                let (depth, icon, label) = &TREE[index];
                ListItem::new(("tree-row", index))
                    .look(RowLook::Sidebar)
                    .selected(active_row == Some(index))
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .pl(px(ROW_INSET + INDENT * f32::from(*depth)))
                    .child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .child(Icon::new(icon.clone()).medium().text_color(muted))
                            .child(*label),
                    )
            })
            .collect::<Vec<_>>()
    })
    .size_full()
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .with_decoration(IndentGuides::new(
        depths,
        active_row,
        px(INDENT),
        px(ROW_INSET + ICON_SLOT / 2.0),
        px(4.0),
        IndentGuideColors::tree(cx),
    ));
    sidebar(
        id,
        div().h(px(ROW_HEIGHT * f32::from(TREE_ROWS))).child(rows),
        cx,
    )
}

fn tree_guides(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(3)
        .state("active row: theme.rs", tree("tree-theme", Some(4), cx))
        .state("active row: build.log", tree("tree-log", Some(10), cx))
        .state("no active row", tree("tree-none", None, cx))
        .into_any_element()
}
