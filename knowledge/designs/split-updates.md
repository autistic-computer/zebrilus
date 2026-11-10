---
type: Design Plan
title: Split updates . the client and the daemon update apart
description: Proposed plan to let a client update without restarting the daemon and ending its sessions. PROTOCOL_VERSION becomes a wire epoch that only bumps for changes an existing peer cannot decode or would get wrong, additive changes travel as messages gated by the capabilities both hellos already carry, a build revision on each side sets floors for behavior, and restarting the daemon onto a newer build becomes its own action that states how many sessions it ends. Step 1, the setup epoch (v122), step 2, the capability rule, and step 3, the client UI, are built.
status: In progress
tags:
- protocol
- versioning
- update
- daemon
- design-plan
timestamp: 2026-11-10T00:00:00-03:00
---

# Overview

> **Status: proposed 2026-11-10.** The counts below were read from the protocol version history at
> `PROTOCOL_VERSION = 121`.
>
> **2026-11-10: step 1 is built.** Wire v122 is the setup epoch. Both sides send their `REVISION` in
> the handshake and hold a floor for the other's; every revision and floor is 1, so nothing is
> refused yet. A client below the daemon's floor gets `ServerError::ClientTooOld`, and a daemon below
> the client's floor fails the attach with `DaemonError::DaemonTooOld`, which clients show as a
> plain attach failure until step 3 gives it its own screen. `PaneKindSnapshot::Other` exists and
> every client draws it as a placeholder, though the daemon never sends it. Nothing is gated by
> capability yet (step 2), so a wire change still bumps the version and still restarts the daemon.
>
> **2026-11-10: step 2 is built.** `ProtocolMessage::required_capability` and
> `EventPayload::required_capability` name each variant's capability in exhaustive matches, and
> every v122 variant answers `None`. `CLIENT_CAPABILITY_NAMES` (empty) sits beside
> `CAPABILITY_NAMES`; the Rust, browser and HTML clients append it to their hello, and each daemon
> `OutboundMailbox` keeps the declared names and drops a gated message for a client without one.
> `zz-daemon-client`'s `ProtocolSender` refuses a gated message the daemon did not list with
> `DaemonError::DaemonLacksCapability`, and the browser client drops one by `Welcome::has_capability`.
> The command client's `ServerHello.capabilities` is built from `CAPABILITY_NAMES`, same names. The
> rule and the steps for adding a message are in [wire protocol](/protocol/wire-protocol.md#wire-epochs)
> and `CLAUDE.md`. A message added from now on stays inside the epoch; the client UI for a daemon
> that is behind is step 3.
>
> **2026-11-10: step 3 is built.** A local daemon below `MIN_DAEMON_REVISION` gets the same
> "Restart the zz daemon?" alert, the same `auto-restart-stale-daemon` and the same raw terminal
> y/N prompt and `--restart-daemon` as an epoch mismatch, with its own sentence (`StaleDaemonInfo`
> is `Protocol` or `Revision`). A client below the daemon's floor is never offered a restart: the
> desktop says this zz is too old and shows Update when the update check knows a newer release,
> and the raw terminal and CLI print the message alone. While attached, `MuxClient::daemon_behind`
> compares the daemon's revision with `REVISION`; a daemon that is behind shows "Restart Daemon to
> Update…" in the tray and a Restart daemon row on the About page, and the prompt names how many
> sessions end. That restart sends `kill-server`, waits for the socket to stop answering, then
> starts a daemon the normal way, and falls back to `terminate_incompatible_daemon` after 5 s. A
> host below the floor is `HostState::DaemonTooOld` and offers Update; a host that is only behind
> offers Update in its menu, and the dialog counts its sessions; a host whose daemon refuses this
> zz is `HostState::ClientTooOld` and offers nothing. The release workflow puts
> `<!-- zz-wire: epoch=N min-daemon-revision=M -->` at the top of each release's notes, and the
> update toast and About line say whether the update keeps the sessions running. Every revision is
> still 1, so none of this shows in a normal run.
>
> **2026-11-10: step 4 is built.** On macOS the daemon starts each pane through a helper that runs
> its own binary with `--zz-pty-exec`. That helper now runs from the pinned copy the daemon already
> keeps under `$TMPDIR/zz-tmux-shim-<pid>-<seq>/` (`pin_pty_exec_helper` in
> `crates/zz-terminal/src/session/unix_pty.rs`, set from `Shared::install_tmux_shim`), so an old
> daemon keeps starting panes with its own build after an update replaces the app bundle. If the
> copy disappears, the helper tries the bundle path and then forks, as before. Linux and Windows
> are unchanged. All four steps are built; no message is gated yet and every revision is 1.

The daemon holds every session, so restarting it ends them. Today a client update almost always
forces that restart, because the client and the daemon must be built from the same protocol version.
This plan keeps the daemon running across most client updates and makes "restart the daemon onto the
new build" a separate choice.

Not in this plan: keeping sessions alive across a daemon restart. A daemon restart still ends them.

# Today

- Every frame carries `PROTOCOL_VERSION`, and `crates/zz-protocol/src/framing.rs` rejects any other
  value with `ProtocolError::VersionMismatch`. The daemon answers with
  `ServerError::ProtocolMismatch`. The desktop then asks "Restart the zz daemon?"
  (`prompt_stale_daemon` in `crates/zz/src/workspace/view.rs`) and the raw terminal asks on stderr;
  both stop the daemon with `terminate_incompatible_daemon`
  (`crates/zz-daemon-client/src/lifecycle.rs`), a signal, because a command cannot cross a
  mismatch. The web, iOS and Android clients show a status line with no action. See
  [wire protocol](/protocol/wire-protocol.md) and
  [session persistence](/concepts/session-persistence.md).
- The version moves about once per release: 107 at v0.15.1, 112 at v0.22.0, 121 now.
- Control payloads are postcard, which has no field tags and no struct boundaries. Terminal payloads
  are zz's own packing (`crates/zz-protocol/src/terminal_codec.rs`).
- The daemon encodes a tree delta once per group of clients with the same scope and hands them the
  same bytes (`shared_tree` and the `shared_delta` it feeds in
  `crates/zz-daemon/src/daemon/ctrl.rs`). Mux options are diffed and encoded per client.
- Both hellos already carry capabilities, in two unrelated sets of names.
  - Daemon to client: `Welcome.caps` is a 64-bit mask over `CAPABILITY_NAMES`
    (`crates/zz-protocol/src/control.rs`, 33 names), and the daemon sets every bit. Rust clients
    read them as strings on `ServerHello.capabilities`, which `materialize_welcome` builds; only
    the web client calls `Welcome::has_capability`. A command client gets a second, hand-written
    list from `Shared::register` (built from `CAPABILITY_NAMES` since step 2).
  - Client to daemon: `ClientHello.capabilities` carries `client-*` names. The daemon turns them
    into fields on its `Client` record in two places (`handle_connection_message` and
    `Session::register`) and keeps no list (since step 2, each `OutboundMailbox` keeps the names
    from `CLIENT_CAPABILITY_NAMES`).
- On a macOS bundle install the daemon runs from `cli` inside the app bundle
  ([process model](/architecture/process-model.md)). `scripts/install-macos.sh` replaces the bundle
  and leaves the running daemon on the previous build.
- The desktop client checks GitHub releases for a newer version (`fetch_latest` in
  `crates/zz-config/src/update.rs`, behind the `update` feature). A release says nothing about its
  protocol version.
- For an ssh host, the probe (`remote_probe_socket` in `crates/zz-daemon-client/src/endpoint.rs`)
  demands an equal protocol version from the installed CLI (`zz protocol-version`), which can
  differ from the daemon that is running; the handshake catches that case. `remote_install_script`
  never runs on connect. It runs when the person accepts "Update zz on {host}?", when the binary is
  missing, or from `zz fleet install`, and it reinstalls unless both the version string and the
  protocol version already match.
- What the daemon launches later mostly stays on its own build: `pinned_executable`
  (`crates/zz-daemon/src/daemon.rs`) pins the `zz` and `tmux` that panes and status jobs run. The
  macOS pty helper is the exception: `pty_exec_helper`
  (`crates/zz-terminal/src/session/unix_pty.rs`) runs whatever binary now sits at the daemon's
  path, and falls back to fork if that fails.

# Decisions

## 1. `PROTOCOL_VERSION` is a wire epoch

The field, the envelope and the equality check stay as they are. What changes is when it bumps: only
for a change that an existing peer cannot decode or would get wrong.

| Change | Epoch |
| --- | --- |
| Remove or reshape a shipped message, variant or field | bumps |
| Change what a shipped message means | bumps |
| Add a variant to a nested enum (`InputMessage`, `TreeOp`, `BrowserCommand`) | bumps, unless it travels in a new top-level variant |
| Add a `ProtocolMessage` or `EventPayload` variant | stays (decision 2) |
| Add data to a struct every client receives | stays (decisions 2 and 3) |
| Fix or change daemon behavior with no wire change | stays (decision 4) |

A mismatch behaves as it does now: the client offers to restart the daemon.

## 2. Additive changes are gated by capability

A message or variant added inside an epoch gets a capability name. The sender checks that the peer
has it before sending.

- Client to daemon: a new `ProtocolMessage` variant, with its name appended to `CAPABILITY_NAMES`.
  The client hides or disables the feature when the daemon lacks the name, and the send path
  refuses the message.
- Daemon to client: a new `EventPayload` variant, with its name in the client's own table, which
  the client sends in `ClientHello.capabilities`. The daemon keeps each client's names and drops a
  gated event for a client without its name. A new top-level message the daemon sends that is not
  an event works the same way.

Each enum says which capability a variant needs in one exhaustive match, so adding a variant does
not compile until that question is answered. Only `ProtocolMessage` and `EventPayload` have one;
`ProtocolMessage::Event` answers with its payload's, and a variant of a nested enum has no gate.

Appending a variant is safe with postcard as long as an older peer never receives it, which is what
the check guarantees.

Shapes inside state that every client receives (snapshots, tree deltas, the option set) are frozen
for the epoch. Snapshots and tree deltas are encoded once and shared, and the option set is one
type for every client. New data for them travels as its own
gated event, keyed by pane or window, and the client merges it. An epoch bump folds the gated data
into the struct and retires the capability.

`Welcome.caps` is a `u64` with a bit per index, and the daemon fills it as `(1 << len) - 1`, so
`CAPABILITY_NAMES` holds at most 63 names; a bit's meaning is its index, so the table is append-only
inside an epoch. The client's table rides `ClientHello.capabilities`, which decodes at most 64
entries and already carries up to 29 of the client's own entries, so it has room for 35. Const
assertions in `control.rs` hold both limits. Each epoch bump folds every capability into the
baseline and starts the table over.

## 3. Open slots for things every client must see

A client cannot decode a postcard variant it was not built with, so a fallback cannot be added to
clients that already shipped. The slot has to be in the frozen shape before it is needed.

`PaneKindSnapshot` gains `Other { kind: String }`. A pane kind added inside an epoch appears in
snapshots as `Other`, and its descriptor travels in a gated event. A client without the capability
draws a placeholder pane that names the kind and says a newer client shows it.

`ChooseTreePaneKind` needs no slot: choose-tree rows carry it as an `Option`, and a kind it does not
name is `None` with a detail line, as `View` already is.

## 4. Build revisions and floors

`zz-protocol` gains `REVISION: u32`, bumped by hand when behavior a peer depends on changes,
including a fix the other side wants to rely on. `ClientHello` and `Welcome` each carry it, and so
does `ServerHello`, the answer to a bare `ClientHello`.

Only a connection that opens with a hello carries a revision. A command connection that opens with
`Exec` (every one-shot `zz <command>`, local or `--host`) carries none either way: `ExecRequest` has
only `protocol_version` and `ExecExit` nothing. The floors do not apply to it, and the epoch is its
only gate.

- The client has `MIN_DAEMON_REVISION`. Below it, the client does not attach and shows the daemon
  restart prompt.
- The daemon has `MIN_CLIENT_REVISION`. Below it, the daemon answers with an error that tells the
  client to update.

Per change, pick one:

- **Gate** when an older peer makes the feature absent.
- **Raise the floor** when an older peer makes a screen wrong. If supporting the older peer needs a
  second code path for the same screen, raise the floor instead of writing it.

Delete a gate once the floor or the epoch passes it.

## 5. Restarting the daemon is its own action

- Inside an epoch and above the floors, a newer client attaches to the running daemon and says the
  daemon is behind.
- While attached, the restart action states what it ends: "Restart the daemon to update it. This
  ends N sessions." The count is the snapshot's, and the restart is `kill-server` followed by a
  normal start.
- Across an epoch mismatch or below a floor the client is not attached, so it has no count. Across
  an epoch it cannot send a command either; below a floor an `Exec` command still works, since it
  carries no revision, but the prompt treats both cases the same. It says the restart ends every
  running session and stops the daemon with `terminate_incompatible_daemon`, as it does now.
- A client update that raises the floor or the epoch says so before it installs, so staying on the
  older client to keep the sessions is a real choice. For that, each release states its epoch and
  its `MIN_DAEMON_REVISION` on one line of the release body, which the update check already
  downloads.
- For ssh hosts, a daemon below the floor gets the same "Update zz on {host}?" offer an epoch
  mismatch gets. A host that is only behind attaches, and the update stays an action in its menu.
- The daemon keeps running the build it was started from. The macOS pty helper gets pinned the way
  `pinned_executable` pins the `zz` that panes run, so a replaced bundle cannot hand an old daemon a
  new helper.

# What the last nine bumps would have been

| Version | Change | Under this plan |
| --- | --- | --- |
| v113 | removes the Agent pane kind | epoch bump |
| v114 | appends `EventPayload::Providers` | gated event |
| v115 | appends `ProtocolMessage::AgentSteer` | gated request |
| v116 | appends `pane_base_index` to `WindowSnapshot` and `TreeOp::WindowFlags` | gated event; a client without it numbers panes from 0, as it did before |
| v117 | appends `MuxOptionKey::CommandAlias` | gated event until the next epoch |
| v118 | brings back the Agent pane kind, appends `PaneSummary.flip_armed`, makes `PathListRequest.pane` optional | epoch bump for the reshape; the pane kind alone would be `Other` plus a gated descriptor |
| v119 | appends `Welcome.platform` and `ServerHello.platform` | gated event after the welcome |
| v120 | appends `PaneSummary.shell` | gated event |
| v121 | appends `PaneKindSnapshot::View` | `Other` plus a gated descriptor |

Two of nine force a restart instead of nine of nine. The cost moves to v116, v117, v119 and v120:
each was one appended field and becomes an event plus a merge on the client until an epoch folds it
in.

# Rejected

- **A revision range agreed in the handshake, with fields encoded per revision.** Serde gives a
  field no way to learn the agreed revision, postcard cannot tell a missing field inside a `Vec`
  element from the next element, the terminal lane does not go through serde, and shared frames
  would need one encode per revision in use with the revision in every cache key. It also needs a
  frozen bootstrap frame, since `Hello` embeds attach data and a command connection can open with
  `Exec`.
- **Pairing each client version with one daemon version.** That is today's rule with more
  bookkeeping.

# Build order

1. **Setup epoch (v122).** `REVISION` and the two floors in `zz-protocol`, the revision in
   `ClientHello` and `Welcome`, the too-old error, `PaneKindSnapshot::Other` and its placeholder in
   every client. Done when a client below `MIN_CLIENT_REVISION` is refused with the update message
   and a pane of kind `Other` draws the placeholder.
2. **The capability rule.** One table of names per direction, the exhaustive match on
   `ProtocolMessage` and `EventPayload` that names each variant's capability, the daemon's
   per-client drop of gated events, the client's refusal to send a gated request, and one source
   for the command client's capability list. The rule goes in the wire protocol doc and in
   `CLAUDE.md`. No message is gated yet, so the first gated message brings the test: the exact
   frames a client writes to a daemon that lacks the capability.
3. **Client UI.** The floor errors on the prompts that exist (desktop dialog, raw terminal prompt,
   CLI hints, thin client status line, the ssh host update offer). The "daemon is behind" state in
   the tray, About and the host menu, with the session count and a `kill-server` restart. The
   release body carries the epoch and the floor, and the update prompt reads them.
4. **Delivery.** Pin the macOS pty helper to the daemon's own build.

Step 2 rewrote the epoch paragraph in [wire protocol](/protocol/wire-protocol.md) and the Recover
row in [session persistence](/concepts/session-persistence.md).

# Open questions

- One gated event per added field, or one generic side event that carries named extras per pane and
  window? The second makes an added field one line again, at the price of untyped values.
- Is `REVISION` worth a hand-bumped counter, or would the release version do? Builds from main
  between releases share a version, which is why this plan picks the counter.
- Does the terminal lane ever need an additive change inside an epoch? Its packing has no capability
  story here; any change to it bumps the epoch.
- Floors do not cover one-shot commands, which open with `Exec` and carry no revision. If that is
  ever needed, a gated request that reports the daemon's revision does it without an epoch bump.

# Citations

- `crates/zz-protocol/src/message.rs` `PROTOCOL_VERSION`, `ClientHello`
- `crates/zz-protocol/src/framing.rs` `ProtocolError::VersionMismatch`
- `crates/zz-protocol/src/control.rs` `Welcome`, `CAPABILITY_NAMES`, `Welcome::has_capability`
- `crates/zz-protocol/src/snapshot.rs` `PaneKindSnapshot`
- `crates/zz-daemon/src/daemon/ctrl.rs` `shared_tree`
- `crates/zz-daemon/src/daemon.rs` `validate_hello`, `client_features_fact`
- `crates/zz-daemon-client/src/lifecycle.rs` `terminate_incompatible_daemon`
- `crates/zz-daemon-client/src/endpoint.rs` `remote_install_script`
- `crates/zz-config/src/update.rs` `fetch_latest`
- `scripts/install-macos.sh`
- [wire protocol](/protocol/wire-protocol.md), version history v113 to v121
