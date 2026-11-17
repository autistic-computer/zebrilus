use std::{cell::RefCell, rc::Rc, sync::Arc};

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Pixels, Render, ScrollHandle, Styled as _, Window, canvas, div, px,
};
use penna::{ActiveTheme as _, IconName, components::input::InputState};
use zz_protocol::{AgentProvider, PaneId};
use zz_terminal::{
    CursorStyle, PackedCell, ScrollbarState, TerminalAppearance, TerminalDictionary,
    TerminalViewport,
};
use zz_ui::{
    agent::resume::EmptyChat,
    pane::{
        HeaderBand, HeaderPlace, PaneActionKeys, PaneDragSource, PaneState, headed_pane,
        header_scrim, pane_header, pane_header_frame, pane_surface, tiled_pane_ground,
    },
    terminal::{
        RowRenderCache, TerminalHistoryRow, TerminalHistorySource, TerminalRenderInput,
        terminal_font_for_style,
    },
};

use super::terminal::{Ink, Screen, appearance, cell_metrics, cursor, terminal_text};
use zz_protocol::chat::ItemKind;

use crate::stories::agent::{
    chat::{Frame, Thread},
    fixtures::{Script, completed},
};
use crate::stories::workspace::fixtures::{chrome, radii};
use crate::story::{Section, Story, states};

pub const STORY: Story = Story {
    id: "floating-header",
    name: "Floating header",
    group: "Terminal",
    summary: "Terminal and agent panes put their header over the content. The live screen starts under the header, so the grid keeps its size. Scrolled back, history rows run up under the header behind a scrim of the pane's surface: solid at the top, 88% at the header's edge, clear 20px below it. Chats always draw the scrim and scroll under it.",
    sections: &[
        Section {
            id: "terminal",
            name: "Terminal",
            summary: "A live shell, the same shell scrolled back, and a full-screen program whose first row is coloured. The live screen and the program have no scrim.",
            build: |window, cx| terminals(window, cx),
        },
        Section {
            id: "chat",
            name: "Chat",
            summary: "The empty screen with its sky running to the pane's top edge, and a chat scrolled to its latest message with earlier ones under the header.",
            build: |window, cx| chats(window, cx),
        },
    ],
};

const WIDTH: f32 = 400.0;
const ROWS: u16 = 12;
const CHAT_HEIGHT: f32 = 420.0;
const FOLDER: &str = "~/dev/autistic-computer/zebrilus";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Live,
    Back,
    Program,
}

struct History(Vec<Vec<PackedCell>>, Arc<TerminalDictionary>);

impl TerminalHistorySource for History {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.0.get(index).map(|cells| TerminalHistoryRow {
            cells,
            dictionary: &self.1,
            revision: 10_000 + index as u64,
        })
    }
}

fn shell_lines() -> TerminalViewport {
    let mut screen = Screen::new(52, 29);
    let path = screen.bold(4);
    let branch = screen.fg(5);
    let arrow = screen.bold(2);
    let green = screen.bold(2);
    let warn = screen.bold(3);
    let hash = screen.fg(3);
    let dir = screen.bold(4);
    let mut row = 0;
    let prompt = |screen: &mut Screen, row: u16, command: &str| {
        let mut column = screen.write(row, 0, "~/dev/zz ", path);
        column = screen.write(row, column, "main* ", branch);
        column = screen.write(row, column, "> ", arrow);
        screen.write(row, column, command, 0);
    };
    prompt(&mut screen, row, "cargo build -p zz-ui");
    row += 1;
    for (head, style, rest) in [
        ("   Compiling", green, " penna v0.9.0"),
        ("   Compiling", green, " zz-ui v0.22.0"),
        ("warning", warn, ": unused import: `px`"),
        ("  -->", dir, " crates/zz-ui/src/pane.rs:12:5"),
        ("warning", warn, ": `zz-ui` (lib) generated 1 warning"),
        ("    Finished", green, " `dev` profile in 41.3s"),
    ] {
        let column = screen.write(row, 0, head, style);
        screen.write(row, column, rest, 0);
        row += 1;
    }
    prompt(&mut screen, row, "git log --oneline -12");
    row += 1;
    for (sha, subject) in [
        ("1954c97", "Add the agent mentions design"),
        ("2380a89", "Refresh the storybook lockfile"),
        ("c763456", "Merge branch 'tab-squares'"),
        ("2d6821e", "Size the chat sidebar's icon column"),
        ("650007e", "Size chat row stacks to their faces"),
        ("d65370b", "Board the agent sign-in steps"),
        ("d96bf32", "Add the chat fluff board"),
        ("abc8966", "Polish the phone client"),
        ("8b1e0aa", "Fold header actions into one menu"),
        ("41aa0c2", "Draw the sky behind the empty chat"),
        ("9f07d13", "Keep the PiP title in its border"),
        ("e2c5b71", "Round pane corners with the window"),
    ] {
        let column = screen.write(row, 0, sha, hash);
        screen.write(row, column + 1, subject, 0);
        row += 1;
    }
    prompt(&mut screen, row, "ls");
    row += 1;
    for line in [
        ["Cargo.lock", "bench/", "design/"],
        ["Cargo.toml", "clients/", "knowledge/"],
        ["Justfile", "compat/", "libs/"],
    ] {
        let mut column = 0;
        for (index, name) in line.into_iter().enumerate() {
            screen.write(row, column, name, if index == 0 { 0 } else { dir });
            column += 14;
        }
        row += 1;
    }
    prompt(&mut screen, row, "rg -c pane_header crates");
    row += 1;
    for line in [
        "crates/zz-ui/src/pane.rs:9",
        "crates/zz-ui/src/pip.rs:1",
        "crates/zz/src/workspace/view.rs:6",
    ] {
        screen.write(row, 0, line, 0);
        row += 1;
    }
    prompt(&mut screen, row, "");
    screen.viewport()
}

fn window_of(all: &TerminalViewport, start: u16, live: bool) -> (TerminalViewport, History) {
    let columns = usize::from(all.columns);
    let cells = |from: u16, to: u16| {
        all.cells[usize::from(from) * columns..usize::from(to) * columns].to_vec()
    };
    let mut viewport = all.clone();
    viewport.rows = ROWS;
    viewport.cells = cells(start, start + ROWS).into();
    viewport.scrollbar = ScrollbarState {
        total: u32::from(all.rows),
        offset: u32::from(start),
        len: u32::from(ROWS),
    };
    viewport.cursor = live.then(|| cursor(17, ROWS - 1, CursorStyle::Block));
    let history = (0..start)
        .map(|row| cells(row, row + 1))
        .collect::<Vec<_>>();
    (viewport, History(history, Arc::clone(&all.dictionary)))
}

fn program() -> TerminalViewport {
    let mut screen = Screen::new(52, ROWS);
    let tabs = screen.ink(Ink {
        fg: Some(screen.palette(0)),
        bg: Some(screen.palette(4)),
        ..Ink::default()
    });
    let current = screen.ink(Ink {
        fg: Some(screen.palette(0)),
        bg: Some(screen.palette(15)),
        attrs: zz_terminal::ATTR_BOLD,
        ..Ink::default()
    });
    screen.fill(0, 0, 52, tabs);
    let column = screen.write(0, 0, " view.rs ", current);
    screen.write(0, column, " pane.rs  sky.rs  chat.rs", tabs);
    let number = screen.fg(8);
    let call = screen.fg(4);
    for (index, line) in [
        "    .child(",
        "        AnyView::from(header).cached(",
        "            StyleRefinement::default()",
        "                .h(header_height)",
        "                .w_full(),",
        "        ),",
        "    )",
        "    .child(content)",
        "    .children(report)",
    ]
    .into_iter()
    .enumerate()
    {
        let row = 1 + index as u16;
        let column = screen.write(row, 0, &format!("{:>4} ", 3846 + index), number);
        screen.write(row, column, line, if line.contains('(') { call } else { 0 });
    }
    let status = screen.ink(Ink {
        fg: Some(screen.palette(0)),
        bg: Some(screen.palette(2)),
        attrs: zz_terminal::ATTR_BOLD,
        ..Ink::default()
    });
    let bar = screen.ink(Ink {
        bg: Some(screen.palette(8)),
        ..Ink::default()
    });
    screen.fill(ROWS - 2, 0, 52, bar);
    let column = screen.write(ROWS - 2, 0, " NORMAL ", status);
    screen.write(ROWS - 2, column, " crates/zz/src/workspace/view.rs", bar);
    let mut viewport = screen.viewport();
    viewport.scrollbar = ScrollbarState {
        total: u32::from(ROWS),
        offset: 0,
        len: u32::from(ROWS),
    };
    viewport
}

struct FloatTerminal {
    id: u64,
    title: &'static str,
    active: bool,
    gaps: bool,
    viewport: Arc<TerminalViewport>,
    history: Rc<History>,
    appearance: Arc<TerminalAppearance>,
    revisions: Arc<[u64]>,
    cache: Rc<RefCell<RowRenderCache>>,
}

impl FloatTerminal {
    fn new(id: u64, shown: Shown, active: bool, gaps: bool, cx: &App) -> Self {
        let all = shell_lines();
        let (viewport, history, title) = match shown {
            Shown::Live => {
                let (viewport, history) = window_of(&all, all.rows - ROWS, true);
                (viewport, history, "fish")
            }
            Shown::Back => {
                let (viewport, history) = window_of(&all, 9, false);
                (viewport, history, "fish")
            }
            Shown::Program => (
                program(),
                History(Vec::new(), Arc::new(TerminalDictionary::default())),
                "Neovim",
            ),
        };
        let mut appearance = appearance();
        let local = zz_terminal::LocalColors::new(
            viewport.foreground,
            viewport.background,
            &appearance.palette,
            zz_ui::terminal::theme_terminal_colors(cx),
        );
        let viewport = local.viewport(&viewport);
        appearance.local_colors = Some(local);
        Self {
            id,
            title,
            active,
            gaps,
            viewport: Arc::new(viewport),
            history: Rc::new(history),
            appearance: Arc::new(appearance),
            revisions: (1..=u64::from(ROWS)).collect(),
            cache: Rc::default(),
        }
    }
}

fn header(id: u64, icon: IconName, title: &'static str, active: bool, cx: &App) -> AnyElement {
    let inactive = crate::pane().inactive;
    pane_header_frame(
        pane_header(
            icon,
            active,
            title,
            Some(FOLDER.into()),
            PaneDragSource::new(PaneId(id), title, true, |_, _, _| {}),
            PaneState::default(),
            None,
            &[],
            &PaneActionKeys::default(),
            px(0.0),
            |_, _, _| {},
            cx,
        ),
        radii(true, cx),
        if active { 1.0 } else { inactive },
    )
    .into_any_element()
}

fn band(active: bool, gaps: bool, cx: &App) -> HeaderBand {
    HeaderPlace::Over
        .band(
            tiled_pane_ground(cx.theme().background, gaps, active, cx),
            cx,
        )
        .expect("a floating header has a band")
}

#[derive(Clone, Copy)]
struct Look {
    id: u64,
    active: bool,
    gaps: bool,
}

impl Look {
    fn framed(
        self,
        size: ala::Size<Pixels>,
        header: AnyElement,
        content: AnyElement,
        scrim: bool,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .flex()
            .w(size.width)
            .h(size.height)
            .child(pane_surface(
                ("floating-header-pane", self.id),
                headed_pane(HeaderPlace::Over, header, content, None, cx),
                Vec::new(),
                chrome(self.active, scrim, self.gaps, cx),
                cx,
            ))
    }
}

impl FloatTerminal {
    const fn look(&self) -> Look {
        Look {
            id: self.id,
            active: self.active,
            gaps: self.gaps,
        }
    }
}

impl Render for FloatTerminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = Arc::clone(&self.viewport);
        let history = Rc::clone(&self.history);
        let appearance = Arc::clone(&self.appearance);
        let revisions = Arc::clone(&self.revisions);
        let prepaint_cache = Rc::clone(&self.cache);
        let paint_cache = Rc::clone(&self.cache);
        let band = band(self.active, self.gaps, cx);
        let (_, cell_height) = cell_metrics(&appearance, window, cx);
        let font = terminal_font_for_style(&appearance, &cx.theme().mono_font_family, false, false);
        let font_size = px(appearance.font_size_points);
        let height = band.height
            + px(appearance.padding_top + appearance.padding_bottom)
            + cell_height * f32::from(ROWS);
        let focused = self.active;
        let text_opacity = if self.active {
            1.0
        } else {
            crate::pane().inactive
        };
        let terminal = terminal_text(div(), font, font_size)
            .size_full()
            .overflow_hidden()
            .pl(px(appearance.padding_left))
            .pr(px(appearance.padding_right))
            .pt(px(appearance.padding_top) + band.height)
            .pb(px(appearance.padding_bottom))
            .line_height(px(appearance
                .cell_height_adjustment
                .apply(f32::from(font_size))
                .max(1.0)))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let header = band.scrim(window.content_mask().bounds);
                        prepaint_cache.borrow_mut().prepaint(
                            TerminalRenderInput {
                                viewport: &viewport,
                                row_revisions: &revisions,
                                revision_epoch: 0,
                                history: Some(&*history),
                                images: None,
                                local_scroll_target: None,
                                scroll_pixel_offset: px(0.0),
                                overscroll: px(0.0),
                                scrollbar_opacity: 1.0,
                                extra_height: px(0.0),
                                command_output: false,
                                appearance: &appearance,
                                appearance_hash: appearance.stable_hash(),
                                text_opacity,
                                focused,
                                cursor_blink_visible: true,
                                marked_text: None,
                                rows_above: None,
                                rows_below: None,
                                header: Some(header),
                            },
                            bounds,
                            window,
                            cx,
                        )
                    },
                    move |bounds, mut paint, window, cx| {
                        paint_cache
                            .borrow_mut()
                            .paint(&mut paint, bounds, window, cx);
                    },
                )
                .size_full(),
            );
        self.look().framed(
            ala::size(px(WIDTH), height),
            header(
                self.id,
                IconName::SquareTerminal,
                self.title,
                self.active,
                cx,
            ),
            terminal.into_any_element(),
            false,
            cx,
        )
    }
}

struct Gallery(Vec<(String, AnyView)>);

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0
            .iter()
            .fold(states().columns(2), |states, (caption, view)| {
                states.state(caption.clone(), view.clone())
            })
    }
}

fn variants() -> impl Iterator<Item = (bool, bool, &'static str)> {
    [
        (true, true, "active · gaps"),
        (false, true, "inactive · gaps"),
        (true, false, "active · flush"),
        (false, false, "inactive · flush"),
    ]
    .into_iter()
}

fn terminals(_: &mut Window, cx: &mut App) -> AnyView {
    let mut items = Vec::new();
    let mut id = 1;
    for (shown, name) in [
        (Shown::Live, "live"),
        (Shown::Back, "scrolled back"),
        (Shown::Program, "full-screen program"),
    ] {
        for (active, gaps, caption) in variants() {
            let view = cx.new(|cx| FloatTerminal::new(id, shown, active, gaps, cx));
            items.push((format!("{name} · {caption}"), AnyView::from(view)));
            id += 1;
        }
    }
    cx.new(|_| Gallery(items)).into()
}

struct EmptyScreen {
    id: u64,
    active: bool,
    gaps: bool,
    search: Entity<InputState>,
    scroll: ScrollHandle,
}

impl EmptyScreen {
    const fn look(&self) -> Look {
        Look {
            id: self.id,
            active: self.active,
            gaps: self.gaps,
        }
    }
}

impl Render for EmptyScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let band = band(self.active, self.gaps, cx);
        let screen = EmptyChat {
            pane: self.id,
            provider: AgentProvider::ClaudeCode,
            folder: "zebrilus",
            rows: &[],
            selected: None,
            open: None,
            search: &self.search,
            scroll: &self.scroll,
            inset_top: band.height,
        }
        .render(|_, _, _, _| {}, |_, _, _| {}, |_, _, _| {}, cx);
        let content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .text_color(cx.theme().foreground)
            .child(screen)
            .child(header_scrim(band));
        self.look().framed(
            ala::size(px(WIDTH), px(CHAT_HEIGHT)),
            header(self.id, zz_ui::CLAUDE_ICON, "Claude Code", self.active, cx),
            content.into_any_element(),
            true,
            cx,
        )
    }
}

struct ScrolledChat {
    id: u64,
    active: bool,
    gaps: bool,
    thread: AnyView,
}

impl ScrolledChat {
    const fn look(&self) -> Look {
        Look {
            id: self.id,
            active: self.active,
            gaps: self.gaps,
        }
    }
}

impl Render for ScrolledChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let band = band(self.active, self.gaps, cx);
        let content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(self.thread.clone())
            .child(header_scrim(band));
        self.look().framed(
            ala::size(px(WIDTH), px(CHAT_HEIGHT)),
            header(
                self.id,
                zz_ui::CLAUDE_ICON,
                "Find agent state",
                self.active,
                cx,
            ),
            content.into_any_element(),
            true,
            cx,
        )
    }
}

fn conversation() -> Script {
    let mut script = Script::new();
    script
        .turn("The header cuts the sky off at its bottom edge, and chat text stops dead there too. Can it sit over the content?")
        .ran("r1", ItemKind::Read, "crates/zz/src/workspace/view.rs")
        .ran("r2", ItemKind::Read, "crates/zz-ui/src/pane.rs")
        .reply("a1", "Today a pane is two flex rows: the header, 34px tall, then the content. Nothing paints under the header, so whatever the content draws ends at its edge.\n\nIf the header goes over the content, the content takes the whole pane and gets a top inset of the header's height instead.")
        .finish(completed())
        .turn("Keep the first row of a full-screen program clear. Scrollback is fine under it, and so are chat messages when I scroll up.")
        .ran("e1", ItemKind::Edit, "crates/zz-ui/src/pane.rs")
        .ran("e2", ItemKind::Edit, "crates/zz-ui/src/terminal.rs")
        .ran("x1", ItemKind::Execute, "cargo test -p zz-ui terminal")
        .reply("a2", "The live screen starts right under the header, so the grid keeps its rows. Scrolled back, history rows draw into the band above row 0 behind the scrim.")
        .finish(completed())
        .turn("Show it in the storybook.")
        .reply("a3", "Added a Floating header story with a live shell, the same shell scrolled back, a full-screen program, the empty chat and this chat.")
        .finish(completed());
    script
}

fn chats(window: &mut Window, cx: &mut App) -> AnyView {
    let mut items = Vec::new();
    let mut id = 100;
    for (active, gaps, caption) in variants() {
        let search = cx.new(|cx| InputState::new(window, cx));
        let view = cx.new(|_| EmptyScreen {
            id,
            active,
            gaps,
            search,
            scroll: ScrollHandle::new(),
        });
        items.push((format!("empty · {caption}"), AnyView::from(view)));
        id += 1;
    }
    for (active, gaps, caption) in variants() {
        let header = cx.theme().pane_header_height();
        let thread = Thread::new(conversation())
            .framed(Frame::Scroll(None))
            .inset_top(header)
            .view(window, cx);
        let view = cx.new(|_| ScrolledChat {
            id,
            active,
            gaps,
            thread: thread.into(),
        });
        items.push((format!("scrolled · {caption}"), AnyView::from(view)));
        id += 1;
    }
    cx.new(|_| Gallery(items)).into()
}
