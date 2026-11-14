use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, IconName, Tone,
    blocks::chat::phone_jump_button,
    blocks::compact::{
        PillLabel, compact_floating_bar, compact_floating_pill, compact_pill_collapse,
        compact_pill_label, mini_pill_width, over_pill, pill_bounds,
    },
    blocks::touch::{KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, PhoneFind, page_dots},
    components::input::InputState,
};
use zz_terminal::{CursorStyle, OverlayKind, OverlaySpan, SearchStatus};
use zz_ui::compact::{LinkAction, ToolKeys, find_count, link_sheet};

use super::phone::{PHONE_WIDTH, groups, key_row, phone, placement, tool_keys};
use super::terminal::{Sample, SampleView, Screen, cursor};
use crate::story::states;

const HEIGHT: f32 = 720.0;
const TOP: f32 = 16.0;
const COLUMNS: u16 = 41;
const ROWS: u16 = 42;
const TITLE: &str = "zsh";
const WHERE: &str = "zebrilus · pill-find";
const URL: &str = "https://github.com/autistic-computer/zebrilus/pull/new/pill-find";
const QUERY: &str = "pill-find";

#[derive(Clone, Copy)]
enum Line {
    Prompt(&'static str),
    Plain(&'static str),
    Success(&'static str),
    Danger(&'static str),
}

const SESSION: &[Line] = &[
    Line::Prompt("cargo test -p penna pill"),
    Line::Plain("   Compiling penna v0.42.0"),
    Line::Plain("    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.12s"),
    Line::Plain("running 14 tests"),
    Line::Success("test result: ok. 14 passed; 0 failed"),
    Line::Prompt("git push -u origin pill-find"),
    Line::Plain("remote:"),
    Line::Plain("remote: Create a pull request for 'pill-find' on GitHub by visiting:"),
    Line::Plain("remote:      https://github.com/autistic-computer/zebrilus/pull/new/pill-find"),
    Line::Plain("remote:"),
    Line::Plain("To github.com:autistic-computer/zebrilus.git"),
    Line::Plain(" * [new branch]      pill-find -> pill-find"),
    Line::Plain("branch 'pill-find' set up to track 'origin/pill-find'."),
    Line::Prompt(""),
];

const OLDER: &[Line] = &[
    Line::Prompt("git log --oneline -6"),
    Line::Plain("9b2f0c1 Draw the pane menu at touch sizes"),
    Line::Plain("e41d7a3 Pin the key row's ends"),
    Line::Plain("7c09e52 Swipe the pill to the next pane"),
    Line::Plain("3a6f1d8 Fold the title bar on scroll"),
    Line::Plain("d52c4b0 Size phone tiles by pane count"),
    Line::Plain("1f8e2a9 Add the zoom card"),
    Line::Prompt("cargo clippy -p zz-app -- -D warnings"),
    Line::Plain("    Checking zz-ui v0.42.0"),
    Line::Plain("    Checking zz-app v0.42.0"),
    Line::Plain("    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.77s"),
    Line::Prompt("git switch -c pill-find"),
    Line::Plain("Switched to a new branch 'pill-find'"),
    Line::Prompt("git status --short"),
    Line::Plain(" M libs/penna/src/blocks/compact/pill_menu.rs"),
    Line::Plain(" M clients/app/src/app/compact.rs"),
    Line::Prompt("git commit -am 'Put Find in the pill menu'"),
    Line::Plain("[pill-find 4c1e9a7] Put Find in the pill menu"),
    Line::Plain(" 2 files changed, 31 insertions(+), 6 deletions(-)"),
    Line::Prompt("cargo test -p penna pill"),
    Line::Plain("   Compiling penna v0.42.0"),
    Line::Plain("    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.12s"),
    Line::Plain("running 14 tests"),
    Line::Success("test result: ok. 14 passed; 0 failed"),
    Line::Prompt("git push -u origin pill-find"),
    Line::Plain("remote:"),
];

const TESTS: &[Line] = &[
    Line::Prompt("cargo test -p zz-daemon"),
    Line::Plain("   Compiling zz-daemon v0.42.0"),
    Line::Plain("     Running unittests src/lib.rs"),
    Line::Plain("running 212 tests"),
    Line::Plain("test agent::turn::wakes_on_time ... ok"),
    Line::Plain("test agent::turn::queues_while_busy ... ok"),
    Line::Plain("test pty::resize_keeps_cursor ... ok"),
    Line::Plain("test server::attach_shares_session ... ok"),
    Line::Plain("test server::detach_keeps_panes ... ok"),
];

const FAILED: &[Line] = &[
    Line::Prompt("cargo build -p zz-daemon"),
    Line::Plain("   Compiling zz-daemon v0.42.0"),
    Line::Danger("error[E0425]: cannot find value `ttl` in this scope"),
    Line::Plain("  --> crates/zz-daemon/src/agent/turn.rs:98:10"),
    Line::Plain("   |"),
    Line::Plain("98 |     wake(ttl)"),
    Line::Plain("   |          ^^^ not found in this scope"),
    Line::Plain(""),
    Line::Danger("error: could not compile `zz-daemon` (lib) due to 1 previous error"),
    Line::Prompt(""),
];

fn wrapped(lines: &[Line]) -> Vec<(Line, String)> {
    let columns = usize::from(COLUMNS);
    let mut rows = Vec::new();
    for line in lines {
        let text = match line {
            Line::Prompt(command) => format!("$ {command}"),
            Line::Plain(text) | Line::Success(text) | Line::Danger(text) => (*text).to_owned(),
        };
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            rows.push((*line, String::new()));
            continue;
        }
        for chunk in chars.chunks(columns) {
            rows.push((*line, chunk.iter().collect()));
        }
    }
    rows
}

fn terminal(lines: &[Line], current: Option<u32>, cx: &App) -> Sample {
    let rows = wrapped(lines);
    let mut screen = Screen::new(COLUMNS, ROWS);
    let prompt = screen.bold(4);
    let success = screen.fg(2);
    let danger = screen.bold(1);
    let mut end = (0, 0);
    let mut overlays = Vec::new();
    let mut hits = 0;
    for (row, (line, text)) in rows.iter().enumerate() {
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        let style = match line {
            Line::Prompt(_) => prompt,
            Line::Plain(_) => 0,
            Line::Success(_) => success,
            Line::Danger(_) => danger,
        };
        let column = screen.write(row, 0, text, style);
        if matches!(line, Line::Prompt(_)) {
            end = (column, row);
        }
        if current.is_some() {
            for (at, _) in text.match_indices(QUERY) {
                hits += 1;
                let start = u16::try_from(text[..at].chars().count()).unwrap_or(0);
                let stop = start + u16::try_from(QUERY.len()).unwrap_or(0);
                let kind = if Some(hits) == current {
                    OverlayKind::SearchCurrent
                } else {
                    OverlayKind::SearchMatch
                };
                overlays.push(OverlaySpan::new(row, start, stop, kind));
            }
        }
    }
    let mut viewport = screen.viewport();
    if let Some(current) = current {
        viewport.search = Some(SearchStatus::new(current, hits));
        viewport.overlays = overlays.into();
    }
    if matches!(lines.last(), Some(Line::Prompt(""))) {
        viewport.cursor = Some(cursor(end.0, end.1, CursorStyle::Block));
    }
    Sample::new(viewport).flush().themed(cx)
}

fn label(word: &str, meta: &str, tone: Tone) -> PillLabel {
    PillLabel {
        icon: IconName::SquareTerminal,
        face: None,
        title: TITLE.into(),
        word: SharedString::from(word.to_owned()),
        meta: SharedString::from(meta.to_owned()),
        tone,
    }
}

fn pill(id: &'static str, label: PillLabel, cx: &App) -> AnyElement {
    let placement = placement();
    let pill = compact_floating_pill(id, label, page_dots(&groups(), cx), &placement, cx);
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .child(compact_floating_bar(pill, &placement, cx))
        .into_any_element()
}

fn rest_pill(id: &'static str, cx: &App) -> AnyElement {
    pill(id, label("", WHERE, Tone::Neutral), cx)
}

fn over_keyboard(element: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .child(element)
        .into_any_element()
}

fn frame(id: &'static str, page: &Entity<SampleView>, over: Vec<AnyElement>, cx: &App) -> ala::Div {
    phone(
        div()
            .id(id)
            .relative()
            .h(px(HEIGHT))
            .overflow_hidden()
            .pt(px(TOP))
            .bg(zz_ui::pane_look::pane_color(
                cx.theme(),
                cx.theme().background,
            ))
            .child(page.clone())
            .children(over),
        cx,
    )
}

pub(super) struct TerminalStates {
    session: Entity<SampleView>,
    tests: Entity<SampleView>,
    found: Entity<SampleView>,
    older: Entity<SampleView>,
    failed: Entity<SampleView>,
    keys: ToolKeys,
    latched: ToolKeys,
    query: Entity<InputState>,
    link: bool,
}

pub(super) fn terminal_states(window: &mut Window, cx: &mut App) -> AnyView {
    let sample = |lines: &[Line], current: Option<u32>, cx: &mut App| {
        let sample = terminal(lines, current, cx);
        cx.new(|_| SampleView::new(sample))
    };
    let latched = tool_keys(cx);
    latched
        .control
        .update(cx, |key, cx| key.show_latched(true, cx));
    latched.scroll.set_offset(ala::point(px(-1000.0), px(0.0)));
    let view = TerminalStates {
        session: sample(SESSION, None, cx),
        tests: sample(TESTS, None, cx),
        found: sample(SESSION, Some(3), cx),
        older: sample(OLDER, None, cx),
        failed: sample(FAILED, None, cx),
        keys: tool_keys(cx),
        latched,
        query: cx.new(|cx| InputState::new(window, cx).default_value(QUERY)),
        link: true,
    };
    cx.new(|_| view).into()
}

impl TerminalStates {
    fn find(&self, id: &'static str, cx: &App) -> PhoneFind {
        let status = Some(SearchStatus::new(3, 6));
        let (count, found) = find_count(QUERY, status);
        PhoneFind::new(id, &self.query, cx.theme().ground())
            .count(count)
            .found(found)
            .on_previous(|_, _| {})
            .on_next(|_, _| {})
    }
}

impl Render for TerminalStates {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placement = placement();
        let width = px(PHONE_WIDTH);
        let short = SharedString::from(TITLE);
        let mini = mini_pill_width(&short, window, cx);
        let bounds = pill_bounds(0.0, width, mini, &placement);
        let jump_at = over_pill(bounds, px(penna::blocks::touch::TOUCH_TARGET));
        let mini_label = compact_pill_label(
            label("", WHERE, Tone::Neutral),
            page_dots(&groups(), cx),
            &placement,
            cx,
        );
        let mini_pill = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(placement.zone())
            .child(
                div()
                    .absolute()
                    .left(jump_at.x)
                    .top(jump_at.y)
                    .child(phone_jump_button(
                        "term-back-jump",
                        "Jump to bottom",
                        cx.theme().ground(),
                    )),
            )
            .child(compact_pill_collapse(
                "term-back-pill",
                mini_label,
                short,
                0.0,
                bounds,
                width,
                &placement,
                cx,
            ))
            .into_any_element();
        let view = cx.entity().downgrade();
        let dismiss = view.clone();
        let link = self.link.then(|| {
            link_sheet(
                "term-link-sheet",
                URL,
                &[LinkAction::Browser, LinkAction::Phone, LinkAction::Copy],
                |_, _, _| {},
                move |_, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        this.link = false;
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        });
        let link_frame = frame("term-link", &self.session, link.into_iter().collect(), cx).when(
            !self.link,
            |frame| {
                frame.child(div().id("term-link-again").absolute().inset_0().on_click(
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.link = true;
                            cx.notify();
                        });
                    },
                ))
            },
        );
        let docked = |find: PhoneFind| {
            over_keyboard(
                div()
                    .px(px(KEY_ROW_MARGIN_X))
                    .pb(px(KEY_ROW_MARGIN_Y))
                    .child(find),
            )
        };
        states()
            .columns(2)
            .state(
                "at rest: the pill names the pane and where it works",
                frame(
                    "term-rest",
                    &self.session,
                    vec![rest_pill("term-rest-pill", cx)],
                    cx,
                ),
            )
            .state(
                "keyboard up: the key row takes the pill's place, the prefix and the ends pinned",
                frame(
                    "term-keys",
                    &self.session,
                    vec![over_keyboard(key_row(&self.keys))],
                    cx,
                ),
            )
            .state(
                "ctrl latched, drawn as a wash, the row scrolled to its end",
                frame(
                    "term-latched",
                    &self.tests,
                    vec![over_keyboard(key_row(&self.latched))],
                    cx,
                ),
            )
            .state(
                "find: the field rides the keyboard, the current match stronger",
                frame(
                    "term-find",
                    &self.found,
                    vec![docked(self.find("term-find-field", cx))],
                    cx,
                ),
            )
            .state(
                "find with the keyboard down: the field keeps the pill's spot",
                frame(
                    "term-find-rest",
                    &self.found,
                    vec![over_keyboard(compact_floating_bar(
                        self.find("term-find-rest-field", cx),
                        &placement,
                        cx,
                    ))],
                    cx,
                ),
            )
            .state("a tapped link: the link sheet", link_frame)
            .state(
                "scrolled back: the mini pill and jump to bottom over it",
                frame("term-back", &self.older, vec![mini_pill], cx),
            )
            .state(
                "a failed command: Exit 1 in danger ink, then the folder",
                frame(
                    "term-failed",
                    &self.failed,
                    vec![pill(
                        "term-failed-pill",
                        label("Exit 1", " · zebrilus · main", Tone::Danger),
                        cx,
                    )],
                    cx,
                ),
            )
    }
}
