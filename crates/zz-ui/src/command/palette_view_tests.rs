use std::cell::RefCell;

use ala::{TestAppContext, VisualTestContext};
use zz_client::recents::RecentKey;
use zz_protocol::{ChooseTreeItem, ChooseTreeKind, PathKind, SessionId, WindowId};

use super::*;
use crate::{
    ActiveTheme as _, Colorize as _, Elevation, IconName,
    command::{
        ActionContext, PaletteStatus, PaletteTreeHost, PaletteTreePane, PaletteTreeSession,
        PaletteTreeWindow, record_recent,
    },
    files::{ListingSink, ListingUpdate},
};
use penna::Root;

const LOCAL: PaletteHostId = PaletteHostId(0);

#[derive(Debug, PartialEq)]
enum Call {
    Input(InputMessage),
    Key(PaneId, String),
    Execute(PaletteHostId, CommandInvocation),
    Connect(PaletteHostId),
    Activate(PaletteHostId, PaletteTarget),
    List(PaletteHostId, String, Option<PaneId>),
    Insert(PaneId, String),
}

type Calls = Rc<RefCell<Vec<Call>>>;
type Sink = Rc<RefCell<Option<ListingSink>>>;

struct FakeBackend {
    calls: Calls,
    snapshot: Arc<MuxSnapshot>,
    tree: Rc<RefCell<PaletteTree>>,
    availability: PaneKindAvailability,
    levels: Option<Rc<RefCell<Vec<String>>>>,
    host: Option<&'static MuxSnapshot>,
    active: PaneId,
    sink: Sink,
    files: Option<std::rc::Weak<FakeBackend>>,
    vim: Rc<std::cell::Cell<bool>>,
}

struct FakeLevel(Rc<RefCell<Vec<String>>>);

impl PaletteActionLevel for FakeLevel {
    fn update(&mut self, context: &ActionContext, _: &mut Window, _: &mut App) -> LevelEffect {
        self.0
            .borrow_mut()
            .push(format!("update {:?} {}", context.kind, context.rest));
        LevelEffect::Stay
    }

    fn key(&mut self, _: ActionKey, _: &mut Window, _: &mut App) -> Option<LevelEffect> {
        None
    }

    fn rows(&mut self, _: &mut Window, _: &mut App) -> ala::AnyElement {
        div().into_any_element()
    }

    fn hints(&self, _: &App) -> Vec<PaletteHint> {
        Vec::new()
    }

    fn open_recent(&mut self, key: &RecentKey, _: &mut Window, _: &mut App) -> LevelEffect {
        self.0.borrow_mut().push(format!("recent {key:?}"));
        LevelEffect::Stay
    }
}

impl PaletteBackend for FakeBackend {
    fn snapshot(&self, _: &App) -> Arc<MuxSnapshot> {
        Arc::clone(&self.snapshot)
    }

    fn host_snapshot<'a>(&self, _: PaletteHostId, _: &'a App) -> Option<&'a MuxSnapshot> {
        self.host
    }

    fn tree(&self, _: &App) -> PaletteTree {
        self.tree.borrow().clone()
    }

    fn settings(&self, _: &App) -> PaletteSettings {
        PaletteSettings {
            vim: self.vim.get(),
            ..PaletteSettings::default()
        }
    }

    fn availability(&self, _: &App) -> PaneKindAvailability {
        self.availability
    }

    fn command_shortcut(&self, _: &str, _: &App) -> Option<SharedString> {
        None
    }

    fn mono_font(&self, _: &App) -> SharedString {
        "Lilex".into()
    }

    fn send_input(&self, input: InputMessage, _: &mut App) {
        self.calls.borrow_mut().push(Call::Input(input));
    }

    fn send_key(&self, pane: PaneId, event: &KeyDownEvent, _: &mut App) {
        self.calls
            .borrow_mut()
            .push(Call::Key(pane, event.keystroke.key.clone()));
    }

    fn active_pane(&self, _: &App) -> Option<PaneId> {
        Some(self.active)
    }

    fn execute(&self, host: PaletteHostId, command: CommandInvocation, _: &mut App) {
        self.calls.borrow_mut().push(Call::Execute(host, command));
    }

    fn connect_host(&self, host: PaletteHostId, _: &mut App) {
        self.calls.borrow_mut().push(Call::Connect(host));
    }

    fn action_level(&self, kind: ActionKind, _: &mut App) -> Option<Box<dyn PaletteActionLevel>> {
        if let Some(backend) = self.files.as_ref().and_then(std::rc::Weak::upgrade) {
            return (kind == ActionKind::Files).then(|| {
                Box::new(crate::files::FilesAction::new(backend)) as Box<dyn PaletteActionLevel>
            });
        }
        self.levels
            .as_ref()
            .map(|log| Box::new(FakeLevel(Rc::clone(log))) as Box<dyn PaletteActionLevel>)
    }

    fn activate(&self, host: PaletteHostId, target: PaletteTarget, _: &mut App) -> bool {
        self.calls.borrow_mut().push(Call::Activate(host, target));
        true
    }

    fn list_path(
        &self,
        host: PaletteHostId,
        dir: &str,
        anchor: Option<PaneId>,
        _: &mut App,
    ) -> Result<u64, SharedString> {
        self.calls
            .borrow_mut()
            .push(Call::List(host, dir.to_owned(), anchor));
        Ok(1)
    }

    fn insert_path(
        &self,
        pane: PaneId,
        _: PaletteHostId,
        _: &PathListRoot,
        entry: &PathEntry,
        _: &mut App,
    ) {
        self.calls
            .borrow_mut()
            .push(Call::Insert(pane, entry.rel.clone()));
    }

    fn feed_listings(&self, sink: ListingSink, _: &mut App) -> Option<Subscription> {
        self.sink.replace(Some(sink));
        None
    }

    fn home_dir(&self, _: PaletteHostId, _: &App) -> Option<String> {
        Some("/home/me".to_owned())
    }
}

fn fake_backend(
    tree: PaletteTree,
    availability: PaneKindAvailability,
) -> (Rc<dyn PaletteBackend>, Calls) {
    let calls = Calls::default();
    let backend = FakeBackend {
        calls: Rc::clone(&calls),
        snapshot: Arc::default(),
        tree: Rc::new(RefCell::new(tree)),
        availability,
        levels: None,
        host: None,
        active: PaneId(0),
        sink: Sink::default(),
        files: None,
        vim: Rc::default(),
    };
    (Rc::new(backend), calls)
}

fn files_backend() -> (Rc<dyn PaletteBackend>, Calls, Sink) {
    let snapshot: &'static MuxSnapshot = Box::leak(Box::new(crate::files::tests::snapshot()));
    let calls = Calls::default();
    let sink = Sink::default();
    let backend = Rc::new_cyclic(|me| FakeBackend {
        calls: Rc::clone(&calls),
        snapshot: Arc::new(snapshot.clone()),
        tree: Rc::new(RefCell::new(online_tree())),
        availability: PaneKindAvailability::default(),
        levels: None,
        host: Some(snapshot),
        active: PaneId(10),
        sink: Rc::clone(&sink),
        files: Some(me.clone()),
        vim: Rc::default(),
    });
    (backend, calls, sink)
}

fn feed(cx: &mut VisualTestContext, sink: &Sink, update: ListingUpdate) {
    cx.update(|_, cx| (sink.borrow().as_ref().expect("fed"))(LOCAL, 1, update, cx));
    cx.run_until_parked();
}

fn entry(rel: &str, kind: PathKind) -> PathEntry {
    PathEntry {
        rel: rel.to_owned(),
        kind,
        symlink: false,
    }
}

fn list(cx: &mut VisualTestContext, sink: &Sink, root: &str, entries: Vec<PathEntry>) {
    feed(
        cx,
        sink,
        ListingUpdate::Begin(Ok(PathListRoot {
            root: root.to_owned(),
            display_root: root.replacen("/home/me", "~", 1),
            cwd: Some("/home/me/one".to_owned()),
            insert: zz_protocol::InsertStyle::Shell(zz_protocol::ShellKind::Posix),
        })),
    );
    feed(
        cx,
        sink,
        ListingUpdate::Chunk {
            entries: entries.into(),
            done: true,
            truncated: false,
        },
    );
}

fn list_one(cx: &mut VisualTestContext, sink: &Sink) {
    list(
        cx,
        sink,
        "/home/me/one",
        vec![
            entry("assets/", PathKind::Dir),
            entry("crates/", PathKind::Dir),
            entry("README.md", PathKind::File),
            entry("crates/zz/", PathKind::Dir),
            entry("crates/zz/src/", PathKind::Dir),
            entry("crates/zz/src/main.rs", PathKind::File),
        ],
    );
}

fn level_backend(tree: PaletteTree) -> (Rc<dyn PaletteBackend>, Rc<RefCell<Vec<String>>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let backend = FakeBackend {
        calls: Calls::default(),
        snapshot: Arc::default(),
        tree: Rc::new(RefCell::new(tree)),
        availability: PaneKindAvailability {
            browser: true,
            editor: true,
        },
        levels: Some(Rc::clone(&log)),
        host: None,
        active: PaneId(0),
        sink: Sink::default(),
        files: None,
        vim: Rc::default(),
    };
    (Rc::new(backend), log)
}

fn backend(tree: PaletteTree) -> (Rc<dyn PaletteBackend>, Calls) {
    fake_backend(
        tree,
        PaneKindAvailability {
            browser: true,
            editor: true,
        },
    )
}

fn local_host(status: PaletteStatus, sessions: Vec<PaletteTreeSession>) -> PaletteTree {
    PaletteTree {
        attached: LOCAL,
        chats: Vec::new(),
        chat_mode: false,
        hosts: vec![PaletteTreeHost {
            id: LOCAL,
            name: "local".to_owned(),
            detail: "This computer".to_owned(),
            right: String::new(),
            status,
            sessions,
        }],
    }
}

fn offline_tree() -> PaletteTree {
    local_host(PaletteStatus::Offline, Vec::new())
}

fn online_tree() -> PaletteTree {
    local_host(
        PaletteStatus::Online,
        vec![PaletteTreeSession {
            id: SessionId(1),
            name: "dev".to_owned(),
            tmux_name: "dev".to_owned(),
            active: true,
            windows: vec![PaletteTreeWindow {
                id: WindowId(1),
                index: 1,
                name: "editor".to_owned(),
                active: true,
                active_pane: PaneId(10),
                panes: vec![PaletteTreePane {
                    id: PaneId(10),
                    index: 0,
                    label: "nvim".to_owned(),
                    detail: "Terminal".into(),
                    program: None,
                    icon: IconName::SquareTerminal,
                    running: false,
                    needs_you: false,
                    terminal: true,
                    cwd: String::new(),
                }],
            }],
        }],
    )
}

fn target_value(palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext) -> String {
    palette.read_with(cx, |palette, cx| palette.input.read(cx).value().to_string())
}

fn sent(calls: &Calls, action: &ChooseTreeAction) -> bool {
    calls.borrow().iter().any(|call| {
        matches!(call, Call::Input(InputMessage::ChooseTree { action: sent }) if sent == action)
    })
}

fn mount(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<CommandPaletteView>) -> CommandPaletteView,
) -> (Entity<CommandPaletteView>, &mut VisualTestContext) {
    let slot = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let palette = cx.new(|cx| build(window, cx));
        palette.read(cx).focus(cx).focus(window, cx);
        captured.replace(Some(palette.clone()));
        Root::new(palette, window, cx)
    });
    let palette = slot.borrow_mut().take().expect("palette captured");
    (palette, cx)
}

fn prompt_state(input: &str, mode: CommandPromptMode) -> CommandPromptState {
    CommandPromptState {
        prompt: ":".to_owned(),
        input: input.to_owned(),
        cursor: u32::try_from(input.chars().count()).unwrap_or(u32::MAX),
        kind: CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: CommandPromptType::Command,
        mode,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    }
}

#[test]
fn chooser_search_expands_descendants_and_restores_only_its_own_branches() {
    let item = |target, flags| ChooseTreeItem {
        label: String::new(),
        detail: String::new(),
        target,
        depth: 0,
        flags,
        pane_kind: None,
        key: String::new(),
        text: String::new(),
    };
    let mut state = ChooseTreeState {
        items: vec![
            item(
                ChooseTreeTarget::Session(SessionId(0)),
                ChooseTreeItem::HAS_CHILDREN | ChooseTreeItem::EXPANDED,
            ),
            item(
                ChooseTreeTarget::Window(WindowId(0)),
                ChooseTreeItem::HAS_CHILDREN,
            ),
            item(
                ChooseTreeTarget::Session(SessionId(1)),
                ChooseTreeItem::HAS_CHILDREN,
            ),
        ],
        search: None,
        selected: 0,
        kind: ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    };
    let mut search = ChooserSearch::default();
    assert_eq!(search.advance(&state, true), Some((1, true)));
    state.selected = 1;
    assert_eq!(search.advance(&state, true), None);
    state.items[1].flags |= ChooseTreeItem::EXPANDED;
    state
        .items
        .insert(2, item(ChooseTreeTarget::Pane(PaneId(0)), 0));
    assert_eq!(search.advance(&state, true), Some((3, true)));
    state.items[3].flags |= ChooseTreeItem::EXPANDED;
    state.items.push(item(
        ChooseTreeTarget::Window(WindowId(1)),
        ChooseTreeItem::HAS_CHILDREN,
    ));
    assert_eq!(search.advance(&state, true), Some((4, true)));
    state.items[4].flags |= ChooseTreeItem::EXPANDED;
    assert_eq!(search.advance(&state, true), None);
    assert_eq!(search.advance(&state, false), Some((4, false)));
    assert_eq!(search.advance(&state, false), None);
    state.items[4].flags &= !ChooseTreeItem::EXPANDED;
    assert_eq!(search.advance(&state, false), Some((3, false)));
    state.items[3].flags &= !ChooseTreeItem::EXPANDED;
    state.items.pop();
    assert_eq!(search.advance(&state, false), Some((1, false)));
    state.items[1].flags &= !ChooseTreeItem::EXPANDED;
    state.items.remove(2);
    assert_eq!(search.advance(&state, false), None);
    assert!(search.expanded.is_empty());
    assert!(state.items[0].expanded());
}

#[ala::test]
fn daemon_tree_navigation_and_search_preserve_pane_activation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::init(cx);
        cx.set_reduce_motion(false);
    });
    let session = ChooseTreeItem {
        label: "workspace".to_owned(),
        detail: "1 window".to_owned(),
        target: ChooseTreeTarget::Session(SessionId(0)),
        depth: 0,
        flags: ChooseTreeItem::HAS_CHILDREN | ChooseTreeItem::ACTIVE,
        pane_kind: None,
        key: String::new(),
        text: String::new(),
    };
    let mut state = ChooseTreeState {
        items: vec![session.clone()],
        search: None,
        selected: 0,
        kind: ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    };
    let initial = state.clone();
    let (backend, messages) = backend(offline_tree());
    let (palette, cx) = cx.add_window_view(move |window, cx| {
        let palette = CommandPaletteView::new_window_chooser(backend, &initial, 1, window, cx);
        palette.focus(cx).focus(window, cx);
        palette
    });
    messages.borrow_mut().clear();
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    std::thread::sleep(Duration::from_millis(200));
    cx.update(Window::simulate_next_frame);
    cx.run_until_parked();
    let surface_top = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            let _ = window.draw(cx);
            window
                .painted_quads()
                .into_iter()
                .find(|quad| {
                    quad.background
                        == ala::solid_background(
                            cx.theme().surface(Elevation::Popover).fill.opaque(),
                        )
                        && quad.bounds.size.width.0 > 300.0 * window.scale_factor()
                })
                .expect("palette stays fully opaque after chooser updates")
                .bounds
                .origin
                .y
        })
    };
    let settled_top = surface_top(cx);
    palette.read_with(cx, |palette, _| {
        assert_eq!(palette.unified.as_ref().unwrap().rows.len(), 1);
        assert_eq!(palette.selected, Some(0));
    });
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert!(sent(&messages, &ChooseTreeAction::Expand));
    state.items[0].flags |= ChooseTreeItem::EXPANDED;
    let mut child = session;
    child.label = "editor".to_owned();
    child.target = ChooseTreeTarget::Window(WindowId(0));
    child.depth = 1;
    state.items.push(child.clone());
    palette.update_in(cx, |palette, window, cx| {
        palette.synchronize_window_chooser(&state, 2, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(surface_top(cx), settled_top);
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, _| palette.selected),
        Some(1)
    );
    messages.borrow_mut().clear();
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert!(sent(&messages, &ChooseTreeAction::Select(1)));
    state.items[1].flags |= ChooseTreeItem::EXPANDED;
    child.label = "shell".to_owned();
    child.target = ChooseTreeTarget::Pane(PaneId(0));
    child.depth = 2;
    child.flags = ChooseTreeItem::ACTIVE;
    state.items.push(child);
    palette.update_in(cx, |palette, window, cx| {
        palette.synchronize_window_chooser(&state, 3, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(surface_top(cx), settled_top);
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, _| palette.selected),
        Some(2)
    );
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, _| palette.selected),
        Some(1)
    );
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert!(cx.update(|window, cx| palette.read(cx).focus(cx).is_focused(window)));
    let mut other = state.items[0].clone();
    other.target = ChooseTreeTarget::Session(SessionId(1));
    other.flags = ChooseTreeItem::HAS_CHILDREN;
    other.label = "other".to_owned();
    state.items.insert(0, other);
    palette.update_in(cx, |palette, window, cx| {
        palette.synchronize_window_chooser(&state, 4, window, cx);
    });
    messages.borrow_mut().clear();
    cx.simulate_input("shell");
    cx.run_until_parked();
    assert!(sent(&messages, &ChooseTreeAction::Select(0)));
    cx.simulate_keystrokes("enter");
    assert!(!messages.borrow().iter().any(|message| matches!(
        message,
        Call::Input(InputMessage::ChooseTree {
            action: ChooseTreeAction::ActivateIndex(_)
        })
    )));
    state.items[0].flags |= ChooseTreeItem::EXPANDED;
    let mut other_window = state.items[2].clone();
    other_window.target = ChooseTreeTarget::Window(WindowId(1));
    other_window.flags = 0;
    state.items.insert(1, other_window);
    palette.update_in(cx, |palette, window, cx| {
        palette.synchronize_window_chooser(&state, 5, window, cx);
    });
    assert!(sent(&messages, &ChooseTreeAction::ActivateIndex(4)));
}

#[ala::test]
fn palette_navigation_scrolls_at_most_one_row_per_step(cx: &mut TestAppContext) {
    cx.update(crate::init);
    for unified in [true, false] {
        let (backend, _) = backend(offline_tree());
        let (palette, cx) = mount(cx, move |window, cx| {
            let mut palette =
                CommandPaletteView::new_local(backend, Some(PaletteMode::Command), window, cx);
            if !unified {
                palette.unified = None;
            }
            palette
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let mut previous_offset = px(0.0);
        let mut scrolled = false;
        for direction in [1, -1] {
            for _ in 0..24 {
                cx.update(|_, cx| {
                    palette.update(cx, |palette, cx| {
                        palette.navigate(direction, cx);
                    });
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    let _ = window.draw(cx);
                });
                let (offset, selected, row_count) = palette.read_with(cx, |palette, _| {
                    (
                        palette.scroll_handle.0.borrow().base_handle.offset().y,
                        palette.selected.expect("selected command"),
                        palette
                            .unified
                            .as_ref()
                            .map_or(palette.suggestions.len(), |state| state.rows.len()),
                    )
                });
                if direction < 0 && selected == row_count - 1 {
                    break;
                }
                let delta = (offset - previous_offset).abs();
                assert!(
                    delta <= px(crate::command::COMMAND_PALETTE_ROW_HEIGHT + 0.01),
                    "unified={unified}, direction={direction}, selected={selected}, delta={delta:?}"
                );
                scrolled |= delta > px(0.0);
                previous_offset = offset;
            }
        }
        assert!(scrolled, "the command list should exceed the viewport");
    }
}

#[ala::test]
fn sigils_stay_in_the_field_and_backspace_drops_the_host_chip(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, _) = backend(offline_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    let chips = |palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext| {
        palette.read_with(cx, |palette, _| {
            palette
                .unified
                .as_ref()
                .unwrap()
                .chips
                .iter()
                .map(|chip| (chip.label.clone(), chip.auto))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(chips(&palette, cx), vec![("local".to_owned(), true)]);
    cx.simulate_input(":");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), ":");
    assert_eq!(
        palette.read_with(cx, |palette, _| palette.unified.as_ref().unwrap().mode),
        None
    );
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    palette.read_with(cx, |palette, _| {
        let state = palette.unified.as_ref().unwrap();
        assert!(state.rows.len() > 11);
        assert_eq!(palette.selected, Some(state.rows.len() - 1));
    });
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, _| palette.selected),
        Some(0)
    );
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "");
    assert_eq!(chips(&palette, cx), vec![("local".to_owned(), true)]);
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert!(chips(&palette, cx).is_empty());
    assert!(cx.update(|window, cx| palette.read(cx).focus(cx).is_focused(window)));
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
}

#[ala::test]
fn local_palette_activates_targets_through_the_backend(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, Some(PaletteMode::Target), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "");
    cx.simulate_input("editor");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Activate(LOCAL, PaletteTarget::Window(WindowId(1)))]
    );
}

#[ala::test]
fn alt_enter_copies_the_highlighted_place_as_tmux_writes_it(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("alt-enter");
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("@local:$1")
    );
    cx.simulate_input("editor");
    cx.run_until_parked();
    cx.dispatch_action(CopyAddress);
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("@local:@1")
    );
    assert!(calls.borrow().is_empty());
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));
}

#[ala::test]
fn a_level_can_replace_the_field(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, _) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_target(backend, "~/dev/zz/", None, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/dev/zz/");
    palette.update_in(cx, |palette, window, cx| {
        palette.apply_level_effect(LevelEffect::SetRest("~/dev/".to_owned()), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/dev/");
}

#[ala::test]
fn a_recent_file_opens_its_level_on_the_files_folder(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let key = RecentKey::File {
        host: "local".to_owned(),
        path: "~/dev/zz/Cargo.toml".to_owned(),
        dir: false,
    };
    let recorded = key.clone();
    cx.update(|cx| record_recent(recorded, "Cargo.toml", cx));
    let (backend, log) = level_backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    let selected = palette.read_with(cx, |palette, _| {
        let state = palette.unified.as_ref().unwrap();
        state.rows[palette.selected.unwrap()].row.label.to_string()
    });
    assert_eq!(selected, "Cargo.toml");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/dev/zz/Cargo.toml");
    assert_eq!(
        *log.borrow(),
        [
            "update Files ~/dev/zz/Cargo.toml".to_owned(),
            format!("recent {key:?}"),
        ]
    );
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));
}

fn insert_path(cx: &mut VisualTestContext) {
    cx.simulate_input("insert path");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

fn chip_labels(palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext) -> Vec<String> {
    palette.read_with(cx, |palette, _| {
        palette
            .unified
            .as_ref()
            .unwrap()
            .chips
            .iter()
            .map(|chip| chip.label.clone())
            .collect()
    })
}

#[ala::test]
fn tab_goes_into_a_place_and_shift_tab_comes_back_to_it(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev"]);
    assert_eq!(target_value(&palette, cx), "");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev", "1 editor"]);
    cx.simulate_keystrokes("shift-tab");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev"]);
    let selected = palette.read_with(cx, |palette, _| {
        let state = palette.unified.as_ref().unwrap();
        state.rows[palette.selected.unwrap()].row.label.to_string()
    });
    assert_eq!(selected, "1 editor");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Activate(LOCAL, PaletteTarget::Window(WindowId(1)))]
    );
}

#[ala::test]
fn words_find_a_window_and_backspace_drops_the_chips(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.simulate_input("dev 1");
    cx.run_until_parked();
    let selected = palette.read_with(cx, |palette, _| {
        let state = palette.unified.as_ref().unwrap();
        state.rows[palette.selected.unwrap()].row.label.to_string()
    });
    assert_eq!(selected, "dev › 1 editor");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev", "1 editor"]);
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev"]);
    cx.simulate_keystrokes("backspace backspace");
    cx.run_until_parked();
    assert!(chip_labels(&palette, cx).is_empty());
    assert!(calls.borrow().is_empty());
}

#[ala::test]
fn a_verb_on_a_place_runs_with_its_target(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("kill window");
    cx.run_until_parked();
    assert_eq!(chip_labels(&palette, cx), ["local", "dev", "1 editor"]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("kill-window", ["-t", "@1"])
        )]
    );
}

#[ala::test]
fn a_verb_opens_a_picker_for_its_target_with_the_current_one_first(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.simulate_input("read pane");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (selected, verb) = palette.read_with(cx, |palette, _| {
        let state = palette.unified.as_ref().unwrap();
        (
            state.rows[palette.selected.unwrap()].row.right.to_string(),
            state.draft.as_ref().map(|draft| draft.verb.name.clone()),
        )
    });
    assert_eq!(selected, "current");
    assert_eq!(verb.as_deref(), Some("read pane"));
    assert!(calls.borrow().is_empty());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("capture-pane", ["-p", "-t", "%10"])
        )]
    );
}

#[ala::test]
fn a_text_slot_takes_what_you_type_and_backspace_steps_back(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.simulate_input("rename window");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter enter");
    cx.run_until_parked();
    let placeholder = palette.read_with(cx, |palette, _| {
        palette.unified.as_ref().unwrap().placeholder()
    });
    assert_eq!(placeholder, "New name");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let filled = palette.read_with(cx, |palette, _| {
        palette
            .unified
            .as_ref()
            .unwrap()
            .draft
            .as_ref()
            .map(|draft| draft.filled.len())
    });
    assert_eq!(filled, Some(0), "backspace takes the window back");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("api logs");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("rename-window", ["-t", "@1", "api logs"])
        )]
    );
}

#[ala::test]
fn a_pasted_written_place_becomes_chips(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, _) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.write_to_clipboard(ala::ClipboardItem::new_string("@local:%10".to_owned()));
    cx.dispatch_action(crate::input::Paste);
    cx.run_until_parked();
    assert_eq!(
        chip_labels(&palette, cx),
        ["local", "dev", "1 editor", "0 nvim"]
    );
    assert_eq!(target_value(&palette, cx), "");
}

#[ala::test]
fn prefix_f_lists_the_panes_folder_and_inserts_into_that_pane(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_target(backend, "~/dev/zz/", Some(PaneId(10)), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        *calls.borrow(),
        [Call::List(LOCAL, "~/dev/zz/".to_owned(), Some(PaneId(10)))]
    );
    let picker = palette.read_with(cx, |palette, _| palette.target.as_ref().unwrap().0.clone());
    picker.update(cx, |picker, cx| {
        picker.apply_begin(
            "local",
            1,
            Ok(PathListRoot {
                root: "/home/fab/dev/zz".to_owned(),
                display_root: "~/dev/zz".to_owned(),
                cwd: Some("/home/fab/dev/zz".to_owned()),
                insert: zz_protocol::InsertStyle::Shell(zz_protocol::ShellKind::Posix),
            }),
            cx,
        );
        let entries: Arc<[PathEntry]> = Arc::from([
            PathEntry {
                rel: "crates/".to_owned(),
                kind: zz_protocol::PathKind::Dir,
                symlink: false,
            },
            PathEntry {
                rel: "crates/zz/src/main.rs".to_owned(),
                kind: zz_protocol::PathKind::File,
                symlink: false,
            },
        ]);
        picker.apply_chunk("local", 1, &entries, true, false, cx);
    });
    cx.simulate_input("mainrs");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Insert(
            PaneId(10),
            "crates/zz/src/main.rs".to_owned()
        ))
    );
}

#[ala::test]
fn prefix_f_opens_files_on_the_panes_folder_and_inserts_into_that_pane(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls, sink) = files_backend();
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_target(backend, "", Some(PaneId(20)), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/two/");
    assert_eq!(
        *calls.borrow(),
        [Call::List(LOCAL, "~/two/".to_owned(), Some(PaneId(20)))]
    );
    list_one(cx, &sink);
    cx.simulate_input("mainrs");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Insert(
            PaneId(20),
            "crates/zz/src/main.rs".to_owned()
        ))
    );
}

#[ala::test]
fn files_open_folders_and_go_back_up_to_the_folder_left(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls, sink) = files_backend();
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    insert_path(cx);
    assert_eq!(
        target_value(&palette, cx),
        "~/one/",
        "insert path starts in the focused pane's folder"
    );
    list_one(cx, &sink);

    cx.simulate_keystrokes("down tab");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/one/crates/");
    cx.simulate_keystrokes("shift-tab");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/one/");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(
        target_value(&palette, cx),
        "~/one/crates/",
        "going up highlights the folder left"
    );
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/one/");
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/one/crates/");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/one/");

    cx.simulate_input("cr");
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(
        target_value(&palette, cx),
        "~/one/cr",
        "Left moves the caret while there is a filter"
    );
    cx.simulate_keystrokes("right backspace backspace backspace");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), "~/");
    assert_eq!(
        *calls.borrow(),
        [
            Call::List(LOCAL, "~/one/".to_owned(), Some(PaneId(10))),
            Call::List(LOCAL, "~/".to_owned(), Some(PaneId(10))),
        ],
        "folders under a finished walk are not listed again"
    );
}

#[ala::test]
fn a_folder_pick_hands_the_folder_back_instead_of_inserting(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls, sink) = files_backend();
    let picked = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&picked);
    let pick: super::FolderPick = Rc::new(move |host, folder, _, _| {
        seen.replace(Some((host, folder)));
    });
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_folder_pick(backend, "", pick, window, cx)
    });
    cx.run_until_parked();
    list_one(cx, &sink);
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        picked.borrow().clone(),
        Some((LOCAL, "~/one/crates".to_owned()))
    );
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|call| matches!(call, Call::Insert(..)))
    );
}

#[ala::test]
fn alt_enter_copies_the_plain_path_and_keeps_it_as_a_recent(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls, sink) = files_backend();
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    insert_path(cx);
    list_one(cx, &sink);
    cx.simulate_keystrokes("down alt-enter");
    cx.run_until_parked();
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("/home/me/one/crates".to_owned())
    );
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|call| matches!(call, Call::Insert(..)))
    );
    let recent = cx.update(|_, cx| {
        crate::command::palette_recents(cx)
            .top(zz_client::recents::RecentKind::File, 3, u64::MAX)
            .into_iter()
            .map(|recent| (recent.key.clone(), recent.label.clone()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        recent,
        [(
            RecentKey::File {
                host: "local".to_owned(),
                path: "/home/me/one/crates".to_owned(),
                dir: true,
            },
            "~/one/crates".to_owned()
        )]
    );
}

#[ala::test]
fn a_recent_file_goes_into_the_pane_once_its_folder_is_listed(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let path = "/home/me/one/crates/zz/src/main.rs";
    cx.update(|cx| {
        record_recent(
            RecentKey::File {
                host: "local".to_owned(),
                path: path.to_owned(),
                dir: false,
            },
            "~/one/crates/zz/src/main.rs",
            cx,
        );
    });
    let (backend, calls, sink) = files_backend();
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(target_value(&palette, cx), path);
    assert_eq!(
        *calls.borrow(),
        [Call::List(
            LOCAL,
            "/home/me/one/crates/zz/src/".to_owned(),
            Some(PaneId(10))
        )]
    );
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));
    list(
        cx,
        &sink,
        "/home/me/one/crates/zz/src",
        vec![
            entry("lib.rs", PathKind::File),
            entry("main.rs", PathKind::File),
        ],
    );
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Insert(PaneId(10), "main.rs".to_owned()))
    );
}

fn files_context(
    rest: &str,
    pane: Option<PaneId>,
    written: zz_client::place::LevelSet,
) -> ActionContext {
    ActionContext {
        kind: ActionKind::Files,
        fleet: Rc::new(zz_client::place::Fleet {
            hosts: Vec::new(),
            here: 0,
        }),
        place: zz_client::place::Place {
            host: None,
            session: None,
            window: None,
            pane,
        },
        written,
        host_auto: true,
        rest: rest.to_owned(),
        rest_range: 1..1 + rest.len(),
        insert_pane: Some(PaneId(10)),
        focus: None,
    }
}

fn set_rest(effect: LevelEffect) -> Option<String> {
    match effect {
        LevelEffect::SetRest(rest) => Some(rest),
        _ => None,
    }
}

fn files_update(
    action: &mut crate::files::FilesAction,
    context: &ActionContext,
    cx: &mut VisualTestContext,
) -> Option<String> {
    set_rest(cx.update(|window, cx| action.update(context, window, cx)))
}

fn files_key(
    action: &mut crate::files::FilesAction,
    key: ActionKey,
    cx: &mut VisualTestContext,
) -> Option<String> {
    cx.update(|window, cx| action.key(key, window, cx))
        .and_then(set_rest)
}

#[ala::test]
fn files_start_in_the_written_place_and_rewrite_their_text(cx: &mut TestAppContext) {
    use zz_client::place::{Level, LevelSet};

    cx.update(crate::init);
    let (backend, calls, sink) = files_backend();
    let mut action = crate::files::FilesAction::new(Rc::clone(&backend));
    let (_, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    let here = LevelSet::default();
    assert_eq!(
        files_update(&mut action, &files_context("", None, here), cx).as_deref(),
        Some("~/one/"),
        "this host alone: the pane a path goes into"
    );
    assert_eq!(
        files_update(&mut action, &files_context("~/one/", None, here), cx),
        None
    );
    list_one(cx, &sink);
    assert_eq!(files_key(&mut action, ActionKey::Down, cx), None);
    assert_eq!(
        files_key(&mut action, ActionKey::Tab, cx).as_deref(),
        Some("~/one/crates/")
    );
    files_update(&mut action, &files_context("~/one/crates/", None, here), cx);
    assert_eq!(
        files_key(&mut action, ActionKey::Backspace, cx).as_deref(),
        Some("~/one/")
    );
    let pane = files_context("", Some(PaneId(21)), here.with(Level::Pane));
    assert_eq!(
        files_update(&mut action, &pane, cx).as_deref(),
        Some("~/three/"),
        "a written pane: its folder"
    );
    let pane = files_context("~/three/", Some(PaneId(21)), here.with(Level::Pane));
    files_update(&mut action, &pane, cx);
    assert_eq!(
        *calls.borrow(),
        [
            Call::List(LOCAL, "~/one/".to_owned(), Some(PaneId(10))),
            Call::List(LOCAL, "~/three/".to_owned(), Some(PaneId(10))),
        ]
    );
}

#[ala::test]
fn local_palette_executes_commands_on_the_attached_host(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, Some(PaletteMode::Command), window, cx)
    });
    cx.simulate_input("new-window");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("new-window", [] as [&str; 0])
        )]
    );
}

#[ala::test]
fn palette_keeps_local_edits_for_the_same_revision_and_disables_value_completions(
    cx: &mut TestAppContext,
) {
    cx.update(crate::init);
    let mut initial = prompt_state("", CommandPromptMode::Text);
    initial.history = vec!["list-panes".to_owned()];
    let stale = initial.clone();
    let (backend, _) = backend(offline_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new(
            backend,
            &initial,
            1,
            Arc::new(MuxSnapshot::default()),
            window,
            cx,
        )
    });

    cx.update(|window, cx| {
        assert!(palette.read(cx).focus(cx).is_focused(window));
        let input = palette.read(cx).input.clone();
        input.update(cx, |input, cx| input.insert("ren", window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| palette.read(cx).last_input.clone()),
        "ren"
    );
    assert!(cx.update(|_, cx| !palette.read(cx).suggestions.is_empty()));

    cx.update(|window, cx| {
        palette.update(cx, |palette, cx| {
            palette.synchronize(&stale, 1, &Arc::new(MuxSnapshot::default()), window, cx);
        });
    });
    assert_eq!(
        cx.update(|_, cx| palette.read(cx).input.read(cx).value().to_string()),
        "ren"
    );

    let value = CommandPromptState {
        prompt: "rename-window: ".to_owned(),
        kind: CommandPromptKind::Value,
        ..prompt_state("notes", CommandPromptMode::Text)
    };
    cx.update(|window, cx| {
        palette.update(cx, |palette, cx| {
            palette.synchronize(&value, 2, &Arc::new(MuxSnapshot::default()), window, cx);
        });
    });
    assert!(cx.update(|_, cx| palette.read(cx).suggestions.is_empty()));
    assert_eq!(
        cx.update(|_, cx| palette.read(cx).input.read(cx).value().to_string()),
        "notes"
    );
}

#[test]
fn key_reading_prompts_relay_instead_of_editing() {
    for mode in [
        CommandPromptMode::Single,
        CommandPromptMode::Numeric,
        CommandPromptMode::Key,
    ] {
        assert!(CommandPaletteView::relays_keys(mode), "{mode:?}");
    }
    for mode in [
        CommandPromptMode::Text,
        CommandPromptMode::Incremental,
        CommandPromptMode::BackspaceExit,
    ] {
        assert!(!CommandPaletteView::relays_keys(mode), "{mode:?}");
    }
}

#[test]
fn only_a_known_command_followed_by_whitespace_has_arguments() {
    assert!(has_command_arguments("split-window -h"));
    assert!(!has_command_arguments("split-window"));
}

#[ala::test]
fn key_reading_prompt_relays_keystrokes_to_the_active_pane(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let state = CommandPromptState {
        prompt: "(key) ".to_owned(),
        ..prompt_state("", CommandPromptMode::Key)
    };
    let (backend, calls) = backend(offline_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new(
            backend,
            &state,
            1,
            Arc::new(MuxSnapshot::default()),
            window,
            cx,
        )
    });
    cx.simulate_keystrokes("a");
    cx.run_until_parked();
    assert_eq!(*calls.borrow(), [Call::Key(PaneId(0), "a".to_owned())]);
    assert!(cx.update(|_, cx| palette.read(cx).input.read(cx).value().is_empty()));
}

#[ala::test]
fn history_suggestions_skip_commands_the_backend_cannot_run(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut state = prompt_state("", CommandPromptMode::Text);
    state.history = vec![
        "set-editor-path /tmp/file".to_owned(),
        "set-browser-tabs []".to_owned(),
        "list-panes".to_owned(),
    ];
    let (backend, _) = fake_backend(
        offline_tree(),
        PaneKindAvailability {
            browser: false,
            editor: false,
        },
    );
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new(
            backend,
            &state,
            1,
            Arc::new(MuxSnapshot::default()),
            window,
            cx,
        )
    });
    let history = palette.read_with(cx, |palette, _| {
        palette
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.kind == CompletionKind::History)
            .map(|suggestion| suggestion.insertion.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(history, ["list-panes"]);
}

#[ala::test]
fn a_key_reading_prompt_drops_the_completion_list(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let initial = prompt_state("ren", CommandPromptMode::Text);
    let (backend, _) = backend(offline_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new(
            backend,
            &initial,
            1,
            Arc::new(MuxSnapshot::default()),
            window,
            cx,
        )
    });
    assert!(cx.update(|_, cx| !palette.read(cx).suggestions.is_empty()));

    for (revision, mode) in [
        (2, CommandPromptMode::Key),
        (3, CommandPromptMode::Numeric),
        (4, CommandPromptMode::Incremental),
    ] {
        let state = prompt_state("ren", mode);
        cx.update(|window, cx| {
            palette.update(cx, |palette, cx| {
                palette.synchronize(
                    &state,
                    revision,
                    &Arc::new(MuxSnapshot::default()),
                    window,
                    cx,
                );
            });
        });
        assert_eq!(cx.update(|_, cx| palette.read(cx).mode), mode);
        assert!(
            cx.update(|_, cx| palette.read(cx).suggestions.is_empty()),
            "{mode:?}"
        );
    }
}

#[ala::test]
fn tab_accepts_completion_without_leaving_the_palette(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let initial = prompt_state("new-w", CommandPromptMode::Text);
    let (backend, _) = backend(offline_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new(
            backend,
            &initial,
            1,
            Arc::new(MuxSnapshot::default()),
            window,
            cx,
        )
    });

    cx.simulate_keystrokes("tab");

    assert_eq!(
        cx.update(|_, cx| palette.read(cx).input.read(cx).value().to_string()),
        "new-window "
    );
    assert!(!palette.read_with(cx, |palette, _| palette.finishing));
    assert!(cx.update(|window, cx| palette.read(cx).focus(cx).is_focused(window)));
}

struct OverlayHost {
    palette: Entity<CommandPaletteView>,
    cached: bool,
}

impl Render for OverlayHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let overlay = if self.cached {
            ala::AnyView::from(self.palette.clone())
                .cached(ala::StyleRefinement::default().absolute().inset_0())
                .into_any_element()
        } else {
            self.palette.clone().into_any_element()
        };
        ala::div()
            .relative()
            .flex()
            .size_full()
            .child(ala::div().flex_1())
            .child(overlay)
    }
}

#[ala::test]
fn a_cached_palette_covers_the_same_area_as_an_inline_one(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut covered = Vec::new();
    for unified in [true, false] {
        for cached in [false, true] {
            let (backend, _) = backend(offline_tree());
            let (_, cx) = cx.add_window_view(move |window, cx| {
                let palette = cx.new(|cx| {
                    let mut palette = CommandPaletteView::new_unified(
                        backend,
                        Some(PaletteMode::Command),
                        window,
                        cx,
                    );
                    if !unified {
                        palette.unified = None;
                    }
                    palette
                });
                palette.read(cx).focus(cx).focus(window, cx);
                let host = cx.new(|_| OverlayHost { palette, cached });
                Root::new(host, window, cx)
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let bounds = cx
                .debug_bounds("command-palette-overlay")
                .expect("the palette overlay is drawn");
            let viewport = cx.update(|window, _| window.viewport_size());
            assert_eq!(bounds.size, viewport, "unified={unified}, cached={cached}");
            covered.push(bounds);
        }
    }
    assert_eq!(covered[0], covered[1]);
    assert_eq!(covered[2], covered[3]);
}

#[test]
fn unicode_scalar_cursor_conversion_is_boundary_safe() {
    assert_eq!(byte_index_for_char("aα界", 0), Some(0));
    assert_eq!(byte_index_for_char("aα界", 2), Some(3));
    assert_eq!(byte_index_for_char("aα界", 3), Some(6));
    assert_eq!(byte_index_for_char("aα界", 4), None);
    let value = "a🦀日本";
    assert_eq!(byte_index_for_char(value, 2), Some(5));
    assert_eq!(byte_index_for_char(value, 4), Some(value.len()));
    assert_eq!(byte_index_for_char(value, 5), None);
}

fn vim_palette(
    cx: &mut TestAppContext,
    vim: bool,
    mode: Option<PaletteMode>,
) -> (
    Entity<CommandPaletteView>,
    Calls,
    Rc<std::cell::Cell<bool>>,
    &mut VisualTestContext,
) {
    cx.update(crate::init);
    let calls = Calls::default();
    let toggle = Rc::new(std::cell::Cell::new(vim));
    let backend: Rc<dyn PaletteBackend> = Rc::new(FakeBackend {
        calls: Rc::clone(&calls),
        snapshot: Arc::default(),
        tree: Rc::new(RefCell::new(online_tree())),
        availability: PaneKindAvailability {
            browser: true,
            editor: true,
        },
        levels: None,
        host: None,
        active: PaneId(0),
        sink: Sink::default(),
        files: None,
        vim: Rc::clone(&toggle),
    });
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, mode, window, cx)
    });
    cx.run_until_parked();
    (palette, calls, toggle, cx)
}

#[derive(Debug, PartialEq)]
struct Seen {
    value: String,
    chips: Vec<String>,
    command: Option<&'static str>,
    selected: Option<usize>,
    finished: bool,
    calls: usize,
    clipboard: Option<String>,
}

fn seen(palette: &Entity<CommandPaletteView>, calls: &Calls, cx: &mut VisualTestContext) -> Seen {
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    palette.read_with(cx, |palette, cx| {
        let unified = palette.unified.as_ref().expect("a unified palette");
        Seen {
            value: palette.input.read(cx).value().to_string(),
            chips: unified
                .chips
                .iter()
                .map(|chip| chip.label.clone())
                .collect(),
            command: unified.command.map(|command| command.name),
            selected: palette.selected,
            finished: palette.is_finished(),
            calls: calls.borrow().len(),
            clipboard,
        }
    })
}

fn vim_mode(
    palette: &Entity<CommandPaletteView>,
    cx: &mut VisualTestContext,
) -> Option<PaletteVimMode> {
    palette.read_with(cx, |palette, _| palette.vim_mode())
}

fn field_normal(palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext) -> bool {
    palette.read_with(cx, |palette, cx| palette.input.read(cx).is_normal_mode())
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

#[ala::test]
fn vim_mode_opens_in_insert_and_escape_steps_to_normal_then_closes(cx: &mut TestAppContext) {
    let (palette, _, _, cx) = vim_palette(cx, true, None);
    assert_eq!(vim_mode(&palette, cx), Some(PaletteVimMode::Insert));
    assert!(!field_normal(&palette, cx));

    cx.simulate_input("dev");
    press(cx, "escape");
    assert_eq!(vim_mode(&palette, cx), Some(PaletteVimMode::Normal));
    assert!(field_normal(&palette, cx));
    assert!(!palette.read_with(cx, |palette, _| palette.is_finished()));

    press(cx, "z");
    cx.simulate_input("jk");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, cx| palette.input.read(cx).value().to_string()),
        "dev",
        "normal mode types nothing, by key or through the input method"
    );

    press(cx, "escape");
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
}

#[ala::test]
fn without_vim_mode_escape_closes_at_once(cx: &mut TestAppContext) {
    let (palette, _, _, cx) = vim_palette(cx, false, None);
    assert_eq!(vim_mode(&palette, cx), None);
    press(cx, "escape");
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
}

#[ala::test]
fn q_and_ctrl_bracket_close_from_normal(cx: &mut TestAppContext) {
    for keys in ["escape q", "ctrl-[ ctrl-["] {
        let (palette, _, _, cx) = vim_palette(cx, true, None);
        press(cx, keys);
        assert!(
            palette.read_with(cx, |palette, _| palette.is_finished()),
            "{keys}"
        );
    }
}

#[ala::test]
fn each_vim_key_does_what_its_twin_does(cx: &mut TestAppContext) {
    for (setup, vim_key, twin) in [
        (":", "j", "down"),
        (":", "j j k", "down down up"),
        (":", "k", "up"),
        ("dev", "x", "backspace"),
        (":", "x", "backspace"),
        ("", "x", "backspace"),
        ("", "l", "tab"),
        ("", "right", "tab"),
        (":join", "l", "tab"),
        ("", "h", "shift-tab"),
        ("", "y y", "alt-enter"),
        ("editor", "y y", "alt-enter"),
    ] {
        cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
        let (vim, vim_calls, _, cx) = vim_palette(cx, true, None);
        cx.simulate_input(setup);
        press(cx, "escape");
        press(cx, vim_key);
        let by_vim = seen(&vim, &vim_calls, cx);

        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
        let (plain, plain_calls, _, cx) = vim_palette(cx, false, None);
        cx.simulate_input(setup);
        cx.run_until_parked();
        let before = seen(&plain, &plain_calls, cx);
        press(cx, twin);
        let by_twin = seen(&plain, &plain_calls, cx);

        assert_eq!(by_vim, by_twin, "{vim_key} after {setup:?}");
        assert_ne!(
            by_twin, before,
            "{twin} after {setup:?} changed nothing to compare"
        );
    }
}

#[ala::test]
fn gg_and_shift_g_jump_to_the_first_and_last_row(cx: &mut TestAppContext) {
    let (palette, _, _, cx) = vim_palette(cx, true, Some(PaletteMode::Command));
    let (first, last) = palette.read_with(cx, |palette, _| {
        let rows = &palette.unified.as_ref().unwrap().rows;
        (
            rows.iter().position(|row| row.action.is_some()),
            rows.iter().rposition(|row| row.action.is_some()),
        )
    });
    press(cx, "escape shift-g");
    assert_eq!(palette.read_with(cx, |palette, _| palette.selected), last);
    press(cx, "g");
    assert_eq!(palette.read_with(cx, |palette, _| palette.selected), last);
    press(cx, "g");
    assert_eq!(palette.read_with(cx, |palette, _| palette.selected), first);
}

#[ala::test]
fn i_a_and_slash_go_back_to_insert(cx: &mut TestAppContext) {
    let (palette, _, _, cx) = vim_palette(cx, true, Some(PaletteMode::Command));
    let field = |palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext| {
        palette.read_with(cx, |palette, cx| palette.input.read(cx).value().to_string())
    };
    cx.simulate_input("join");
    press(cx, "escape i");
    assert_eq!(vim_mode(&palette, cx), Some(PaletteVimMode::Insert));
    assert!(!field_normal(&palette, cx));
    cx.simulate_input("-");
    press(cx, "escape a");
    cx.simulate_input("p");
    cx.run_until_parked();
    assert_eq!(field(&palette, cx), ":join-p");

    press(cx, "escape /");
    assert_eq!(vim_mode(&palette, cx), Some(PaletteVimMode::Insert));
    assert_eq!(
        field(&palette, cx),
        ":",
        "/ clears the text after the action sigil"
    );

    press(cx, "backspace");
    cx.simulate_input("edi");
    press(cx, "escape /");
    let chips = palette.read_with(cx, |palette, _| {
        palette.unified.as_ref().unwrap().chips.len()
    });
    assert_eq!(field(&palette, cx), "", "/ clears the search");
    assert!(chips >= 1, "/ keeps the chips");
}

#[ala::test]
fn a_colon_in_normal_mode_types_itself_in_insert(cx: &mut TestAppContext) {
    let (vim, vim_calls, _, cx) = vim_palette(cx, true, None);
    press(cx, "escape");
    press(cx, ":");
    assert_eq!(vim_mode(&vim, cx), Some(PaletteVimMode::Insert));
    let by_vim = seen(&vim, &vim_calls, cx);

    let (plain, plain_calls, _, cx) = vim_palette(cx, false, None);
    cx.simulate_input(":");
    cx.run_until_parked();
    assert_eq!(by_vim, seen(&plain, &plain_calls, cx));
}

#[ala::test]
fn turning_vim_mode_off_gives_the_field_back(cx: &mut TestAppContext) {
    let (palette, _, toggle, cx) = vim_palette(cx, true, None);
    press(cx, "escape");
    assert!(field_normal(&palette, cx));
    toggle.set(false);
    cx.update(|window, cx| palette.update(cx, |palette, cx| palette.refresh(window, cx)));
    cx.run_until_parked();
    assert_eq!(vim_mode(&palette, cx), None);
    assert!(!field_normal(&palette, cx));
    cx.simulate_input("dev");
    cx.run_until_parked();
    assert_eq!(
        palette.read_with(cx, |palette, cx| palette.input.read(cx).value().to_string()),
        "dev"
    );
}

#[ala::test]
fn a_pane_lists_its_actions_and_runs_one(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab tab tab");
    cx.run_until_parked();
    let header = palette.read_with(cx, |palette, _| {
        palette.unified.as_ref().unwrap().rows[0]
            .row
            .label
            .to_string()
    });
    assert_eq!(
        chip_labels(&palette, cx),
        ["local", "dev", "1 editor", "0 nvim"]
    );
    assert_eq!(header, "Actions on 0 nvim");
    let usage = palette.read_with(cx, |palette, _| palette.row_usage());
    assert_eq!(usage.as_deref(), Some("runs  capture-pane -p -t dev:1.0"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(palette.read_with(cx, |palette, _| palette.is_finished()));
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("capture-pane", ["-p", "-t", "%10"])
        )]
    );
}

fn two_windows() -> PaletteTree {
    let mut tree = online_tree();
    let session = &mut tree.hosts[0].sessions[0];
    let mut logs = session.windows[0].clone();
    logs.id = WindowId(2);
    logs.index = 2;
    logs.name = "logs".to_owned();
    logs.active = false;
    logs.active_pane = PaneId(20);
    logs.panes[0].id = PaneId(20);
    logs.panes[0].label = "tail".to_owned();
    session.windows.push(logs);
    tree
}

#[ala::test]
fn a_picked_target_stays_picked_when_a_refresh_reorders_the_rows(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let calls = Calls::default();
    let tree = Rc::new(RefCell::new(two_windows()));
    let backend: Rc<dyn PaletteBackend> = Rc::new(FakeBackend {
        calls: Rc::clone(&calls),
        snapshot: Arc::default(),
        tree: Rc::clone(&tree),
        availability: PaneKindAvailability::default(),
        levels: None,
        host: None,
        active: PaneId(0),
        sink: Sink::default(),
        files: None,
        vim: Rc::default(),
    });
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.simulate_input("kill window");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter down");
    cx.run_until_parked();
    let picked = |cx: &mut VisualTestContext| {
        palette.read_with(cx, |palette, _| {
            let unified = palette.unified.as_ref().unwrap();
            unified.rows[palette.selected.unwrap()]
                .row
                .label
                .to_string()
        })
    };
    assert_eq!(picked(cx), "dev › 2 logs");
    {
        let mut tree = tree.borrow_mut();
        let windows = &mut tree.hosts[0].sessions[0].windows;
        windows[0].active = false;
        windows[1].active = true;
    }
    cx.update(|window, cx| palette.update(cx, |palette, cx| palette.refresh(window, cx)));
    cx.run_until_parked();
    assert_eq!(picked(cx), "dev › 2 logs");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("kill-window", ["-t", "@2"])
        )]
    );
}

#[ala::test]
fn a_colon_in_a_text_slot_is_text_not_a_tmux_command(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (backend, calls) = backend(online_tree());
    let (palette, cx) = mount(cx, move |window, cx| {
        CommandPaletteView::new_unified(backend, None, window, cx)
    });
    cx.simulate_input("rename window");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter enter");
    cx.simulate_input(":kill-server");
    cx.run_until_parked();
    let usage = palette.read_with(cx, |palette, _| palette.draft_usage());
    assert_eq!(
        usage.as_deref(),
        Some("runs  rename-window -t dev:1 :kill-server")
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        *calls.borrow(),
        [Call::Execute(
            LOCAL,
            CommandInvocation::new("rename-window", ["-t", "@1", ":kill-server"])
        )]
    );
}
