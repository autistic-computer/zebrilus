use std::sync::Arc;

use ala::{
    AnyElement, App, IntoElement, Keystroke, ParentElement as _, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Emphasis, IconName, Sizable as _, StyledExt as _, Surface,
    blocks::docs::{DocsHeader, DocsLayout, DocsNav, Example, ExampleCode, NavGroup},
    components::button::{Button, ButtonVariants as _},
    components::kbd::Kbd,
    components::text::{TextView, TextViewStyle},
    foundation::roles,
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "docs",
    name: "Docs",
    group: "Kit",
    summary: "The pieces of a documentation page: the header, the grouped navigation, a live example with its code, and the layout that holds them.",
    sections: &[
        Section {
            id: "header",
            name: "Header",
            summary: "DocsHeader: the brand, page links with the current one selected, and trailing actions such as search.",
            build: |_, cx| stateless(header_section, cx),
        },
        Section {
            id: "navigation",
            name: "Navigation",
            summary: "DocsNav: entries in titled groups, drawn with sidebar_heading and sidebar_row, the open page selected.",
            build: |_, cx| stateless(navigation_section, cx),
        },
        Section {
            id: "example",
            name: "Example",
            summary: "Example: a live preview over its code, with a tab per language and a copy button on the code.",
            build: |_, cx| stateless(example_section, cx),
        },
        Section {
            id: "layout",
            name: "Layout",
            summary: "DocsLayout: the header across the top, the navigation down the side, and the page in a centred column that scrolls on its own.",
            build: |_, cx| stateless(layout_section, cx),
        },
    ],
};

const RUST: &str = "Button::new(\"save\")\n    .primary()\n    .label(\"Save\")\n    .on_click(|_, _, cx| cx.notify())";
const HTML: &str = "<button class=\"penna-button\" data-penna=\"button\" data-tone=\"primary\">\n  <span class=\"penna-label\">Save</span>\n</button>";
const PAGE: &str = "# Button\n\nA pressable control. The native build draws it with ala and the DOM build is plain HTML and CSS, from the same tokens.";

fn header() -> DocsHeader {
    DocsHeader::new("docs-header")
        .brand(IconName::AppWindow, "penna")
        .links(["Docs", "Components", "Blocks", "Storybook"])
        .current(Some(1))
        .end(
            Button::new("docs-search")
                .small()
                .emphasis(Emphasis::Outline)
                .w(px(200.0))
                .icon(IconName::Search)
                .label("Search docs")
                .child(div().flex_1())
                .child(Kbd::new(Keystroke::parse("/").unwrap_or_default())),
        )
        .end(
            Button::new("docs-github")
                .icon(IconName::GitBranch)
                .ghost()
                .small()
                .tooltip("Source"),
        )
        .end(
            Button::new("docs-theme")
                .icon(IconName::Palette)
                .ghost()
                .small()
                .tooltip("Theme"),
        )
}

fn nav() -> DocsNav {
    DocsNav::new(
        "docs-nav",
        [
            NavGroup::new(
                "Getting started",
                ["Introduction", "Installation", "Theming", "Native and DOM"],
            ),
            NavGroup::new("Components", ["Badge", "Button", "Input", "Kbd"]),
            NavGroup::new("Blocks", ["Settings", "Sidebar", "Title bar"]),
        ],
    )
    .selected(Some(5))
    .on_pick(|_, _, _| {})
}

fn example(id: &'static str) -> Example {
    Example::new(id)
        .code(ExampleCode::new("Rust", "rust", RUST))
        .code(ExampleCode::new("HTML", "html", HTML))
        .child(Button::new("example-primary").label("Primary").primary())
        .child(Button::new("example-secondary").label("Secondary"))
        .child(Button::new("example-ghost").label("Ghost").ghost())
        .child(Button::new("example-danger").label("Danger").danger())
        .child(Button::new("example-link").label("Link").link())
}

fn header_section(_: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .w(px(880.0))
        .framed(cx)
        .child(header())
        .into_any_element()
}

fn navigation_section(_: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .w(px(248.0))
        .surface(Surface::new(roles::SIDEBAR), cx)
        .framed(cx)
        .child(nav())
        .into_any_element()
}

fn example_section(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state(
            "two languages, Rust shown",
            div().w(px(640.0)).child(example("docs-example")),
        )
        .into_any_element()
}

fn layout_section(_: &mut Window, cx: &mut App) -> AnyElement {
    let style = TextViewStyle {
        highlight_theme: Arc::clone(&cx.theme().highlight_theme),
        is_dark: cx.theme().is_dark(),
        ..TextViewStyle::default()
    };
    div()
        .w(px(1024.0))
        .h(px(600.0))
        .framed(cx)
        .child(
            DocsLayout::new("docs-layout")
                .header(header())
                .nav(nav())
                .child(TextView::markdown("docs-page", PAGE).style(style))
                .child(example("docs-layout-example")),
        )
        .into_any_element()
}
