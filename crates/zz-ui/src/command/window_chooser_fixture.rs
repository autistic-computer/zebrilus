use std::cell::RefCell;

use ala::{TestAppContext, TestDispatcher, VisualTestContext};
use serde_json::{Value, json};

use super::*;

struct Backend {
    calls: Rc<RefCell<Vec<InputMessage>>>,
    grouped: bool,
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
        PaletteSettings {
            grouped: self.grouped,
            ..PaletteSettings::default()
        }
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
    fn send_input(&self, input: InputMessage, _: &mut App) {
        self.calls.borrow_mut().push(input);
    }
    fn send_key(&self, _: PaneId, _: &KeyDownEvent, _: &mut App) {}
    fn active_pane(&self, _: &App) -> Option<PaneId> {
        None
    }
    fn execute(&self, _: PaletteHostId, _: CommandInvocation, _: &mut App) {}
    fn connect_host(&self, _: PaletteHostId, _: &mut App) {}
    fn activate(&self, _: PaletteHostId, _: PaletteTarget, _: &mut App) -> bool {
        false
    }
}

fn target(value: ChooseTreeTarget) -> Value {
    match value {
        ChooseTreeTarget::Session(id) => json!([0, id.0]),
        ChooseTreeTarget::Window(id) => json!([1, id.0]),
        ChooseTreeTarget::Pane(id) => json!([2, id.0]),
        ChooseTreeTarget::Client(id) => json!([3, id.0]),
    }
}

fn state(
    palette: &Entity<CommandPaletteView>,
    cx: &VisualTestContext,
    calls: &Rc<RefCell<Vec<InputMessage>>>,
    events: &Rc<RefCell<Vec<u8>>>,
) -> Value {
    palette.read_with(cx, |palette, cx| {
        let input = palette.input.read(cx);
        let frames: Vec<_> = calls
            .borrow()
            .iter()
            .map(|input| {
                zz_protocol::encode_protocol_message(&zz_protocol::ProtocolMessage::Input(
                    input.clone(),
                ))
                .expect("source input encoding")
            })
            .collect();
        let rows: Vec<_> = palette
            .unified
            .as_ref()
            .map(|unified| {
                unified
                    .rows
                    .iter()
                    .filter_map(|entry| match entry.action {
                        Some(PaletteAction::ChooseWindow(index)) => Some(index),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({
            "revision": palette.revision,
            "local": palette.local,
            "window": palette.window_chooser.is_some(),
            "finishing": palette.finishing,
            "navigation": palette.navigation_engaged,
            "selected": palette.selected,
            "query": palette.last_input,
            "cursor": palette.last_cursor,
            "input": input.value().to_string(),
            "input_cursor": input.cursor(),
            "rows": rows,
            "expanded": palette.chooser_search.expanded.iter().copied().map(target).collect::<Vec<_>>(),
            "pending": palette.chooser_search.pending.map(|(value, expanded)| json!([target(value), expanded])),
            "selection": palette.chooser_search.selection.map(target),
            "activation": palette.chooser_activation.map(target),
            "frames": frames,
            "events": events.borrow().clone(),
            "scroll": palette.scroll_handle.0.borrow().deferred_scroll_to_item.map(|request| json!([request.item_index, match request.strategy { ScrollStrategy::Top => 0, ScrollStrategy::Nearest => 1, ScrollStrategy::Center => 2, ScrollStrategy::Bottom => 3 }, request.offset, request.scroll_strict])),
        })
    })
}

pub fn trace(
    states: &[ChooseTreeState],
    initial: usize,
    grouped: bool,
    operations: &[Value],
) -> Vec<Value> {
    let mut app = TestAppContext::build(TestDispatcher::new(0x3723_5671), None);
    app.update(crate::init);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let events = Rc::new(RefCell::new(Vec::new()));
    let backend: Rc<dyn PaletteBackend> = Rc::new(Backend {
        calls: Rc::clone(&calls),
        grouped,
    });
    let cx = app.add_empty_window();
    let palette = cx.update(|window, cx| {
        cx.new(|cx| {
            CommandPaletteView::new_window_chooser(backend, &states[initial], 1, window, cx)
        })
    });
    let observed = Rc::clone(&events);
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&palette, move |_, event, _| {
            observed.borrow_mut().push(match event {
                CommandPaletteEvent::ReturnToDefault => 0,
                CommandPaletteEvent::Gui(_) => 1,
            });
        })
    });
    cx.run_until_parked();
    let mut result = vec![state(&palette, cx, &calls, &events)];
    for operation in operations {
        calls.borrow_mut().clear();
        events.borrow_mut().clear();
        palette.update_in(cx, |palette, window, cx| {
            palette.scroll_handle.0.borrow_mut().deferred_scroll_to_item = None;
            match operation[0].as_u64().expect("operation kind") {
                0 => palette.synchronize_window_chooser(
                    &states[operation[1].as_u64().expect("state index") as usize],
                    operation[2].as_u64().expect("source revision"),
                    window,
                    cx,
                ),
                1 => {
                    let value = operation[1].as_str().expect("query").to_owned();
                    let selected = operation[2].as_u64().map(|cursor| cursor as usize);
                    palette.input.update(cx, |input, cx| {
                        input.set_value(value, window, cx);
                        if let Some(cursor) = selected {
                            input.set_selected_range(cursor..cursor, cx);
                        }
                    });
                }
                2 => palette.navigate(operation[1].as_i64().expect("direction") as isize, cx),
                3 => palette.move_in_tree(operation[1].as_bool().expect("expand"), cx),
                4 => {
                    palette.activate_unified(operation[1].as_bool().expect("complete"), window, cx);
                }
                5 => palette.engage_pointer_selection(
                    operation[1].as_u64().expect("row index") as usize,
                    cx,
                ),
                6 => palette.select_edge(operation[1].as_bool().expect("first"), window, cx),
                7 => palette.escape(cx),
                8 => palette.return_to_default(window, cx),
                9 => palette.leave_layer(&Backspace, window, cx),
                10 => palette.set_query(
                    operation[1]
                        .as_str()
                        .expect("programmatic query")
                        .to_owned(),
                    window,
                    cx,
                ),
                13 => {
                    let value = operation[1].as_str().expect("inserted text");
                    palette.input.update(cx, |input, cx| {
                        input.set_selected_range(0..input.value().len(), cx);
                        input.replace(value, window, cx);
                    });
                    let input = palette.input.clone();
                    palette.synchronize_local_input(&input, window, cx);
                }
                11 => {}
                12 => palette.advance_chooser_search(cx),
                _ => panic!("unknown window chooser operation"),
            }
        });
        cx.run_until_parked();
        result.push(state(&palette, cx, &calls, &events));
    }
    result
}
