use super::*;

fn effect(value: Option<LevelEffect>, inserts: bool) -> Value {
    match value {
        None => json!([0, ""]),
        Some(LevelEffect::Stay) => json!([1, ""]),
        Some(LevelEffect::SetRest(text)) => json!([2, text]),
        Some(LevelEffect::Close) => json!([if inserts { 3 } else { 4 }, ""]),
        Some(LevelEffect::Copied(text)) => json!([5, text]),
        Some(_) => panic!("unexpected files effect"),
    }
}

pub fn vectors() -> Value {
    let mut app = TestAppContext::build(TestDispatcher::new(0), None);
    let cx = app.add_empty_window();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let backend: Rc<dyn PaletteBackend> = Rc::new(Backend {
        next: Cell::new(1),
        calls: Rc::clone(&calls),
    });
    let entity = level(Rc::clone(&backend), &mut cx.cx);
    let mut action = FilesAction::new(backend);
    action.level = Some((entity.clone(), FilesPlace::host(PaletteHostId(0))));
    let paths = [
        ("/", "", "", "/"),
        ("/root/", "", "", "/root/"),
        ("/root/sub/", "sub/", "", "/root/"),
        ("~/sub/", "~/sub/", "", "~/"),
        ("../", "../", "", "../"),
        ("/root/", "", "part", "/root/"),
    ];
    let row_sets = [
        Vec::new(),
        vec![("alpha", "alpha", false, false)],
        vec![("dir/", "dir", true, false)],
        vec![
            ("sub/λ file", "λ file", false, true),
            ("sub/next/", "next", true, false),
        ],
    ];
    let keys = [
        ActionKey::Enter,
        ActionKey::Tab,
        ActionKey::ShiftTab,
        ActionKey::Left,
        ActionKey::Right,
        ActionKey::Up,
        ActionKey::Down,
        ActionKey::First,
        ActionKey::Last,
        ActionKey::Backspace,
        ActionKey::Copy,
    ];
    let mut cases = Vec::new();
    for (dir, typed, filter, listing_dir) in paths {
        for rows in &row_sets {
            for selected in [0, 1, 8] {
                for root in [None, Some(("/root", "~/")), Some(("/λ root/", "/shown/"))] {
                    for here in [false, true] {
                        for insert in [false, true] {
                            for (key_index, key) in keys.iter().copied().enumerate() {
                                calls.borrow_mut().clear();
                                cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                                entity.update(cx, |level, _| {
                                    "remote λ".clone_into(&mut level.host_name);
                                    level.here = here;
                                    level.path = FilesPath {
                                        dir: dir.to_owned(),
                                        typed: typed.to_owned(),
                                        filter: filter.to_owned(),
                                    };
                                    level.insert_pane = insert.then_some(PaneId(9));
                                    let mut listing =
                                        Listing::new(&level.host_name, listing_dir.to_owned());
                                    listing.root = root.map(|(root, display)| PathListRoot {
                                        root: root.to_owned(),
                                        display_root: display.to_owned(),
                                        cwd: Some("/root".to_owned()),
                                        insert: InsertStyle::Shell(ShellKind::Posix),
                                    });
                                    listing.done = true;
                                    if !rows.is_empty() {
                                        listing.chunks.push(
                                            rows.iter()
                                                .map(|(rel, _, directory, symlink)| PathEntry {
                                                    rel: (*rel).to_owned(),
                                                    kind: if *directory {
                                                        PathKind::Dir
                                                    } else {
                                                        PathKind::File
                                                    },
                                                    symlink: *symlink,
                                                })
                                                .collect::<Vec<_>>()
                                                .into(),
                                        );
                                    }
                                    level.listing = Some(listing);
                                    level.rows = rows
                                        .iter()
                                        .map(|(rel, name, directory, symlink)| FileRow {
                                            entry: PathEntry {
                                                rel: (*rel).to_owned(),
                                                kind: if *directory {
                                                    PathKind::Dir
                                                } else {
                                                    PathKind::File
                                                },
                                                symlink: *symlink,
                                            },
                                            name: (*name).to_owned(),
                                            matches: Vec::new(),
                                            mark: None,
                                        })
                                        .collect::<Vec<_>>()
                                        .into();
                                    level.selected = selected;
                                    level.pending = None;
                                    level.wanted = None;
                                    level.ranking = None;
                                    level.dirty = false;
                                });
                                let before = entity.read_with(cx, |level, cx| json!({"open":level.open(),"absolute":level.absolute_dir(),"folder":level.picked_folder(),"copy":level.copy_text(),"empty":level.empty_message(),"inserts":level.inserts(cx),"hints":level.hints(cx).iter().map(|hint|[hint.key,hint.label]).collect::<Vec<_>>()}));
                                let result = cx.update(|window, cx| action.key(key, window, cx));
                                let after = entity.read_with(cx, |level, _| json!({"selected":level.selected,"pending":level.pending}));
                                let clipboard = cx
                                    .read_from_clipboard()
                                    .and_then(|item| item.text())
                                    .unwrap_or_default();
                                cases.push(json!({"input":{"path":[dir,typed,filter,listing_dir],"rows":rows,"selected":selected,"root":root,"here":here,"insert":insert,"key":key_index},"before":before,"effect":effect(result,insert),"after":after,"clipboard":clipboard,"calls":calls.borrow().clone()}));
                            }
                        }
                    }
                }
            }
        }
    }
    json!({"cases":cases,"source":"actual FilesLevel methods and FilesAction::key through isolated TestAppContext Window"})
}

pub fn ui_keys() -> Value {
    let parts = [
        "",
        "a",
        "A",
        "λ",
        "İ",
        "ctrl",
        "CTRL",
        "alt",
        "shift",
        "fn",
        "secondary",
        "cmd",
        "super",
        "win",
        "control",
        "enter",
        "tab",
        "-",
        "a->A",
        "A->Z",
        ">",
        "x-y",
        "x--",
        "x-",
        "x->λ-ctrl",
    ];
    let mut inputs = parts
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    for first in parts {
        for second in parts {
            inputs.push(format!("{first}-{second}"));
        }
    }
    for first in ["ctrl", "alt", "shift", "fn", "secondary", "cmd"] {
        for second in ["ctrl", "alt", "shift", "fn", "secondary", "cmd"] {
            for third in parts {
                inputs.push(format!("{first}-{second}-{third}"));
            }
        }
    }
    let cases = inputs
        .into_iter()
        .map(|input| {
            let value = ala::Keystroke::parse(&input).ok().map(|key| {
                let modifiers = u8::from(key.modifiers.shift)
                    | (u8::from(key.modifiers.control) << 1)
                    | (u8::from(key.modifiers.alt) << 2)
                    | (u8::from(key.modifiers.platform) << 3)
                    | (u8::from(key.modifiers.function) << 4);
                json!([key.key, modifiers, key.key_char])
            });
            json!([input, value])
        })
        .collect::<Vec<_>>();
    json!({"cases": cases, "source": "actual Keystroke::parse on the native host"})
}
