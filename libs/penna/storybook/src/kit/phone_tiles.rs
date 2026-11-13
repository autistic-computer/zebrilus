use ala::{
    AnyElement, App, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use penna::{
    ActiveTheme as _, Emphasis, IconName, Tone,
    blocks::compact::{MiniPane, PhoneTile},
    blocks::touch::{StatusMark, TouchGroup, TouchRow, TouchTrailing},
    v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "phone-tiles",
    name: "Phone tiles",
    group: "Kit",
    summary: "The phone tile with its two contents, a machine and a window, the add tile, and the status marks a tile, a row or the pill leads with.",
    sections: &[
        Section {
            id: "marks",
            name: "Status marks",
            summary: "StatusMark: a tone dot 8 across in success, warning, danger, the ink and the faint off tone, and the spinner at 16.",
            build: |_, cx| stateless(marks, cx),
        },
        Section {
            id: "mark-rows",
            name: "Rows with a mark",
            summary: "TouchTrailing::State: a mark and its word at a touch row's end, the word in the mark's ink.",
            build: |_, cx| stateless(mark_rows, cx),
        },
        Section {
            id: "machines",
            name: "Machines",
            summary: "PhoneTile with machine content: a mark and the name, the state in words, and foot lines at the bottom. The add tile goes last.",
            build: |_, cx| stateless(machines, cx),
        },
        Section {
            id: "windows",
            name: "Window states",
            summary: "PhoneTile with window content: the mark and the title, a second line only when the window asks for you or broke, then the window's split as mini panes.",
            build: |_, cx| stateless(windows, cx),
        },
        Section {
            id: "window-sizes",
            name: "Window sizes",
            summary: "A window takes 2 by 2 cells for one or two panes, 4 by 2 for three and 4 by 3 for four or more. Mini panes sit 4 apart, concentric with the tile.",
            build: |_, cx| stateless(sizes, cx),
        },
        Section {
            id: "previews",
            name: "Previews",
            summary: "MiniPane: a terminal's text, a browser's host over a page stand-in, and a chat's last exchange.",
            build: |_, cx| stateless(previews, cx),
        },
    ],
};

const SQUARE: f32 = 181.0;
const WIDE: f32 = 370.0;
const TALL: f32 = 275.5;
const ROW_WIDTH: f32 = 343.0;

const ONE: [[f32; 4]; 1] = [[0.0, 0.0, 1.0, 1.0]];
const SIDE_BY_SIDE: [[f32; 4]; 2] = [[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 1.0]];
const STACKED: [[f32; 4]; 2] = [[0.0, 0.0, 1.0, 0.5], [0.0, 0.5, 1.0, 0.5]];
const LEFT_AND_TWO: [[f32; 4]; 3] = [
    [0.0, 0.0, 0.5, 1.0],
    [0.5, 0.0, 0.5, 0.5],
    [0.5, 0.5, 0.5, 0.5],
];
const QUAD: [[f32; 4]; 4] = [
    [0.0, 0.0, 0.5, 0.5],
    [0.5, 0.0, 0.5, 0.5],
    [0.0, 0.5, 0.5, 0.5],
    [0.5, 0.5, 0.5, 0.5],
];

#[derive(Clone, Copy)]
enum Ink {
    Plain,
    Prompt,
    Success,
    Danger,
}

const TEST: &[(Ink, &str)] = &[
    (Ink::Prompt, "$ cargo test -p penna"),
    (Ink::Plain, "   Compiling penna v0.4.0"),
    (Ink::Plain, "    Finished test in 12.4s"),
    (Ink::Plain, "     Running unittests"),
    (Ink::Success, "test result: ok. 214 passed"),
    (Ink::Prompt, "$ git status --short"),
    (Ink::Plain, " M src/blocks/compact/tile.rs"),
];

const BUILD: &[(Ink, &str)] = &[
    (Ink::Prompt, "$ cargo build"),
    (Ink::Plain, "   Compiling nidus v0.9.1"),
    (Ink::Danger, "error[E0308]: mismatched types"),
    (Ink::Plain, "  --> src/warp.rs:88:17"),
    (Ink::Danger, "error: could not compile nidus"),
];

#[derive(Clone, Copy)]
enum Kind {
    Term(&'static [(Ink, &'static str)]),
    Web(&'static str),
    Chat,
}

fn terminal(lines: &'static [(Ink, &'static str)], cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ink = |ink: Ink| -> Option<Hsla> {
        match ink {
            Ink::Plain => None,
            Ink::Prompt => Some(theme.foreground),
            Ink::Success => Some(theme.paint(Tone::Success, Emphasis::Ghost).text),
            Ink::Danger => Some(theme.paint(Tone::Danger, Emphasis::Ghost).text),
        }
    };
    v_flex().children(lines.iter().map(|(kind, text)| {
        let line = div().child(SharedString::from(*text));
        match ink(*kind) {
            Some(color) => line.text_color(color),
            None => line,
        }
    }))
}

fn pane(id: (&'static str, usize), kind: Kind, cx: &App) -> MiniPane {
    match kind {
        Kind::Term(lines) => MiniPane::terminal(id, terminal(lines, cx)),
        Kind::Web(host) => MiniPane::browser(id, host),
        Kind::Chat => MiniPane::chat(id),
    }
}

fn window(
    id: &'static str,
    title: &'static str,
    mark: Option<StatusMark>,
    state: Option<&'static str>,
    split: &[[f32; 4]],
    kinds: &[Kind],
    cx: &App,
) -> PhoneTile {
    let built = PhoneTile::new(id, title).mark(mark).panes(
        split
            .iter()
            .zip(kinds)
            .enumerate()
            .map(|(index, (rect, kind))| (*rect, pane((id, index), *kind, cx))),
    );
    match state {
        Some(state) => built.state(state),
        None => built,
    }
}

fn boxed(width: f32, height: f32, tile: impl IntoElement) -> impl IntoElement {
    div().w(px(width)).h(px(height)).child(tile)
}

fn marks(_: &mut Window, _: &mut App) -> AnyElement {
    row()
        .child(StatusMark::Success)
        .child(StatusMark::Warning)
        .child(StatusMark::Danger)
        .child(StatusMark::Ink)
        .child(StatusMark::Off)
        .child(StatusMark::Spinner)
        .into_any_element()
}

fn mark_rows(_: &mut Window, _: &mut App) -> AnyElement {
    let state = |id: &'static str, title: &'static str, mark, word: &'static str| {
        TouchRow::new(id, title)
            .icon(IconName::Laptop)
            .trailing(TouchTrailing::State(mark, word.into()))
    };
    div()
        .w(px(ROW_WIDTH))
        .child(
            TouchGroup::new()
                .row(state(
                    "mark-row-connected",
                    "macbook",
                    StatusMark::Success,
                    "Connected",
                ))
                .row(state(
                    "mark-row-waiting",
                    "xps",
                    StatusMark::Warning,
                    "1 waiting",
                ))
                .row(state(
                    "mark-row-failed",
                    "alienware",
                    StatusMark::Danger,
                    "Failed",
                ))
                .row(state(
                    "mark-row-connecting",
                    "thinkpad",
                    StatusMark::Spinner,
                    "Connecting",
                ))
                .row(state(
                    "mark-row-off",
                    "homelab",
                    StatusMark::Off,
                    "Not connected",
                )),
        )
        .into_any_element()
}

fn machines(_: &mut Window, _: &mut App) -> AnyElement {
    let square = |tile: PhoneTile| boxed(SQUARE, SQUARE, tile);
    states()
        .columns(2)
        .state(
            "connected",
            square(
                PhoneTile::new("machine-xps", "xps")
                    .mark(StatusMark::Success)
                    .state("Connected")
                    .foot("2 sessions"),
            ),
        )
        .state(
            "waiting",
            square(
                PhoneTile::new("machine-macbook", "macbook")
                    .mark(StatusMark::Warning)
                    .state("1 waiting for you")
                    .foot("3 sessions")
                    .foot("2 working"),
            ),
        )
        .state(
            "connecting",
            square(
                PhoneTile::new("machine-alienware", "alienware")
                    .mark(StatusMark::Spinner)
                    .state("Connecting")
                    .foot("fabrico@alienware"),
            ),
        )
        .state(
            "offline",
            square(
                PhoneTile::new("machine-thinkpad", "thinkpad")
                    .mark(StatusMark::Off)
                    .state("Not connected")
                    .foot("Last seen 2 days ago"),
            ),
        )
        .state("add", square(PhoneTile::add("machine-add", "Add machine")))
        .into_any_element()
}

fn windows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let square = |tile: PhoneTile| boxed(SQUARE, SQUARE, tile);
    states()
        .columns(3)
        .state(
            "needs approval",
            square(window(
                "window-approval",
                "Make the pill menu offer New chat",
                Some(StatusMark::Warning),
                Some("Needs approval"),
                &SIDE_BY_SIDE,
                &[Kind::Chat, Kind::Term(TEST)],
                cx,
            )),
        )
        .state(
            "has a question",
            square(window(
                "window-question",
                "Pick a tailnet library",
                Some(StatusMark::Warning),
                Some("Has a question"),
                &ONE,
                &[Kind::Chat],
                cx,
            )),
        )
        .state(
            "working",
            square(window(
                "window-working",
                "Fix pane stack hit testing on iPad",
                Some(StatusMark::Spinner),
                None,
                &ONE,
                &[Kind::Chat],
                cx,
            )),
        )
        .state(
            "done, not seen",
            square(window(
                "window-done",
                "Review the tailnet node plan",
                Some(StatusMark::Ink),
                None,
                &STACKED,
                &[Kind::Chat, Kind::Term(TEST)],
                cx,
            )),
        )
        .state(
            "failed",
            square(window(
                "window-failed",
                "cargo build",
                Some(StatusMark::Danger),
                Some("Exit 101"),
                &ONE,
                &[Kind::Term(BUILD)],
                cx,
            )),
        )
        .state(
            "idle",
            square(window(
                "window-idle",
                "zsh",
                None,
                None,
                &ONE,
                &[Kind::Term(TEST)],
                cx,
            )),
        )
        .into_any_element()
}

fn sizes(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .state(
            "one pane, 2 by 2",
            boxed(
                SQUARE,
                SQUARE,
                window(
                    "size-one",
                    "Sync the dotfiles",
                    None,
                    None,
                    &ONE,
                    &[Kind::Chat],
                    cx,
                ),
            ),
        )
        .state(
            "three panes, 4 by 2",
            boxed(
                WIDE,
                SQUARE,
                window(
                    "size-three",
                    "Make the pill menu offer New chat",
                    Some(StatusMark::Warning),
                    Some("Needs approval"),
                    &LEFT_AND_TWO,
                    &[Kind::Chat, Kind::Term(TEST), Kind::Web("127.0.0.1:8090")],
                    cx,
                ),
            ),
        )
        .state(
            "four panes, 4 by 3",
            boxed(
                WIDE,
                TALL,
                window(
                    "size-four",
                    "Release 0.42",
                    Some(StatusMark::Ink),
                    None,
                    &QUAD,
                    &[
                        Kind::Chat,
                        Kind::Term(TEST),
                        Kind::Web("github.com"),
                        Kind::Term(BUILD),
                    ],
                    cx,
                ),
            ),
        )
        .into_any_element()
}

fn previews(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let square = |tile: PhoneTile| boxed(SQUARE, SQUARE, tile);
    states()
        .columns(3)
        .state(
            "terminal",
            square(window(
                "preview-terminal",
                "cargo watch",
                None,
                None,
                &ONE,
                &[Kind::Term(TEST)],
                cx,
            )),
        )
        .state(
            "browser",
            square(window(
                "preview-browser",
                "Phone machines",
                None,
                None,
                &ONE,
                &[Kind::Web("127.0.0.1:8090")],
                cx,
            )),
        )
        .state(
            "chat",
            square(window(
                "preview-chat",
                "Write the release notes",
                None,
                None,
                &ONE,
                &[Kind::Chat],
                cx,
            )),
        )
        .into_any_element()
}
