use std::cell::{Cell, RefCell};

use ala::{KeyDownEvent, TestAppContext, TestDispatcher};
use serde_json::{Value, json};
use zz_client::completion::PaneKindAvailability;
use zz_protocol::{CommandInvocation, InputMessage, InsertStyle, ShellKind};

use super::*;
use crate::command::{PaletteSettings, PaletteTarget, PaletteTree};

struct Backend {
    next: Cell<u64>,
    calls: Rc<RefCell<Vec<Value>>>,
}

impl PaletteBackend for Backend {
    fn snapshot(&self, _: &App) -> Arc<MuxSnapshot> {
        Arc::default()
    }
    fn host_snapshot<'a>(&self, _: PaletteHostId, _: &'a App) -> Option<&'a MuxSnapshot> {
        None
    }
    fn tree(&self, _: &App) -> PaletteTree {
        PaletteTree::default()
    }
    fn settings(&self, _: &App) -> PaletteSettings {
        PaletteSettings::default()
    }
    fn availability(&self, _: &App) -> PaneKindAvailability {
        PaneKindAvailability::default()
    }
    fn command_shortcut(&self, _: &str, _: &App) -> Option<SharedString> {
        None
    }
    fn mono_font(&self, _: &App) -> SharedString {
        "mono".into()
    }
    fn send_input(&self, _: InputMessage, _: &mut App) {}
    fn send_key(&self, _: PaneId, _: &KeyDownEvent, _: &mut App) {}
    fn active_pane(&self, _: &App) -> Option<PaneId> {
        None
    }
    fn execute(&self, _: PaletteHostId, _: CommandInvocation, _: &mut App) {}
    fn connect_host(&self, _: PaletteHostId, _: &mut App) {}
    fn activate(&self, _: PaletteHostId, _: PaletteTarget, _: &mut App) -> bool {
        false
    }
    fn list_path(
        &self,
        _: PaletteHostId,
        dir: &str,
        _: Option<PaneId>,
        _: &mut App,
    ) -> Result<u64, SharedString> {
        let request = self.next.get();
        self.next.set(request.wrapping_add(1));
        self.calls.borrow_mut().push(json!([0, request, dir]));
        Ok(request)
    }
    fn cancel_path(&self, _: PaletteHostId, request: u64, _: &mut App) {
        self.calls.borrow_mut().push(json!([1, request]));
    }
    fn insert_path(
        &self,
        pane: PaneId,
        _: PaletteHostId,
        root: &PathListRoot,
        entry: &PathEntry,
        _: &mut App,
    ) {
        self.calls.borrow_mut().push(json!([
            2,
            pane.0,
            root.root,
            root.display_root,
            root.cwd,
            entry.rel,
            entry.kind == PathKind::Dir,
            entry.symlink
        ]));
    }
}

fn level(backend: Rc<dyn PaletteBackend>, cx: &mut TestAppContext) -> Entity<FilesLevel> {
    cx.new(|cx| {
        FilesLevel::new(
            backend,
            FilesPlace {
                host: PaletteHostId(0),
                session: None,
                window: None,
                pane: None,
            },
            None,
            Some("/root/".to_owned()),
            cx,
        )
    })
}

fn state(
    entity: &Entity<FilesLevel>,
    cx: &TestAppContext,
    calls: &Rc<RefCell<Vec<Value>>>,
) -> Value {
    entity.read_with(cx,|level,_|json!({"generation":level.generation,"running":level.ranking.is_some(),"dirty":level.dirty,"selected":level.selected,"pending":level.pending,"wanted":level.wanted,"path":[level.path.dir,level.path.typed,level.path.filter],"listing":level.listing.as_ref().map(|listing|json!([listing.dir,listing.request,listing.done,listing.error.as_deref()])),"rows":level.rows.iter().map(|row|json!([row.entry.rel,row.name])).collect::<Vec<_>>(),"calls":calls.borrow().clone()}))
}

fn trace(seed: u64, operations: &[Value]) -> Value {
    let mut cx = TestAppContext::build(TestDispatcher::new(seed), None);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let backend: Rc<dyn PaletteBackend> = Rc::new(Backend {
        next: Cell::new(1),
        calls: Rc::clone(&calls),
    });
    let mut entity = level(Rc::clone(&backend), &mut cx);
    let mut states = vec![state(&entity, &cx, &calls)];
    for operation in operations {
        let kind = operation[0].as_u64().expect("operation kind");
        if kind == 7 {
            cx.run_until_parked();
        } else if kind == 8 {
            drop(entity);
            cx.run_until_parked();
            entity = level(Rc::clone(&backend), &mut cx);
        } else {
            entity.update(&mut cx, |level, cx| match kind {
                0 => level.set_rest(operation[1].as_str().expect("rest"), cx),
                1..=3 => {
                    let request = level
                        .listing
                        .as_ref()
                        .and_then(|listing| listing.request)
                        .unwrap_or(0);
                    let update = match kind {
                        1 => ListingUpdate::Begin(Ok(PathListRoot {
                            root: operation[1].as_str().expect("root").to_owned(),
                            display_root: "~/".to_owned(),
                            cwd: None,
                            insert: InsertStyle::Shell(ShellKind::Posix),
                        })),
                        2 => ListingUpdate::Chunk {
                            entries: operation[1]
                                .as_array()
                                .expect("entries")
                                .iter()
                                .map(|rel| PathEntry {
                                    rel: rel.as_str().expect("rel").to_owned(),
                                    kind: if rel.as_str().expect("rel").ends_with('/') {
                                        PathKind::Dir
                                    } else {
                                        PathKind::File
                                    },
                                    symlink: false,
                                })
                                .collect(),
                            done: operation[2].as_bool().expect("done"),
                            truncated: false,
                        },
                        _ => ListingUpdate::Begin(Err("missing".into())),
                    };
                    level.apply(PaletteHostId(0), request, update, cx);
                }
                4 => level.want(operation[1].as_str().expect("wanted").to_owned()),
                5 => level.navigate(operation[1].as_i64().expect("direction") as isize, cx),
                6 => {
                    if let Some(parent) = level.up() {
                        level.set_rest(&parent, cx);
                    }
                }
                _ => panic!("unknown operation"),
            });
        }
        states.push(state(&entity, &cx, &calls));
    }
    cx.run_until_parked();
    json!({"seed":seed,"operations":operations,"states":states})
}

pub fn vectors() -> Value {
    let initial = vec![
        json!([0, ""]),
        json!([1, "/root"]),
        json!([
            2,
            ["a/", "b/", "a/main.rs", "b/main.rs", "alpha", "beta"],
            false
        ]),
        json!([7]),
    ];
    let batches = vec![
        vec![json!([0, "a"]), json!([0, "b"]), json!([7])],
        vec![
            json!([0, "a/"]),
            json!([2, ["new", "other/"], false]),
            json!([0, "b/"]),
            json!([2, ["target", "more"], true]),
            json!([7]),
        ],
        vec![
            json!([0, "a"]),
            json!([4, "/root/alpha"]),
            json!([0, "b"]),
            json!([7]),
        ],
        vec![json!([5, -1]), json!([0, "a"]), json!([5, 1]), json!([7])],
        vec![
            json!([0, "a/"]),
            json!([1, "/root/a"]),
            json!([2, ["main.rs", "child/"], true]),
            json!([7]),
            json!([6]),
            json!([2, ["a/", "b/"], false]),
            json!([7]),
        ],
        vec![
            json!([0, "a/"]),
            json!([8]),
            json!([0, ""]),
            json!([2, ["fresh"], true]),
            json!([7]),
        ],
        vec![json!([0, "missing/"]), json!([3]), json!([7])],
    ];
    let mut traces = Vec::new();
    for seed in 0..32 {
        for batch in &batches {
            let mut operations = initial.clone();
            operations.extend_from_slice(batch);
            traces.push(trace(seed, &operations));
        }
    }
    json!({"traces":traces,"scope":"actual FilesLevel async rank/refresh/land methods with deterministic TestDispatcher"})
}

#[path = "adapter_fixture.rs"]
pub mod adapter_fixture;
