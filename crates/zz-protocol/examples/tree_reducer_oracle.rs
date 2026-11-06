use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use zz_protocol::{
    ClientView, Event, EventPayload, MuxSnapshot, PaneId, PaneSnapshot, ProtocolMessage, SessionId,
    SessionSnapshot, TreeDelta, TreeDeltaError, TreeOp, WindowId, WindowSnapshot,
    decode_protocol_frame,
};

#[derive(Serialize)]
struct Payload {
    group: &'static str,
    bytes: Vec<u8>,
    parts: Vec<Vec<u8>>,
}

#[derive(Default)]
struct Arena {
    entries: Vec<Payload>,
    known: BTreeMap<(&'static str, Vec<u8>), u32>,
}

impl Arena {
    fn intern(&mut self, group: &'static str, parts: Vec<Vec<u8>>) -> u32 {
        let bytes: Vec<_> = parts.iter().flatten().copied().collect();
        let key = (group, bytes.clone());
        if let Some(id) = self.known.get(&key) {
            return *id;
        }
        let id = u32::try_from(self.entries.len()).expect("fixture payload arena fits u32");
        self.known.insert(key, id);
        self.entries.push(Payload {
            group,
            bytes,
            parts,
        });
        id
    }
}

macro_rules! payload {
    ($arena:expr, $group:literal, $($field:expr),+ $(,)?) => {{
        $arena.intern($group, vec![$(postcard::to_stdvec(&$field)?),+])
    }};
}

fn id(value: u64) -> Value {
    json!({"$":"Id", "high":(value >> 32) as u32, "low":value as u32})
}

fn maybe_id(value: Option<u64>) -> Value {
    value.map_or_else(
        || json!({"$":"None"}),
        |value| json!({"$":"Some", "value":id(value)}),
    )
}

fn key(value: u64, major: u32) -> Value {
    json!({"$":"SortKey", "major":major, "high":(value >> 32) as u32, "low":value as u32})
}

fn pane(arena: &mut Arena, value: &PaneSnapshot) -> Result<Value, Box<dyn Error>> {
    Ok(json!({"$":"PaneRecord", "id":id(value.id.0),
        "title":payload!(arena,"pane-title",value.title),
        "state":payload!(arena,"pane-state",value.kind,value.synchronized_input,value.bell,value.dead,value.dead_status),
        "presentation":payload!(arena,"pane-presentation",value.border_colour,value.active_border_colour,value.border_status_text,value.mode),
        "status":payload!(arena,"pane-status",value.status),
        "summary":payload!(arena,"pane-summary",value.summary)}))
}

fn window(arena: &mut Arena, value: &WindowSnapshot) -> Result<Value, Box<dyn Error>> {
    let panes = value
        .panes
        .iter()
        .map(|(map_key, value)| {
            Ok(json!({"$":"Tagged", "key":key(map_key.0,0), "value":pane(arena,value)?}))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(
        json!({"$":"Tagged", "key":key(value.id.0,value.index), "value":{
        "$":"WindowRecord", "name":payload!(arena,"window-name",value.name), "active_pane":id(value.active_pane.0),
        "layout":payload!(arena,"window-layout",value.layout,value.zoomed_pane,value.layout_dump,value.visible_layout_dump,value.pane_order,value.pane_z_order,value.floating,value.modal,value.sx,value.sy),
        "flags":payload!(arena,"window-flags",value.automatic_rename,value.activity,value.silence,value.pane_base_index),
        "presentation":payload!(arena,"window-presentation",value.status_label,value.pane_border_status,value.pane_border_lines,value.pane_border_indicators),
        "panes":panes}}),
    )
}

fn session(arena: &mut Arena, value: &SessionSnapshot) -> Result<Value, Box<dyn Error>> {
    let windows = value
        .windows
        .iter()
        .map(|value| window(arena, value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"$":"Tagged", "key":key(value.id.0,0), "value":{
        "$":"SessionRecord", "name":payload!(arena,"session-name",value.name), "active_window":id(value.active_window.0),
        "viewers":payload!(arena,"session-viewers",value.viewers), "windows":windows}}))
}

fn snapshot(arena: &mut Arena, value: &MuxSnapshot) -> Result<Value, Box<dyn Error>> {
    let sessions = value
        .sessions
        .iter()
        .map(|value| session(arena, value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"$":"TreeSnapshot", "generation":id(value.generation), "focused":maybe_id(value.focused_window.map(|value|value.0)), "sessions":sessions}),
    )
}

fn op(arena: &mut Arena, value: &TreeOp) -> Result<Value, Box<dyn Error>> {
    Ok(match value {
        TreeOp::UpsertSession(value) => {
            json!({"$":"UpsertSession","session":session(arena,value)?})
        }
        TreeOp::RemoveSession(value) => json!({"$":"RemoveSession","id":id(value.0)}),
        TreeOp::SessionName { session, name } => {
            json!({"$":"SessionName","session":id(session.0),"name":payload!(arena,"session-name",name)})
        }
        TreeOp::SessionActiveWindow { session, window } => {
            json!({"$":"SessionActiveWindow","session":id(session.0),"window":id(window.0)})
        }
        TreeOp::SessionViewers { session, viewers } => {
            json!({"$":"SessionViewers","session":id(session.0),"viewers":payload!(arena,"session-viewers",viewers)})
        }
        TreeOp::UpsertWindow {
            session,
            window: value,
        } => json!({"$":"UpsertWindow","session":id(session.0),"window":window(arena,value)?}),
        TreeOp::RemoveWindow { session, window } => {
            json!({"$":"RemoveWindow","session":id(session.0),"window":id(window.0)})
        }
        TreeOp::WindowName {
            session,
            window,
            name,
        } => {
            json!({"$":"WindowName","session":id(session.0),"window":id(window.0),"name":payload!(arena,"window-name",name)})
        }
        TreeOp::WindowActivePane {
            session,
            window,
            pane,
        } => {
            json!({"$":"WindowActivePane","session":id(session.0),"window":id(window.0),"pane":id(pane.0)})
        }
        TreeOp::WindowLayout {
            session,
            window,
            layout,
            zoomed_pane,
            layout_dump,
            visible_layout_dump,
            pane_order,
            pane_z_order,
            floating,
            modal,
            sx,
            sy,
        } => {
            json!({"$":"WindowLayout","session":id(session.0),"window":id(window.0),"layout":payload!(arena,"window-layout",layout,zoomed_pane,layout_dump,visible_layout_dump,pane_order,pane_z_order,floating,modal,sx,sy)})
        }
        TreeOp::WindowFlags {
            session,
            window,
            index,
            automatic_rename,
            activity,
            silence,
            pane_base_index,
        } => {
            json!({"$":"WindowFlags","session":id(session.0),"window":id(window.0),"index":index,"flags":payload!(arena,"window-flags",automatic_rename,activity,silence,pane_base_index)})
        }
        TreeOp::WindowPresentation {
            session,
            window,
            status_label,
            pane_border_status,
            pane_border_lines,
            pane_border_indicators,
        } => {
            json!({"$":"WindowPresentation","session":id(session.0),"window":id(window.0),"presentation":payload!(arena,"window-presentation",status_label,pane_border_status,pane_border_lines,pane_border_indicators)})
        }
        TreeOp::UpsertPane {
            session,
            window,
            pane: value,
        } => {
            json!({"$":"UpsertPane","session":id(session.0),"window":id(window.0),"pane":pane(arena,value)?})
        }
        TreeOp::RemovePane {
            session,
            window,
            pane,
        } => {
            json!({"$":"RemovePane","session":id(session.0),"window":id(window.0),"pane":id(pane.0)})
        }
        TreeOp::PaneTitle {
            session,
            window,
            pane,
            title,
        } => {
            json!({"$":"PaneTitle","session":id(session.0),"window":id(window.0),"pane":id(pane.0),"title":payload!(arena,"pane-title",title)})
        }
        TreeOp::PaneState {
            session,
            window,
            pane,
            kind,
            synchronized_input,
            bell,
            dead,
            dead_status,
        } => {
            json!({"$":"PaneState","session":id(session.0),"window":id(window.0),"pane":id(pane.0),"state":payload!(arena,"pane-state",kind,synchronized_input,bell,dead,dead_status)})
        }
        TreeOp::PanePresentation {
            session,
            window,
            pane,
            border_colour,
            active_border_colour,
            border_status_text,
            mode,
        } => {
            json!({"$":"PanePresentation","session":id(session.0),"window":id(window.0),"pane":id(pane.0),"presentation":payload!(arena,"pane-presentation",border_colour,active_border_colour,border_status_text,mode)})
        }
        TreeOp::PaneStatus {
            session,
            window,
            pane,
            status,
        } => {
            json!({"$":"PaneStatus","session":id(session.0),"window":id(window.0),"pane":id(pane.0),"status":payload!(arena,"pane-status",status)})
        }
        TreeOp::PaneSummary {
            session,
            window,
            pane,
            summary,
        } => {
            json!({"$":"PaneSummary","session":id(session.0),"window":id(window.0),"pane":id(pane.0),"summary":payload!(arena,"pane-summary",summary)})
        }
    })
}

fn error(value: TreeDeltaError) -> Value {
    let value = match value {
        TreeDeltaError::BaseMismatch { expected, received } => {
            json!({"$":"BaseMismatch","expected":id(expected),"received":id(received)})
        }
        TreeDeltaError::MissingSession(value) => json!({"$":"MissingSession","id":id(value.0)}),
        TreeDeltaError::MissingWindow(value) => json!({"$":"MissingWindow","id":id(value.0)}),
        TreeDeltaError::MissingPane(value) => json!({"$":"MissingPane","id":id(value.0)}),
    };
    json!({"$":"Some","value":value})
}

enum Action {
    Delta(TreeDelta),
    Overlay(Vec<TreeOp>),
    View(ClientView),
}

fn record(
    vectors: &mut Vec<Value>,
    name: &str,
    before: &MuxSnapshot,
    action: Action,
) -> Result<(), Box<dyn Error>> {
    let mut arena = Arena::default();
    let input = snapshot(&mut arena, before)?;
    let mut actual = before.clone();
    let (kind, update, update_wire, result) = match action {
        Action::Delta(delta) => {
            let ops = delta
                .ops
                .iter()
                .map(|value| op(&mut arena, value))
                .collect::<Result<Vec<_>, _>>()?;
            (
                "delta",
                json!({"$":"Delta","base":id(delta.base),"version":id(delta.version),"ops":ops}),
                postcard::to_stdvec(&delta)?,
                delta.apply(&mut actual),
            )
        }
        Action::Overlay(ops) => {
            let normalized = ops
                .iter()
                .map(|value| op(&mut arena, value))
                .collect::<Result<Vec<_>, _>>()?;
            (
                "overlay",
                json!(normalized),
                postcard::to_stdvec(&ops)?,
                TreeDelta::apply_overlay(&mut actual, &ops),
            )
        }
        Action::View(view) => {
            let ops = view
                .overlay
                .iter()
                .map(|value| op(&mut arena, value))
                .collect::<Result<Vec<_>, _>>()?;
            (
                "view",
                json!({"$":"ClientView","focused":maybe_id(view.focused_window.map(|value|value.0)),"ops":ops}),
                postcard::to_stdvec(&view)?,
                view.apply(&mut actual),
            )
        }
    };
    if result.is_err() {
        assert_eq!(
            postcard::to_stdvec(&actual)?,
            postcard::to_stdvec(before)?,
            "atomic source failure {name}"
        );
    }
    let input_payloads = arena.entries.len();
    let after = snapshot(&mut arena, &actual)?;
    assert_eq!(
        arena.entries.len(),
        input_payloads,
        "source generated a new opaque group in {name}"
    );
    let outcome = json!({"$":"Outcome","error":result.err().map_or_else(||json!({"$":"None"}),error),"snapshot":after});
    vectors.push(json!({"name":name,"kind":kind,"before":input,"update":update,"update_wire":update_wire,"outcome":outcome,"arena":arena.entries,"before_wire":postcard::to_stdvec(before)?,"after_wire":postcard::to_stdvec(&actual)?}));
    Ok(())
}

fn event(value: &Value, field: &str) -> Result<EventPayload, Box<dyn Error>> {
    let bytes: Vec<u8> = serde_json::from_value(value[field].clone())?;
    match decode_protocol_frame(&bytes)? {
        ProtocolMessage::Event(Event { payload, .. }) => Ok(payload),
        _ => Err(std::io::Error::other("source fixture is not an event").into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let source_path = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("target/port/control-vectors.json"),
        PathBuf::from,
    );
    let source: Value = serde_json::from_slice(&fs::read(source_path)?)?;
    let seed = &source["reductions"][0];
    let EventPayload::Snapshot(base) = event(seed, "before_bytes")? else {
        return Err(std::io::Error::other("source has no snapshot").into());
    };
    let mut operations = Vec::new();
    for row in source["reductions"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("source has no reductions"))?
    {
        let EventPayload::TreeDelta(delta) = event(row, "delta_bytes")? else {
            return Err(std::io::Error::other("source has no delta").into());
        };
        operations.extend(delta.ops);
    }
    assert_eq!(operations.len(), 19, "source operation vocabulary");
    let mut vectors = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        record(
            &mut vectors,
            &format!("single-{index}"),
            &base,
            Action::Delta(TreeDelta {
                base: base.generation,
                version: 0,
                ops: vec![operation.clone()],
            }),
        )?;
        for (next, second) in operations.iter().enumerate() {
            record(
                &mut vectors,
                &format!("ordered-{index}-{next}"),
                &base,
                Action::Delta(TreeDelta {
                    base: base.generation,
                    version: u64::MAX,
                    ops: vec![operation.clone(), second.clone()],
                }),
            )?;
        }
    }
    let mut state = 0x9e37_79b9_u32;
    for index in 0..256 {
        let mut ops = Vec::new();
        for _ in 0..index % 17 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ops.push(operations[(state as usize) % operations.len()].clone());
        }
        record(
            &mut vectors,
            &format!("batch-{index}"),
            &base,
            Action::Delta(TreeDelta {
                base: base.generation,
                version: u64::from(state) << 32,
                ops,
            }),
        )?;
    }
    for received in [0, u64::MAX, base.generation] {
        record(
            &mut vectors,
            "base-alignment",
            &base,
            Action::Delta(TreeDelta {
                base: received,
                version: 1,
                ops: operations.clone(),
            }),
        )?;
    }
    let sid = base.sessions[0].id;
    let wid = base.sessions[0].windows[0].id;
    let pid = *base.sessions[0].windows[0]
        .panes
        .keys()
        .next()
        .expect("source has pane map entries");
    let mut duplicates = base.clone();
    let mut duplicate = duplicates.sessions[0].clone();
    "same session id, different body".clone_into(&mut duplicate.name);
    duplicates.sessions.push(duplicate);
    let mut duplicate_window = duplicates.sessions[0].windows[0].clone();
    duplicate_window.index = 9;
    duplicates.sessions[0].windows[0].index = 0;
    "same window id, other index".clone_into(&mut duplicate_window.name);
    duplicates.sessions[0].windows.insert(0, duplicate_window);
    for action in [
        Action::Overlay(vec![]),
        Action::Delta(TreeDelta {
            base: base.generation,
            version: 0,
            ops: vec![],
        }),
        Action::Overlay(vec![
            TreeOp::WindowFlags {
                session: sid,
                window: wid,
                index: u32::MAX,
                automatic_rename: false,
                activity: true,
                silence: false,
                pane_base_index: 9,
            },
            TreeOp::WindowName {
                session: sid,
                window: wid,
                name: "must hit same first window before final sort".to_owned(),
            },
        ]),
        Action::Overlay(vec![TreeOp::RemoveWindow {
            session: sid,
            window: wid,
        }]),
        Action::Overlay(vec![TreeOp::RemoveSession(sid)]),
        Action::Overlay(vec![TreeOp::RemoveSession(SessionId(1234))]),
    ] {
        record(
            &mut vectors,
            "duplicate-vec-and-final-sort",
            &duplicates,
            action,
        )?;
    }
    let mut mismatched = base.clone();
    mismatched.sessions[0].windows[0]
        .panes
        .get_mut(&pid)
        .expect("source pane exists")
        .id = PaneId(1234);
    let incoming = mismatched.sessions[0].windows[0].panes[&pid].clone();
    for ops in [
        vec![TreeOp::PaneTitle {
            session: sid,
            window: wid,
            pane: pid,
            title: "target is map key".to_owned(),
        }],
        vec![TreeOp::UpsertPane {
            session: sid,
            window: wid,
            pane: Box::new(incoming),
        }],
        vec![TreeOp::RemovePane {
            session: sid,
            window: wid,
            pane: PaneId(1234),
        }],
    ] {
        record(
            &mut vectors,
            "pane-map-key-differs-from-payload-id",
            &mismatched,
            Action::Overlay(ops),
        )?;
    }
    for target in [None, Some(WindowId(1234)), Some(wid)] {
        let mut view = ClientView {
            focused_window: target,
            ..ClientView::default()
        };
        record(
            &mut vectors,
            "view-empty-overlay-focus",
            &duplicates,
            Action::View(view.clone()),
        )?;
        view.overlay = vec![
            TreeOp::SessionActiveWindow {
                session: sid,
                window: WindowId(1234),
            },
            TreeOp::WindowActivePane {
                session: sid,
                window: wid,
                pane: PaneId(1234),
            },
        ];
        record(
            &mut vectors,
            "view-dangling-references-allowed",
            &base,
            Action::View(view.clone()),
        )?;
        view.overlay.push(TreeOp::PaneTitle {
            session: sid,
            window: wid,
            pane: PaneId(1234),
            title: "must roll back all edits and focus".to_owned(),
        });
        record(&mut vectors, "view-late-failure", &base, Action::View(view))?;
    }
    for row in source["rejected_reductions"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("source has no rejections"))?
    {
        if let EventPayload::TreeDelta(delta) = event(row, "delta_bytes")? {
            record(
                &mut vectors,
                "source-missing-target",
                &base,
                Action::Delta(delta),
            )?;
        }
    }
    println!(
        "{}",
        json!({"reference":"zz_protocol::TreeDelta::apply/apply_overlay + ClientView::apply","vectors":vectors})
    );
    Ok(())
}
