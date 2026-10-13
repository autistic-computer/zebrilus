---
type: Protocol
title: Mux snapshots (snapshot.rs)
description: The MuxSnapshot state tree (sessions, windows, recursive split layouts, pane descriptors, pane status and summary records, behavior flags, per-client focus, and viewer presence) that clients reconcile on attach and after a resync, plus the daemon clock that dates pane changes.
resource: crates/zz-protocol/src/snapshot.rs
tags: [protocol, snapshot, layout, state, presence, sidebar, pane-summary]
timestamp: 2026-10-12T21:30:00-03:00
---

# Overview

A **`MuxSnapshot`** is the complete, renderer-neutral picture of the daemon's session/window/pane/split
tree at a point in time. Defined in `crates/zz-protocol/src/snapshot.rs`, it is the model an interactive client
materializes from its initial state, tree deltas and per-client view.
The snapshot carries *structure and metadata* (layout, titles, active/zoomed panes, pane kind). It
deliberately does **not** carry terminal cell contents, which stream separately over the
[packed terminal lanes](/protocol/terminal-lanes.md). A monotonically increasing `generation` counter
lets clients detect staleness. The [GPUI app](/crates/zz.md) reconciles this tree into live pane
entities, reusing them by stable [`PaneId`](/protocol/ids.md).

# When it is produced and consumed

The daemon sends one raw scoped snapshot in the initial attach or repair `Batch`, then sends
`TreeDelta { base, version, ops }` on tree changes. The client retains the raw tree and applies a
separate `ClientView` overlay to expose the same `MuxSnapshot` renderer model. Tree subscriptions
select no tree, the attached session, or all sessions. The daemon sends no event for an empty diff.

A delta applies only when its base equals the retained tree generation. On a mismatch the client
queues `TreeSync` and keeps its current tree until the daemon supplies a fresh scoped snapshot.
Event sequence gaps alone do not trigger repair: the daemon can supersede stale terminal frames.

`ClientView` supplies the client's focus, presence and presentation fields, plus layout and
attachment generations. A reattach increments the attachment generation even for the same
session, clearing overlays and terminal bases once. Ordinary label or presence updates keep the
attachment generation. Geometry reports name the layout generation the client rendered, so the
daemon can reject a late report after zoom, unzoom or another layout change. Generation
validation and geometry application share one model lock. A new layout generation requires
a fresh tagged report even if the measured grid and cell pixels are unchanged; same-connection
attachment follows that rule too. Ordinary presentation changes retain the geometry tag.

The legacy `Attached` and `Snapshot` forms remain usable by internal fixtures. Normal control
plane v2 clients receive grouped state and deltas. Produced by [zz-daemon](/crates/zz-daemon.md)
from [the mux state machine](/crates/zz-mux.md), and consumed by the desktop, TUI, web and iOS clients.

# Schema . the snapshot tree

`MuxSnapshot` nests four levels; all types serialize through `serde` and travel via `postcard` on the
Control lane. Most types derive `Deserialize`; recursive `LayoutNode` uses a wire-compatible custom
deserializer so hostile nesting is rejected before it can exhaust the receiving thread's stack.

| Type | Fields |
|------|--------|
| `MuxSnapshot` | `generation: u64`, `sessions: Vec<SessionSnapshot>`, `focused_window: Option<WindowId>` |
| `SessionSnapshot` | `id: SessionId`, `name: String`, `active_window: WindowId`, `windows: Vec<WindowSnapshot>`, `viewers: Vec<SessionViewer>` |
| `SessionViewer` | `name: String`, `window: WindowId`, `is_self: bool` |
| `WindowSnapshot` | `id: WindowId`, `index: u32`, `name: String`, `automatic_rename: bool`, `active_pane: PaneId`, `zoomed_pane: Option<PaneId>`, `layout: LayoutNode`, `panes: BTreeMap<PaneId, PaneSnapshot>`, `layout_dump: String`, `visible_layout_dump: String`, `status_label: String`, `activity: bool`, `silence: bool`, `pane_border_status: PaneBorderStatus`, `pane_border_lines: PaneBorderLines`, `pane_border_indicators: PaneBorderIndicators`, `pane_order: Vec<PaneId>`, `pane_z_order: Vec<PaneId>`, `floating: Vec<FloatingPaneSnapshot>`, `modal: Option<ModalPaneSnapshot>`, `sx: u16`, `sy: u16` |
| `FloatingPaneSnapshot` | `pane: PaneId`, `xoff: i32`, `yoff: i32`, `sx: u16`, `sy: u16`, `visible: bool`, `border_lines: PaneBorderLines`, `border_status: PaneBorderStatus` |
| `ModalPaneSnapshot` | `pane: PaneId`, `capture_keys: bool`, `close_on_click: bool`, `close_on_cancel: bool` |
| `PaneSnapshot` | `id: PaneId`, `title: String`, `kind: PaneKindSnapshot`, `synchronized_input: bool`, `bell: bool`, `dead: bool`, `dead_status: Option<u32>`, `border_colour: Option<TmuxColour>`, `active_border_colour: Option<TmuxColour>`, `border_status_text: String`, `mode: Option<PaneMode>`, `status: Option<PaneStatus>`, `summary: PaneSummary` |
| `PaneStatus` | `state: PaneStatusState`, `kind: Option<PaneStatusKind>`, `progress: Option<u8>`, `app: String`, `title: String`, `message: String` |
| `PaneStatusState` | `Idle` \| `Working` \| `Done` \| `Blocked` \| `Error` |
| `PaneStatusKind` | `Permission` \| `Question` \| `Auth` |
| `PaneSummary` | `changed_at: u64`, `unseen: bool`, `command: Option<ShellCommand>`, `name: String`, `cwd: String`, `repo: String`, `branch: String` |
| `ShellCommand` | `Running` \| `Finished(u8)` |
| `PaneMode` | `Clock { .. }` \| `Switch { .. }` \| `Customize { .. }` \| `Panes { .. }` |
| `PaneKindSnapshot` | `Picker` \| `Terminal` \| `Browser(BrowserDescriptor)` \| `Agent(AgentDescriptor)` \| `Editor(EditorDescriptor)` |
| `BrowserDescriptor` | `tabs: Vec<String>`, `active_tab: usize`, `profile: String` |
| `AgentDescriptor` | `provider: AgentProvider`, `cwd: Option<PathBuf>`, `session_id: Option<String>` |
| `EditorDescriptor` | `path: Option<String>`, `cwd: String` |

Every field after `activity` on `WindowSnapshot`, and after `active_border_colour` on
`PaneSnapshot`, carries `#[serde(default)]`. That only helps JSON readers: postcard is positional,
so an older decoder still fails on a newer frame, and the protocol version gates the mix.

`generation` is the version stamp for the whole tree; `WindowSnapshot.index` is the tmux-style window
number; `automatic_rename` tells presentation surfaces whether the active pane may supply the tab
label; `zoomed_pane` records a temporarily maximized pane; `synchronized_input` marks panes receiving
mirrored keystrokes; `bell` is latched until the pane is read after a BEL. A retained terminal exit
sets `dead`; `dead_status` contains only a normal exit code and is empty for signals and worker
failures. Respawn clears both without changing the pane id or layout leaf. `border_colour` and
`active_border_colour` (appended at v71) are the pane's `pane-border-style` /
`pane-active-border-style` colour overrides; `None` means no tmux override and `TmuxColour::Rgb` values
above `0xFFFFFF` are rejected on decode. Since Wave C run 3 (2026-08-22) the daemon resolves the
explicit values pane → window → global during personalized snapshot stamping (formats expanded in the
pane's context) and publishes the style's fg colour. The raw TUI consumes both fields. The GPUI client
ignores them because pane borders and separators belong to its local chrome theme. With the options
unset both fields stay `None`. `BrowserDescriptor.tabs`
is the strip in order (never empty in the type's contract; `url()` returns the active tab or
`about:blank`). `WindowSnapshot.name` is the user's stable, explicit window name;
`PaneSnapshot.title` is live presentation metadata. Terminal OSC title changes are synchronized by
the daemon's terminal watcher; automatic Unix Bash/zsh hooks publish the working directory at a
prompt and the full command immediately before execution, while applications may override either
with later OSC 0/2 output. Browser document-title changes arrive through tmux-compatible
`select-pane -T`. Either title change advances `generation` and publishes a tree delta without
renaming the containing window.

`layout_dump` and `visible_layout_dump` carry tmux 3.8's JSON v2 layout strings (protocol v108;
before that, the checksummed v1 cell tree). A control client converts them back to v1 with
`zz_mux::legacy_layout` unless it set the `new-layouts` flag. Protocol v69
appended the daemon-expanded `status_label` for the TUI's window-status loop. GUI clients ignore
that label. Protocol v86 appended `activity`; the daemon copies its latched window-activity flag
there, and selection clears it. Native window strips can render the flag from structured state
without parsing status text. Protocol v95 then appended the window's pane border chrome after it:
`pane_border_status`, `pane_border_lines`, `pane_border_indicators`, `pane_order` (the order
`window_pane_index` counts) and `pane_z_order` (which of two panes owns a border cell they both
touch).

`silence` is the window's latched `monitor-silence` alert. `border_status_text` is the pane's
expanded `pane-border-format`, empty while `pane-border-status` is off. `mode` is the server pane
mode a client draws over the pane: `clock-mode`, `switch-mode`, `options-mode` (customize) or
`panes-mode` (`display-panes`). `floating`, `modal`, `sx` and `sy` are the window's floating panes, its modal
float and its own size ([floating panes](/designs/floating-panes.md)). `status` and `summary` are
described under [pane status, summary and time](#pane-status-summary-and-time).

`Picker` is a durable, runtime-free pane state used by the native split-window --kind picker flow. It occupies a real
layout leaf and survives GUI detach/reattach, but the daemon owns no PTY and the GUI owns no CEF
session for it. `select-pane-kind` with `terminal`, `browser`, `agent`, or `editor` materializes the
same `PaneId` and advances the snapshot generation.

`Editor(EditorDescriptor)` is the daemon-owned restore metadata for an Editor pane: the open file's
absolute `path` (`None` for a scratch pane) and the pane's `cwd`. Buffer bytes, cursor position, and
dirty state stay GUI-local. Both paths are validated on deserialization as absolute, at most
`MAX_EDITOR_PATH_BYTES` (16 KiB), and free of control characters, so a hostile descriptor cannot
become mux state.

`Agent(AgentDescriptor)` stores a bounded built-in `AgentProvider` (`Codex` or `ClaudeCode`) without
transporting provider-specific ACP payloads. The daemon captures the donor terminal's live working
directory when a picker becomes an Agent pane, then updates the descriptor with the ACP session's
absolute cwd and opaque session ID. The daemon owns the adapter process, retained runtime state,
and journal. Prompts, messages, tool calls, permissions, credentials, and provider-specific payloads
travel through the separate Agent protocol lanes rather than the mux snapshot.

## Pane status, summary and time

`PaneSnapshot.status` is one attention record per pane, whoever reported it:

- a terminal that speaks OSC 7501 gets its headline record ([program status](/terminal/program-status.md));
- an Agent pane gets the record the daemon's agent fanout computes from its phase, pending
  permission and last turn ([Native Agent pane](/concepts/agent-pane.md));
- any other terminal pane gets its `@agent_state` option mirrored into a record (`working`,
  `blocked`, `needs-approval` as Blocked/Permission, `needs-input` as Blocked/Question, `failed` or
  `error`, `done`, and `idle` or `starting`, which only replace an existing record), so the Claude
  peer sampler, the OSC 9;4 bridge and user hooks all reach the tree.

`app`, `title` and `message` are bounded on decode at `MAX_PANE_STATUS_APP_BYTES` (64),
`MAX_PANE_STATUS_TITLE_BYTES` (256) and `MAX_PANE_STATUS_MESSAGE_BYTES` (2048) bytes; a longer
string fails the decode. Producers cut text on a char boundary before it gets there.

`PaneSnapshot.summary: PaneSummary` is what a sidebar row needs beyond the status. Every field
moves on an event, never on output bytes:

| Field | Meaning |
|---|---|
| `changed_at` | Daemon wall clock, Unix seconds, of the pane's last meaningful change; 0 until one happens |
| `unseen` | A finish nobody has looked at |
| `command` | The last OSC 133 mark on a Terminal pane: `Running` after `C`, `Finished(status)` after `D`; `None` without shell integration |
| `name` | The pane-scoped `@name` user option, cut to `MAX_PANE_NAME_BYTES` (256) |
| `cwd` | Working directory for display: absolute, the daemon user's home folded to `~` (spelled as `HOME` gives it or as it resolves through symlinks), with the daemon platform's separators (`\` on Windows); empty when unknown or longer than `MAX_PANE_CWD_BYTES` (4096) |
| `repo` | The root of the git repository holding `cwd` (the folder with the `.git` directory or `gitdir:` file, so a linked worktree's own folder), written like `cwd`; empty outside a repository. Clients name a numbered session after it |
| `branch` | The branch checked out at `cwd`, the ref as written when HEAD names something outside `refs/heads/`, or a 7-character commit id when HEAD is detached; empty outside a repository; cut to `MAX_PANE_BRANCH_BYTES` (256) |

`changed_at` is stamped by a status state or kind change, a shell command start (the `C` mark's
start time) or finish (the `D` mark's end time), a bell latch going from false to true, and pane
death. Output, title writes, a status message, title or progress change within the same state and
kind, cwd, repo, branch, `@name`, browser navigation and pane creation never stamp it, and neither does a
pane's first record when it is `Idle` (an Agent pane opening or restoring).

`unseen` is raised on a status edge into `Done` from anything else, or from `Working` or `Blocked`
into `Idle` on a terminal pane, unless the pane's window is the current window of a session some client is attached
to (the same rule that keeps a watched pane's bell from latching). An edge into `Working` or
`Blocked` drops it, so a stale finish does not resurface after the next turn. Agent panes report
`Done` themselves, so their `Idle` after a cancelled turn raises nothing. It clears wherever the
bell clears: pane selection, attach, pane input, window activation, `kill-session -C` and respawn.
Because it lives in the daemon, every client on every host sees the same latch. A daemon restart
clears it and `changed_at`, as it does the bell.

`cwd` comes from the terminal's OSC 7 report, or from the live working directory of the pane's
foreground process when the shell sends no OSC 7, and from `AgentDescriptor.cwd` for Agent panes.
The daemon resolves it and the branch on a location thread of its own, never under the server lock
and never on the helper pool, so a stat that hangs on a network mount delays only other locations.
The daemon remembers the last 64 answers by source (the OSC 7 report, or the live path when there
is none). When a pane moves (its terminal is spawned or an Agent pane opens, the OSC 7 path
changes, or the live path changes on a pane without OSC 7), a remembered answer fills the summary
at once, from memory, so it rides the publish of the move itself. An answer read in the last 10
seconds is taken without a job, and every pane that took one is read again 2 seconds after the
first such take, once per source for the burst; an older answer is shown and read again at once. A
shell command finishing and a native agent status edge out of `Working`, or while the summary has
no cwd yet, always read again, so a branch switched from a pane shows up there. A read renews
every remembered answer and every pane summary with the same `cwd`, and sends summary ops only for
the panes it changes. At most one job per pane is in flight; a
trigger while one runs drops that job's result, which is neither shown nor remembered, and runs it
again with fresh inputs. The branch is read from the `.git` directory
or `gitdir:` file and `HEAD` (4 KiB at most) without running git; bare repositories and `GIT_DIR`
are not followed, and a reftable repository, whose `HEAD` is the `refs/heads/.invalid` placeholder,
shows no branch.

`TreeOp::PaneSummary { session, window, pane, summary }` (tag 18) carries a summary change on its
own, so a stamp that moves sends one small op instead of the pane.

`Welcome.clock` and `ServerHello.clock` carry the daemon's wall clock, Unix seconds, when the
handshake was sent. `ClientCore::server_clock()` keeps it and a reconnect replaces it.
`zz_client::window_summary::DaemonClock` turns it into an offset, so ages are computed against
daemon time and saturate at "now" when either clock steps backwards. The same module holds the
renderer-free sidebar view model (`summarize_window`, `summarize_session`, `summarize_host`); no
client draws it yet ([flat sidebar data](/designs/flat-sidebar-data.md)).

## Per-client stamping and presence

A session holds a set of attached clients, so the tree alone cannot say which window a given device
is looking at. The daemon sends the recipient-specific fields in a `ClientView` overlay: `MuxSnapshot.focused_window` is that client's own focused window, and every
`SessionSnapshot.viewers` entry gets `is_self` set for the receiving client. Both fields carry
`#[serde(default)]`.

`focused_window_for(session)` resolves what to render. It keeps the stamped window only while that
window still exists in the session, and otherwise falls back to `session.active_window`, which covers
both a detached snapshot and a client focus left stale by a window that closed. Removing a window
needs no snapshot repair pass.

A `SessionViewer` names one attached device: `name` is the `device_name` from its `ClientHello`
(`device-<client id>` when the hello carried none), `window` is the window that device is watching,
and `is_self` distinguishes the recipient from its peers. This is what the sidebar renders as
presence.

## Layout . the recursive `LayoutNode`

Each window's wire geometry is a binary tree that keeps stable `^split` IDs across resizes. Since
the cell-authoritative layout landed, this tree is a **derived projection**: the engine's truth is
an n-ary cell tree in `zz-mux/src/layout.rs`, and `ratio` is computed from cell extents at
snapshot time (`first / (first + rest + inner borders)`), so the wire shape and decode rules below
are unchanged:

| Variant | Fields |
|---------|--------|
| `LayoutNode::Pane(PaneId)` | a leaf pane |
| `LayoutNode::Split { id, axis, ratio, first, second }` | `id: SplitId`, `axis: Axis`, `ratio: f32`, `first`/`second: Box<LayoutNode>` |

`Axis` is `Horizontal | Vertical` (default `Vertical`, `repr(u8)`). `ratio` is the first child's share
of the split's extent as an `f32` (the live [split resize](/protocol/wire-protocol.md) input
`ResizeSplit` uses fixed-point `ratio_basis_points` over `SPLIT_RATIO_BASIS = 10_000`, applied in the
mux). `LayoutNode` provides walk helpers: `contains(pane)`, `panes(&mut Vec<PaneId>)`,
`contains_split(split)`, and `splits(&mut Vec<SplitId>)`.

Decode accepts at most 256 nested layout nodes on one path and 65,535 total nodes in one layout.
Exceeding either budget returns a normal protocol decode error. Serialization and the postcard enum
layout are unchanged, so these defensive bounds do not require a protocol-version bump.

# Examples

```rust
// A window split vertically 60/40 with two terminal panes:
let layout = LayoutNode::Split {
    id: SplitId(11),
    axis: Axis::Vertical,
    ratio: 0.6,
    first:  Box::new(LayoutNode::Pane(PaneId(3))),
    second: Box::new(LayoutNode::Pane(PaneId(4))),
};

let snapshot = MuxSnapshot {
    generation: 42,
    sessions: vec![SessionSnapshot {
        id: SessionId(0),
        name: "dev".into(),
        active_window: WindowId(7),
        windows: vec![WindowSnapshot {
            id: WindowId(7), index: 0, name: "edit".into(),
            automatic_rename: false,
            active_pane: PaneId(3), zoomed_pane: None,
            layout,
            panes: /* BTreeMap<PaneId, PaneSnapshot> */ Default::default(),
            layout_dump: "layout".into(),
            visible_layout_dump: "layout".into(),
            status_label: "0:edit".into(),
            activity: false,
            // silence, the pane border fields, pane_order, pane_z_order, floating, modal,
            // sx and sy omitted
        }],
        // stamped per recipient: this copy went to "laptop"
        viewers: vec![
            SessionViewer { name: "laptop".into(),  window: WindowId(7), is_self: true },
            SessionViewer { name: "desktop".into(), window: WindowId(7), is_self: false },
        ],
    }],
    focused_window: Some(WindowId(7)),
};

// Delivered on attach:  ProtocolMessage::Attached { session: SessionId(0), snapshot }
```

A browser pane's kind carries its descriptor:
`PaneKindSnapshot::Browser(BrowserDescriptor { tabs: vec!["https://example.com".into()], active_tab: 0, profile: "default".into() })`.
An Agent pane carries restore metadata:
`PaneKindSnapshot::Agent(AgentDescriptor { provider: AgentProvider::Codex, cwd: Some("/workspace".into()), session_id: Some("opaque-session".into()) })`.
An Editor pane carries the file it reopens:
`PaneKindSnapshot::Editor(EditorDescriptor { path: Some("/workspace/src/main.rs".into()), cwd: "/workspace".into() })`.

# Related

- Carried by [the wire protocol](/protocol/wire-protocol.md) (`Attached`, `Event`/`Snapshot`, `Resync`).
- Part of [the zz-protocol crate](/crates/zz-protocol.md); identifiers from [stable IDs](/protocol/ids.md).
- Built by [the mux state machine](/crates/zz-mux.md), served by [zz-daemon](/crates/zz-daemon.md),
  reconciled by [the GPUI app](/crates/zz.md).
- Layout details: [split-pane layout](/concepts/split-pane-layout.md); durability:
  [session persistence](/concepts/session-persistence.md).
- Cell contents stream separately via [packed terminal lanes](/protocol/terminal-lanes.md).
- Agent rendering, ACP lifecycle, and restore boundary: [Native Agent pane](/concepts/agent-pane.md).
