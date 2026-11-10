---
type: Design Plan
title: Split updates . the client and the daemon update apart
description: Proposed plan to let a client update without restarting the daemon and ending its sessions. PROTOCOL_VERSION becomes a wire epoch that only bumps for changes an existing peer cannot decode or would get wrong, additive changes travel as messages gated by the capabilities both hellos already carry, a build revision on each side sets floors for behavior, and restarting the daemon onto a newer build becomes its own action that states how many sessions it ends. Nothing is built.
status: Proposed
tags:
- protocol
- versioning
- update
- daemon
- design-plan
timestamp: 2026-11-10T00:00:00-03:00
---

# Overview

> **Status: proposed 2026-11-10. Nothing is built.** The counts below were read from the protocol
> version history at `PROTOCOL_VERSION = 121`.

The daemon holds every session, so restarting it ends them. Today a client update almost always
forces that restart, because the client and the daemon must be built from the same protocol version.
This plan keeps the daemon running across most client updates and makes "restart the daemon onto the
new build" a separate choice.

Not in this plan: keeping sessions alive across a daemon restart. A daemon restart still ends them.

# Today

- Every frame carries `PROTOCOL_VERSION`, and `crates/zz-protocol/src/framing.rs` rejects any other
  value with `ProtocolError::VersionMismatch`. The daemon answers with
  `ServerError::ProtocolMismatch` and the client offers `terminate_incompatible_daemon`
  (`crates/zz-daemon-client/src/lifecycle.rs`). See [wire protocol](/protocol/wire-protocol.md) and
  [session persistence](/concepts/session-persistence.md).
- The version moves about once per release: 107 at v0.15.1, 112 at v0.22.0, 121 now.
- Control payloads are postcard, which has no field tags and no struct boundaries. Terminal payloads
  are zz's own packing (`crates/zz-protocol/src/terminal_codec.rs`).
- The daemon encodes a tree delta once and hands the same bytes to every client (`shared_tree` and
  the `shared_delta` it feeds in `crates/zz-daemon/src/daemon/ctrl.rs`).
- Both hellos already carry capabilities. `ClientHello.capabilities` is a list of names the daemon
  reads at registration (`client_features_fact`). `Welcome.caps` is a 64-bit mask over
  `CAPABILITY_NAMES` (`crates/zz-protocol/src/control.rs`), read with `Welcome::has_capability`.
- On a macOS bundle install the daemon runs from `cli` inside the app bundle
  ([process model](/architecture/process-model.md)). `scripts/install-macos.sh` replaces the bundle
  and leaves the running daemon on the previous build.
- The desktop client checks GitHub releases for a newer version (`fetch_latest` in
  `crates/zz-config/src/update.rs`, behind the `update` feature). A release says nothing about its
  protocol version.
- For an ssh host, `remote_install_script` (`crates/zz-daemon-client/src/endpoint.rs`) installs the
  client's own version there and the probe demands an equal protocol version. The probe asks the
  installed CLI (`zz protocol-version`), which can differ from the daemon that is running.

# Decisions

## 1. `PROTOCOL_VERSION` is a wire epoch

The field, the envelope and the equality check stay as they are. What changes is when it bumps: only
for a change that an existing peer cannot decode or would get wrong.

| Change | Epoch |
| --- | --- |
| Remove or reshape a shipped message, variant or field | bumps |
| Change what a shipped message means | bumps |
| Add a message or variant | stays (decision 2) |
| Add data to a struct every client receives | stays (decisions 2 and 3) |
| Fix or change daemon behavior with no wire change | stays (decision 4) |

A mismatch behaves as it does now: the client offers to restart the daemon.

## 2. Additive changes are gated by capability

A message or variant added inside an epoch gets a capability name, appended to `CAPABILITY_NAMES`.
The sender checks that the peer has it before sending.

- Client to daemon: a new `ProtocolMessage` variant. The client checks `Welcome::has_capability` and
  hides or disables the feature when it is missing.
- Daemon to client: a new `EventPayload` variant, sent only to clients whose hello named the
  capability.

Appending a variant is safe with postcard as long as an older peer never receives it, which is what
the check guarantees.

Shapes inside state that every client receives (snapshots, tree deltas, the option set) are frozen
for the epoch, because those bytes are encoded once and shared. New data for them travels as its own
gated event, keyed by pane or window, and the client merges it. An epoch bump folds the gated data
into the struct and retires the capability.

`Welcome.caps` holds 64 names, and a bit's meaning is its index, so the table is append-only inside
an epoch. Each epoch bump folds every capability into the baseline and starts the table over.

## 3. Open slots for things every client must see

A client cannot decode a postcard variant it was not built with, so a fallback cannot be added to
clients that already shipped. The slot has to be in the frozen shape before it is needed.

`PaneKindSnapshot` gains `Other { kind: String }`. A pane kind added inside an epoch appears in
snapshots as `Other`, and its descriptor travels in a gated event. A client without the capability
draws a placeholder pane that names the kind and says a newer client shows it.

## 4. Build revisions and floors

`zz-protocol` gains `REVISION: u32`, bumped by hand when behavior a peer depends on changes,
including a fix the other side wants to rely on. `ClientHello` and `Welcome` each carry it.

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
- The restart action states what it ends: "Restart the daemon to update it. This ends N sessions."
  It is `kill-server` followed by a normal start.
- A client update that raises the floor or the epoch says so before it installs, so staying on the
  older client to keep the sessions is a real choice. For that, each release publishes its epoch and
  its `MIN_DAEMON_REVISION` where the update check can read them.
- For ssh hosts, `remote_install_script` runs only when the epoch differs or the remote revision is
  below the floor. Otherwise the client attaches and offers the update as the same action.
- The daemon binary moves out of the app bundle into a directory named for its revision. A running
  daemon keeps its directory until it exits, and the next start uses the newest one. This keeps
  anything the daemon launches from its own path on the build it was started from.

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
2. **The capability rule.** A send-side check on the daemon for gated events, and the rule in the
   wire protocol doc. Done when a client built with one gated request attaches to a daemon built
   before it without a restart, and the request is never sent. One test: the exact frames the client
   writes to a daemon that lacks the capability.
3. **Client UI.** The "daemon is behind" hint, the restart action with its session count, and the
   floor refusal screen.
4. **Delivery.** The daemon directory per revision, and the ssh install that runs only on an epoch
   or floor mismatch.

When step 2 lands, rewrite "the version is a gate, not a negotiation" in
[wire protocol](/protocol/wire-protocol.md) and the Recover row in
[session persistence](/concepts/session-persistence.md).

# Open questions

- One gated event per added field, or one generic side event that carries named extras per pane and
  window? The second makes an added field one line again, at the price of untyped values.
- Is `REVISION` worth a hand-bumped counter, or would the release version do? Builds from main
  between releases share a version, which is why this plan picks the counter.
- Does the terminal lane ever need an additive change inside an epoch? Its packing has no capability
  story here; any change to it bumps the epoch.
- Where a release publishes its epoch and floor: a small asset next to the builds, or a line the
  update check parses from the release body.
- Where the per-revision daemon directory lives on each platform, and when old ones are removed.

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
