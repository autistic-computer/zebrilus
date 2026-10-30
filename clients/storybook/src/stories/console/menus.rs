use std::rc::Rc;

use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window,
    div, px, size,
};
use penna::ActiveTheme as _;
use zz_protocol::{MenuItem, MenuState, PopupBorderLines};
use zz_ui::{
    command::floating::{confirm_prompt, display_menu},
    pane::FloatingSurface,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "floating-menus",
    name: "Floating menus",
    group: "Commands",
    summary: "display-menu drawn as the kit's popup menu from the daemon's MenuState, and the confirm-before question on a floating surface.",
    sections: &[
        Section {
            id: "display-menu",
            name: "display-menu",
            summary: "The pane menu: a centred title, key hints, separators, a disabled item and the highlighted row. A disabled row under the pointer stays unhighlighted.",
            build: |_, cx| stateless(menus, cx),
        },
        Section {
            id: "menu-styles",
            name: "Menu styles",
            summary: "-b none drops the outline and the title. Literal colours from -s, -S and -H tint the surface, the outline and the highlight; theme slots leave the look's own.",
            build: |_, cx| stateless(menu_styles, cx),
        },
        Section {
            id: "confirm-before",
            name: "confirm-before",
            summary: "The one-line question confirm-before asks, sized to its prompt.",
            build: |_, cx| stateless(confirm, cx),
        },
    ],
};

const PANE_MENU: &[Option<(&str, Option<&str>, bool)>] = &[
    Some(("Search For #[underscore]config", Some("C-r"), true)),
    Some(("Copy #[underscore]config", Some("c"), true)),
    None,
    Some(("Horizontal Split", Some("h"), true)),
    Some(("Vertical Split", Some("v"), true)),
    None,
    Some(("Swap Up", Some("u"), true)),
    Some(("Swap Down", Some("d"), true)),
    Some(("Swap Marked", None, false)),
    None,
    Some(("Kill", Some("X"), true)),
    Some(("Respawn", Some("R"), true)),
    Some(("Mark", Some("m"), true)),
    Some(("Zoom", Some("z"), true)),
];

const DISABLED_ROW: usize = 8;

struct Styles {
    style: &'static str,
    border_style: &'static str,
    selected_style: &'static str,
}

const DEFAULT_STYLES: Styles = Styles {
    style: "bg=themedarkgrey,fg=themewhite",
    border_style: "bg=themedarkgrey,fg=themelightgrey",
    selected_style: "bg=themeyellow,fg=themeblack",
};

fn pane_menu(
    selected: Option<usize>,
    border_lines: PopupBorderLines,
    styles: &Styles,
) -> MenuState {
    let items: Vec<Option<MenuItem>> = PANE_MENU
        .iter()
        .map(|item| {
            item.map(|(name, key, enabled)| MenuItem {
                name: name.to_owned(),
                key: key.map(str::to_owned),
                annotation: key.map(str::to_owned),
                enabled,
            })
        })
        .collect();
    MenuState {
        left: 0,
        top: 0,
        width: 26,
        height: items.len() as u16 + 2,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 16,
        cell_height_px: 38,
        title: "#[align=centre]0 (%3)".to_owned(),
        style: styles.style.to_owned(),
        selected_style: styles.selected_style.to_owned(),
        border_style: styles.border_style.to_owned(),
        border_lines,
        items,
        selected: selected.map(|index| index as u32),
        stay_open: false,
        mouse_keys: true,
    }
}

fn menu(id: &'static str, state: &MenuState, cx: &App) -> AnyElement {
    let selected = state.selected.map(|index| index as usize);
    div()
        .id(id)
        .flex()
        .child(display_menu(
            state,
            selected,
            size(px(800.0), px(600.0)),
            &Rc::default(),
            cx,
        ))
        .into_any_element()
}

fn menus(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let single = PopupBorderLines::Single;
    states()
        .columns(3)
        .state(
            "nothing selected",
            menu("menu-rest", &pane_menu(None, single, &DEFAULT_STYLES), cx),
        )
        .state(
            "row selected",
            menu(
                "menu-selected",
                &pane_menu(Some(3), single, &DEFAULT_STYLES),
                cx,
            ),
        )
        .state(
            "disabled row under the pointer",
            menu(
                "menu-disabled",
                &pane_menu(Some(DISABLED_ROW), single, &DEFAULT_STYLES),
                cx,
            ),
        )
        .into_any_element()
}

fn menu_styles(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let tmux = Styles {
        style: "bg=themedarkgrey,fg=themewhite,bg=colour236,fg=colour252",
        border_style: "bg=themedarkgrey,fg=themelightgrey,fg=colour39",
        selected_style: "bg=themeyellow,fg=themeblack,bg=colour39,fg=colour16",
    };
    let themed = Styles {
        style: "bg=themedarkgrey,fg=themewhite,bg=themeblue",
        border_style: "bg=themedarkgrey,fg=themelightgrey,fg=themegreen",
        selected_style: "bg=themeyellow,fg=themeblack,bg=themered",
    };
    states()
        .columns(3)
        .state(
            "-b none",
            menu(
                "menu-borderless",
                &pane_menu(Some(3), PopupBorderLines::None, &DEFAULT_STYLES),
                cx,
            ),
        )
        .state(
            "colour256 styles",
            menu(
                "menu-tmux",
                &pane_menu(Some(3), PopupBorderLines::Single, &tmux),
                cx,
            ),
        )
        .state(
            "theme slots follow the look",
            menu(
                "menu-theme",
                &pane_menu(Some(3), PopupBorderLines::Single, &themed),
                cx,
            ),
        )
        .into_any_element()
}

fn confirm_surface(id: &'static str, prompt: &str, cx: &App) -> AnyElement {
    let width = (prompt.chars().count() as f32 * 8.0 + 32.0).max(180.0);
    div()
        .w(px(width))
        .h(px(48.0))
        .child(FloatingSurface::new(
            id,
            confirm_prompt(
                (id, 0_usize),
                prompt.to_owned(),
                cx.theme().mono_font_family.clone(),
            ),
            cx,
        ))
        .into_any_element()
}

fn confirm(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .state(
            "short prompt, minimum width",
            confirm_surface("confirm-short", "kill-pane? (y/n)", cx),
        )
        .state(
            "long prompt",
            row().child(confirm_surface(
                "confirm-long",
                "kill-session -t notes? This also closes 3 windows (y/n)",
                cx,
            )),
        )
        .into_any_element()
}
