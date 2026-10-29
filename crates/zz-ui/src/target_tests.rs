use std::cell::{Cell, RefCell};

use ala::{Focusable as _, TestAppContext, VisualTestContext};
use zz_protocol::{InsertStyle, ShellKind};

use super::*;
use crate::{Root, input::Input};

fn token(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    target_at(text, cursor).map(|token| (token.range, token.body))
}

#[test]
fn the_word_at_the_cursor_is_a_mention_only_after_whitespace() {
    assert_eq!(token("hi @ubu", 7), Some((3..7, "ubu".to_owned())));
    assert_eq!(token("@", 1), Some((0..1, String::new())));
    assert_eq!(
        token("@ubuntu:work rest", 5),
        Some((0..12, "ubun".to_owned()))
    );
    assert_eq!(token("mail fab@host", 13), None);
    assert_eq!(token("hi @x ", 6), None);
    assert_eq!(token("hi @x", 3), None);
    assert_eq!(
        token("say: @ubuntu:/tmp", 99),
        Some((5..17, "ubuntu:/tmp".to_owned()))
    );
    let escaped = r"see @ubuntu:~/My\ Files/no";
    assert_eq!(
        token(escaped, escaped.len()),
        Some((4..escaped.len(), r"ubuntu:~/My\ Files/no".to_owned()))
    );
}

fn pane(id: u64, title: &str, program: Option<&'static str>) -> TargetPane {
    TargetPane {
        id: PaneId(id),
        title: title.to_owned(),
        program: program.map(SharedString::from),
        icon: IconName::SquareTerminal,
    }
}

pub(crate) fn hosts() -> Vec<TargetHost> {
    vec![
        TargetHost {
            id: PaletteHostId(0),
            name: "macbook".to_owned(),
            status: PaletteStatus::Online,
            here: true,
            sessions: vec![TargetSession {
                id: SessionId(1),
                name: "main".to_owned(),
                windows: vec![TargetWindow {
                    id: WindowId(2),
                    index: 1,
                    name: "shell".to_owned(),
                    panes: vec![pane(5, "zsh", None)],
                }],
            }],
        },
        TargetHost {
            id: PaletteHostId(1),
            name: "ubuntu".to_owned(),
            status: PaletteStatus::Online,
            here: false,
            sessions: vec![
                TargetSession {
                    id: SessionId(3),
                    name: "work".to_owned(),
                    windows: vec![
                        TargetWindow {
                            id: WindowId(7),
                            index: 1,
                            name: "vim".to_owned(),
                            panes: vec![pane(11, "editor", None)],
                        },
                        TargetWindow {
                            id: WindowId(8),
                            index: 2,
                            name: "agents".to_owned(),
                            panes: vec![
                                pane(12, "fix tests", Some("Claude Code")),
                                pane(13, "", Some("Codex")),
                            ],
                        },
                    ],
                },
                TargetSession {
                    id: SessionId(4),
                    name: "my notes".to_owned(),
                    windows: Vec::new(),
                },
            ],
        },
        TargetHost {
            id: PaletteHostId(2),
            name: "xps".to_owned(),
            status: PaletteStatus::Offline,
            here: false,
            sessions: Vec::new(),
        },
    ]
}

fn query(body: &str) -> (TargetLevel, String, bool) {
    let query = parse_target(body, &hosts());
    (query.level, query.filter, query.written)
}

fn files(host: &str, dir: &str) -> TargetLevel {
    TargetLevel::Files {
        host: host.to_owned(),
        dir: dir.to_owned(),
    }
}

#[test]
fn colons_and_dots_drill_in_like_tmux_targets() {
    let sessions = TargetLevel::Sessions {
        host: "ubuntu".to_owned(),
    };
    let windows = TargetLevel::Windows {
        host: "ubuntu".to_owned(),
        session: "work".to_owned(),
    };
    let panes = TargetLevel::Panes {
        host: "ubuntu".to_owned(),
        session: "work".to_owned(),
        window: "2".to_owned(),
    };
    assert_eq!(query(""), (TargetLevel::Top, String::new(), false));
    assert_eq!(query("ubu"), (TargetLevel::Top, "ubu".to_owned(), false));
    assert_eq!(query("ubuntu:"), (sessions.clone(), String::new(), true));
    assert_eq!(query("ubuntu:wo"), (sessions, "wo".to_owned(), true));
    assert_eq!(
        query("ubuntu:work:"),
        (windows.clone(), String::new(), true)
    );
    assert_eq!(query("ubuntu:work:2"), (windows, "2".to_owned(), true));
    assert_eq!(
        query("ubuntu:work:2."),
        (panes.clone(), String::new(), true)
    );
    assert_eq!(query("ubuntu:work:2.%1"), (panes, "%1".to_owned(), true));
}

#[test]
fn without_a_host_a_target_means_the_host_here() {
    let windows = TargetLevel::Windows {
        host: "macbook".to_owned(),
        session: "main".to_owned(),
    };
    assert_eq!(query("main:"), (windows.clone(), String::new(), false));
    assert_eq!(query("main:sh"), (windows, "sh".to_owned(), false));
    assert_eq!(
        query("main:1.%"),
        (
            TargetLevel::Panes {
                host: "macbook".to_owned(),
                session: "main".to_owned(),
                window: "1".to_owned(),
            },
            "%".to_owned(),
            false
        )
    );
    assert_eq!(
        query("~/dev/"),
        (files("macbook", "~/dev/"), String::new(), false)
    );
    assert_eq!(
        query("/etc/ho"),
        (files("macbook", "/etc/"), "ho".to_owned(), false)
    );
}

#[test]
fn a_slash_or_tilde_after_the_host_lists_files() {
    assert_eq!(
        query("ubuntu:/"),
        (files("ubuntu", "/"), String::new(), true)
    );
    assert_eq!(
        query("ubuntu:/home/fab/de"),
        (files("ubuntu", "/home/fab/"), "de".to_owned(), true)
    );
    assert_eq!(
        query("ubuntu:~"),
        (files("ubuntu", "~/"), String::new(), true)
    );
    assert_eq!(
        query(r"ubuntu:~/My\ Files/no\ te"),
        (files("ubuntu", r"~/My\ Files/"), "no te".to_owned(), true)
    );
}

#[test]
fn backspace_on_a_levels_edge_goes_up_one_level() {
    assert_eq!(pop_level("ubuntu:work:").as_deref(), Some("ubuntu:"));
    assert_eq!(pop_level("ubuntu:").as_deref(), Some(""));
    assert_eq!(pop_level("main:1.").as_deref(), Some("main:"));
    assert_eq!(pop_level("~/dev/zz/").as_deref(), Some("~/dev/"));
    assert_eq!(pop_level("~/").as_deref(), Some(""));
    assert_eq!(pop_level("ubuntu:~/dev/").as_deref(), Some("ubuntu:~/"));
    assert_eq!(
        pop_level(r"~/My\ Files/x/").as_deref(),
        Some(r"~/My\ Files/")
    );
    assert_eq!(pop_level("ubuntu:wo"), None);
    assert_eq!(pop_level(""), None);
}

fn items(body: &str) -> Vec<TargetItem> {
    target_items(&parse_target(body, &hosts()), &hosts())
}

fn tokens(body: &str) -> Vec<String> {
    items(body).into_iter().map(|item| item.token).collect()
}

fn typed(body: &str) -> Vec<String> {
    items(body).into_iter().map(|item| item.typed).collect()
}

#[test]
fn each_level_lists_its_targets_with_their_host() {
    assert_eq!(
        tokens(""),
        ["macbook:main", "ubuntu", "xps", "macbook:~"],
        "the sessions here, the other hosts, then the files here"
    );
    assert_eq!(typed(""), ["main", "ubuntu", "xps", "~"]);
    assert_eq!(tokens("ubuntu:"), ["ubuntu:work", "ubuntu:$4", "ubuntu:~"]);
    assert_eq!(tokens("ubuntu:not"), ["ubuntu:$4"]);
    assert_eq!(tokens("ubuntu:work:"), ["ubuntu:work:1", "ubuntu:work:2"]);
    assert_eq!(tokens("ubuntu:$3:age"), ["ubuntu:work:2"]);
    assert_eq!(
        tokens("ubuntu:work:2."),
        ["ubuntu:work:2.%12", "ubuntu:work:2.%13"]
    );
    assert_eq!(tokens("ubuntu:work:agents.").len(), 2);
    assert_eq!(tokens("ubuntu:%13"), ["ubuntu:%13"]);
    assert_eq!(tokens("%5"), ["macbook:%5"]);
    assert_eq!(tokens("main:"), ["macbook:main:1"]);
    assert_eq!(typed("main:"), ["main:1"]);
    assert_eq!(typed("macbook:main:"), ["macbook:main:1"]);
    assert!(tokens("nowhere:").is_empty());
    assert!(tokens("ubuntu:gone:").is_empty());

    let top = items("");
    assert_eq!(top[2].status, Some(PaletteStatus::Offline));
    assert_eq!(top[2].detail.as_ref(), "offline");
    assert_eq!(top[1].detail.as_ref(), "2 sessions");
    assert_eq!(top[0].target, Some(PaletteTarget::Session(SessionId(1))));
    let panes = items("ubuntu:work:2.");
    assert_eq!(panes[0].label.as_ref(), "%12");
    assert_eq!(panes[0].detail.as_ref(), "Claude Code · fix tests");
    assert_eq!(panes[0].kind, TargetKind::Pane);
    assert_eq!(panes[0].target, Some(PaletteTarget::Pane(PaneId(12))));
    assert_eq!(panes[0].host_id, PaletteHostId(1));
}

#[test]
fn typing_before_any_colon_searches_the_whole_fleet() {
    assert_eq!(tokens("ubu")[0], "ubuntu");
    assert_eq!(tokens("work")[0], "ubuntu:work");
    let agents = items("agents");
    assert_eq!(agents[0].token, "ubuntu:work:2");
    assert_eq!(agents[0].label.as_ref(), "ubuntu:work:2");
    assert!(
        items("claude")
            .iter()
            .any(|item| item.token == "ubuntu:work:2.%12"),
        "a pane is found by its program"
    );
    assert_eq!(typed("mai")[0], "main", "the host here is left out");
}

fn entry(rel: &str, kind: PathKind) -> PathEntry {
    PathEntry {
        rel: rel.to_owned(),
        kind,
        symlink: false,
    }
}

fn home() -> Arc<[PathEntry]> {
    Arc::from([
        entry("notes.md", PathKind::File),
        entry("dev/", PathKind::Dir),
        entry("My Files/", PathKind::Dir),
        entry("dev/zz/", PathKind::Dir),
        entry("dev/zz/README.md", PathKind::File),
    ])
}

fn file_query(body: &str) -> FileQuery {
    let query = parse_target(body, &hosts());
    let TargetLevel::Files { host, dir } = &query.level else {
        panic!("{body} is not a files target");
    };
    let mut marks = GitMarks::default();
    marks.extend(vec![("notes.md".to_owned(), GitMark::Modified)]);
    FileQuery {
        chunks: vec![home()],
        marks: Arc::new(marks),
        base: unescape(dir)
            .strip_prefix("~/")
            .unwrap_or_default()
            .to_owned(),
        filter: query.filter.clone(),
        host: hosts()
            .into_iter()
            .find(|candidate| candidate.name == *host)
            .unwrap(),
        written: query.written,
        dir: dir.clone(),
    }
}

fn file_tokens(body: &str) -> Vec<String> {
    file_items(&file_query(body))
        .into_iter()
        .map(|item| item.token)
        .collect()
}

#[test]
fn a_folder_lists_its_entries_and_typing_searches_under_it() {
    assert_eq!(
        file_tokens("ubuntu:~/"),
        [r"ubuntu:~/My\ Files", "ubuntu:~/dev", "ubuntu:~/notes.md"]
    );
    assert_eq!(file_tokens("ubuntu:~/no"), ["ubuntu:~/notes.md"]);
    assert_eq!(
        file_tokens("ubuntu:~/readme"),
        ["ubuntu:~/dev/zz/README.md"],
        "a match deep under the folder"
    );
    assert_eq!(file_tokens("ubuntu:~/dev/"), ["ubuntu:~/dev/zz"]);
    let here = file_items(&file_query("~/"));
    assert_eq!(here[1].typed, "~/dev");
    assert_eq!(here[1].token, "macbook:~/dev");
    assert_eq!(here[2].right().as_ref(), "M file");
    assert_eq!(here[1].right().as_ref(), "dir ›");
    assert_eq!(here[2].entry, Some(entry("notes.md", PathKind::File)));
}

#[test]
fn tab_opens_the_level_under_a_row_and_enter_finishes_it() {
    let tab =
        |body: &str| -> Vec<String> { items(body).iter().map(TargetItem::completion).collect() };
    let enter =
        |body: &str| -> Vec<String> { items(body).iter().map(TargetItem::insertion).collect() };
    assert_eq!(tab("ubu")[0], "ubuntu:");
    assert_eq!(tab("ubuntu:"), ["ubuntu:work:", "ubuntu:$4:", "ubuntu:~/"]);
    assert_eq!(tab("ubuntu:work:"), ["ubuntu:work:1.", "ubuntu:work:2."]);
    assert_eq!(
        tab("ubuntu:work:2."),
        ["ubuntu:work:2.%12", "ubuntu:work:2.%13"]
    );
    assert_eq!(tab(""), ["main:", "ubuntu:", "xps:", "~/"]);
    assert_eq!(
        parse_target(&tab("ubuntu:work:")[1], &hosts()).level,
        TargetLevel::Panes {
            host: "ubuntu".to_owned(),
            session: "work".to_owned(),
            window: "2".to_owned(),
        }
    );
    let notes = file_items(&file_query("~/no"));
    assert_eq!(notes[0].completion(), "~/notes.md");
    assert_eq!(notes[0].insertion(), "@macbook:~/notes.md ");

    assert_eq!(enter("ubu")[0], "@ubuntu ");
    assert_eq!(enter("ubuntu:wo"), ["@ubuntu:work "]);
    assert_eq!(enter("ubuntu:work:2"), ["@ubuntu:work:2 "]);
    assert_eq!(enter("main:"), ["@macbook:main:1 "]);
}

#[test]
fn rows_with_a_level_under_them_say_so() {
    let right = |body: &str| -> Vec<String> {
        items(body)
            .iter()
            .map(|item| item.right().to_string())
            .collect()
    };
    assert_eq!(right("ubuntu:"), ["session ›", "session ›", "dir ›"]);
    assert_eq!(right("ubuntu:work:"), ["window ›", "window ›"]);
    assert_eq!(right("ubuntu:work:2."), ["pane", "pane"]);
    assert_eq!(HINTS.map(|hint| hint.label), ["open", "insert", "close"]);
}

#[test]
fn the_header_names_the_level() {
    let crumb = |body: &str, root| breadcrumb(&parse_target(body, &hosts()), root);
    assert_eq!(crumb("ub", None), "@");
    assert_eq!(crumb("ubuntu:work:", None), "@ ubuntu › work");
    assert_eq!(crumb("main:1.", None), "@ macbook › main › 1");
    assert_eq!(crumb("ubuntu:~/", None), "@ ubuntu › ~/");
    let root = PathListRoot {
        root: "/home/fab".to_owned(),
        display_root: "~/".to_owned(),
        cwd: None,
        insert: InsertStyle::Shell(ShellKind::Posix),
    };
    assert_eq!(crumb("ubuntu:~/", Some(&root)), "@ ubuntu › /home/fab");
}

#[derive(Debug, PartialEq)]
enum Call {
    List(String, String, u64),
    Cancel(String, u64),
}

struct FakeBackend {
    calls: Rc<RefCell<Vec<Call>>>,
    next: Cell<u64>,
}

impl TargetBackend for FakeBackend {
    fn hosts(&self, _: &App) -> Vec<TargetHost> {
        hosts()
    }

    fn list(
        &self,
        host: &str,
        dir: &str,
        _: Option<PaneId>,
        _: &mut App,
    ) -> Result<u64, SharedString> {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.calls
            .borrow_mut()
            .push(Call::List(host.to_owned(), dir.to_owned(), id));
        Ok(id)
    }

    fn cancel(&self, host: &str, request_id: u64, _: &mut App) {
        self.calls
            .borrow_mut()
            .push(Call::Cancel(host.to_owned(), request_id));
    }
}

struct Composer {
    input: Entity<InputState>,
    picker: Entity<TargetPicker>,
    keys: Rc<RefCell<Vec<String>>>,
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let keys = Rc::clone(&self.keys);
        mention_keys(div().id("composer"), &self.picker)
            .capture_key_down(move |event, _, _| {
                keys.borrow_mut().push(event.keystroke.key.clone());
            })
            .child(Input::new(&self.input))
            .child(self.picker.clone())
    }
}

#[ala::test]
fn the_picker_follows_the_composer_and_owns_its_keys(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let backend: Rc<dyn TargetBackend> = Rc::new(FakeBackend {
        calls: Rc::clone(&calls),
        next: Cell::new(0),
    });
    let keys = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&keys);
    let slot = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx));
        let picker = cx.new(|cx| TargetPicker::for_composer(backend, input.clone(), cx));
        input.read(cx).focus_handle(cx).focus(window, cx);
        captured.replace(Some((input.clone(), picker.clone())));
        let composer = cx.new(|_| Composer {
            input,
            picker,
            keys: seen,
        });
        Root::new(composer, window, cx)
    });
    let (input, picker) = slot.borrow_mut().take().expect("composer captured");
    let value =
        |cx: &mut VisualTestContext| input.read_with(cx, |input, _| input.value().to_string());
    let open = |cx: &mut VisualTestContext| picker.read_with(cx, |picker, _| picker.is_open());
    let first = |cx: &mut VisualTestContext| {
        picker.read_with(cx, |picker, _| {
            picker.selected_item().map(|item| item.token.clone())
        })
    };

    cx.simulate_input("ask @ubu");
    assert!(open(cx));
    assert_eq!(first(cx).as_deref(), Some("ubuntu"));
    cx.simulate_keystrokes("tab");
    assert_eq!(value(cx), "ask @ubuntu:");
    assert!(open(cx));
    assert_eq!(first(cx).as_deref(), Some("ubuntu:work"));
    cx.simulate_keystrokes("tab");
    assert_eq!(value(cx), "ask @ubuntu:work:");
    cx.simulate_keystrokes("down tab");
    assert_eq!(value(cx), "ask @ubuntu:work:2.");
    assert_eq!(first(cx).as_deref(), Some("ubuntu:work:2.%12"));
    cx.simulate_keystrokes("backspace backspace");
    cx.simulate_input("1.");
    assert_eq!(first(cx).as_deref(), Some("ubuntu:work:1.%11"));
    cx.simulate_keystrokes("backspace backspace");
    cx.simulate_input("2");
    cx.simulate_keystrokes("enter");
    assert_eq!(value(cx), "ask @ubuntu:work:2 ");
    assert!(!open(cx));

    cx.simulate_input("@main:");
    cx.simulate_keystrokes("enter");
    let said = "ask @ubuntu:work:2 @macbook:main:1 ";
    assert_eq!(value(cx), said, "a target here goes out with its host");

    cx.simulate_input("and @ubuntu:");
    cx.simulate_keystrokes("up");
    assert_eq!(first(cx).as_deref(), Some("ubuntu:~"));
    cx.simulate_keystrokes("tab");
    let said = format!("{said}and @ubuntu:");
    assert_eq!(value(cx), format!("{said}~/"));
    assert_eq!(
        calls.borrow().as_slice(),
        [Call::List("ubuntu".to_owned(), "~/".to_owned(), 1)]
    );
    picker.update(cx, |picker, cx| {
        picker.apply_chunk("ubuntu", 1, &home(), true, false, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("down tab");
    assert_eq!(value(cx), format!("{said}~/dev/"));
    cx.run_until_parked();
    assert_eq!(
        first(cx).as_deref(),
        Some("ubuntu:~/dev/zz"),
        "a finished walk lists a folder under it without asking again"
    );
    assert_eq!(calls.borrow().len(), 1);

    cx.simulate_keystrokes("escape");
    assert!(!open(cx));
    assert_eq!(value(cx), format!("{said}~/dev/"));
    cx.simulate_input("z");
    assert!(!open(cx));
    cx.simulate_input(" @");
    assert!(open(cx));
    assert_eq!(first(cx).as_deref(), Some("macbook:main"));
    assert!(
        keys.borrow()
            .iter()
            .all(|key| !["tab", "enter", "escape", "up", "down"].contains(&key.as_str())),
        "{:?}",
        keys.borrow()
    );
    cx.simulate_keystrokes("escape escape");
    assert_eq!(keys.borrow().last().map(String::as_str), Some("escape"));
}
