use ala::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Icon, IconName, IndexPath, Space, StyledExt as _, TextStyle,
    blocks::settings::{
        SettingEntry, SettingsSelectItem, SettingsStack,
        about::{
            ABOUT_LOGO_SIZE, About, AboutLink, about_build_stack, about_copy_button, about_hero,
            about_project_stack,
        },
        settings_card_ground, settings_page_content, settings_page_heading,
    },
    components::button::Button,
    components::card::{Card, CardLook},
    components::select::{Select, SelectState},
    components::switch::Switch,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "settings",
    name: "Settings",
    group: "Kit",
    summary: "What a settings page is built from: the page heading, SettingsStack and SettingEntry in each of their states, and the About page's hero and stacks.",
    sections: &[
        Section {
            id: "entries",
            name: "Stacks and entries",
            summary: "SettingsStack glues SettingEntry rows into one card and rounds only its ends, with a list rule between rows inset-card 12 from the sides. A row is its title and its control at the trailing end; a short description sits under the title only where it says something the title and control do not.",
            build: |window, cx| cx.new(|cx| Entries::new(window, cx)).into(),
        },
        Section {
            id: "about",
            name: "About",
            summary: "about_hero over about_build_stack and about_project_stack: the logo, name, tagline and version, the build details to quote in a bug report, and the project links.",
            build: |_, cx| stateless(about, cx),
        },
    ],
};

struct Entries {
    start: Entity<SelectState<Vec<SettingsSelectItem>>>,
    on: [bool; 4],
}

impl Entries {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let start = cx.new(|cx| {
            SelectState::new(
                vec![
                    SettingsSelectItem::new("Last session", "last"),
                    SettingsSelectItem::new("New window", "new"),
                ],
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        Self {
            start,
            on: [true, true, true, false],
        }
    }
}

const CONFIG: [&str; 3] = [
    "font-family = Inter",
    "cursor-style = bar",
    "window-padding-x = 8",
];

impl Render for Entries {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = settings_card_ground(cx);
        let switch = |id: &'static str, at: usize| {
            Switch::new(id).checked(self.on[at]).on_click(cx.listener(
                move |this, on: &bool, _, cx| {
                    this.on[at] = *on;
                    cx.notify();
                },
            ))
        };
        states()
            .columns(2)
            .state("page heading", settings_page_heading("General", cx))
            .state(
                "titled stack: switch, select and button controls",
                SettingsStack::titled("Startup")
                    .id("stack-startup")
                    .child(
                        SettingEntry::new("Open at login")
                            .id("entry-login")
                            .control(switch("switch-login", 0)),
                    )
                    .child(
                        SettingEntry::new("Start with")
                            .id("entry-start")
                            .control(Select::new(&self.start).title("Start with").on(ground)),
                    )
                    .child(
                        SettingEntry::new("Theme file")
                            .id("entry-theme-file")
                            .control(Button::new("choose-theme").label("Choose…").on(ground)),
                    ),
            )
            .state(
                "a single entry rounds all four corners",
                SettingsStack::new().id("stack-single").child(
                    SettingEntry::new("Show the sidebar")
                        .id("entry-sidebar")
                        .control(switch("switch-sidebar", 1)),
                ),
            )
            .state(
                "a description under the title",
                SettingsStack::titled("Updates")
                    .id("stack-updates")
                    .child(
                        SettingEntry::new("Check for updates")
                            .id("entry-updates")
                            .description("Looks for a new version once a day")
                            .control(switch("switch-updates", 2)),
                    )
                    .child(
                        SettingEntry::new("Early builds")
                            .id("entry-early")
                            .description("Get each version a week before it ships")
                            .control(switch("switch-early", 3)),
                    ),
            )
            .state(
                "disabled: no effect with the current configuration",
                SettingsStack::titled("Sync")
                    .id("stack-sync")
                    .child(
                        SettingEntry::new("Sync settings")
                            .id("entry-sync-settings")
                            .description("Needs an account")
                            .disabled(true)
                            .control(Switch::new("switch-sync-settings")),
                    )
                    .child(
                        SettingEntry::new("Sync themes")
                            .id("entry-sync-themes")
                            .description("Needs an account")
                            .disabled(true)
                            .control(Switch::new("switch-sync-themes")),
                    ),
            )
            .state(
                "a stack note saying why its rows are off",
                SettingsStack::titled("Shortcuts")
                    .id("stack-shortcuts")
                    .note("Connect to a local session to edit shortcuts.")
                    .child(
                        SettingEntry::new("Split below")
                            .id("entry-split")
                            .disabled(true)
                            .control(Button::new("record-split").label("Record").on(ground)),
                    ),
            )
            .state(
                "a state glyph before the title",
                SettingsStack::titled("Devices").id("stack-devices").child(
                    SettingEntry::new("studio")
                        .id("entry-studio")
                        .description("fabrico@studio.local")
                        .title_icon(Icon::new(IconName::CircleCheck).text_color(cx.theme().success))
                        .control(
                            Button::new("disconnect-studio")
                                .label("Disconnect")
                                .on(ground),
                        ),
                ),
            )
            .state(
                "full-width content under the copy",
                SettingsStack::new().id("stack-config").child(
                    SettingEntry::new("Colors, cursor, and spacing")
                        .id("entry-config")
                        .child(
                            Card::new("config-preview", CardLook::Code)
                                .on(ground)
                                .flex()
                                .flex_col()
                                .type_style(TextStyle::Mono, cx)
                                .children(CONFIG.map(|line| div().child(line))),
                        ),
                ),
            )
    }
}

const NESTLING: About = About {
    name: "Nestling",
    tagline: "Notes that never leave the nest.",
    version: "0.3.1",
    license: "MIT or Apache-2.0",
    links: &[
        AboutLink {
            id: "about-source",
            title: "Source code",
            label: "Repository",
            url: "https://example.com/nestling",
        },
        AboutLink {
            id: "about-releases",
            title: "Releases",
            label: "Releases",
            url: "https://example.com/nestling/releases",
        },
        AboutLink {
            id: "about-issues",
            title: "Report an issue",
            label: "New issue",
            url: "https://example.com/nestling/issues",
        },
    ],
};

fn about(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let logo = Icon::new(IconName::Pencil)
        .size(px(ABOUT_LOGO_SIZE))
        .text_color(cx.theme().accent);
    states()
        .state(
            "page",
            settings_page_content()
                .gap(Space::GAP_GROUP)
                .child(about_hero(&NESTLING, logo, cx))
                .child(
                    about_build_stack(
                        &NESTLING,
                        "browser · wasm32",
                        about_copy_button("copy-build").on(settings_card_ground(cx)),
                        cx,
                    )
                    .id("stack-build"),
                )
                .child(about_project_stack(&NESTLING, cx).id("stack-project")),
        )
        .into_any_element()
}
