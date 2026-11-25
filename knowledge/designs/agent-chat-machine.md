---
type: Design Plan
title: "Agent chat state and replay"
description: "Typed chat events carry turns, items, subagents and requests from provider drivers to client views."
status: "Implemented, verified against source 2026-11-24"
resource: crates/zz-agent/src/chat.rs
tags:
- agent
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

The daemon converts provider frames into typed facts, runs the chat and [turn machine](/designs/agent-turn-machine.md), and publishes ordered `ChatEvent` records. Clients fold the events into the same `ChatView` for live updates and replay. This is the current driver-backed [agent pane](/concepts/agent-panes.md). The terminal transcript lens was retired in November 2026.

# Ownership

| Responsibility | Source |
| --- | --- |
| Project turn, item and request lifetimes into chat events | `ChatMachine` in `crates/zz-agent/src/chat.rs` |
| Admit, dispatch and reclaim live prompts | `PanePump::prompt`, `dispatch_next` and `reclaim_queue` in `crates/zz-agent/src/host.rs` |
| Translate and apply facts from the drivers | `crates/zz-agent/src/chat_bridge.rs` |
| Protocol events, projection and compacted chat records | `ChatEvent`, `ChatView::apply` and `ChatLog::in_effect` in `crates/zz-protocol/src/chat.rs` |
| Replay, including a client whose requested sequence fell out of the ring | `crates/zz-agent/src/fanout.rs` |

# Replay

A client that attaches late asks the fanout lane for a replay. While the lane's memory still holds
the requested items it sends them; once older items were dropped it sends a reset and rebuilds from
the journal or from memory, and the chat events would be lost. So the lane keeps a `ChatLog`: every
chat event in order, with text appends and body changes folded into the item's start and only the
latest queue and usage kept. The journal path ends with `Restart` and that whole log, which folds to
the same view (the random test checks it at every event). The rebuild from memory goes on the
numbers the asker skips and stays within 4.5 MiB: it sends `Restart`, the events of the log still in
effect (`ChatLog::in_effect`: the last session start and end, the running turn, every open item and
the item of every open request with their parents, every open request, the turns those started in,
the queue and the usage) and then the newest events that fit, all in their original order. A
rebuilt chat can show an old turn holding only its still-open rows, and an open item without its
finished children. When the rebuild needs more numbers than the asker skips, the lane first takes
exactly the missing ones with repeats of its latest usage event (its latest queue event when it
has no usage, an empty queue when it has neither), which every viewer gets and which change no
client's chat, and the asker gets the rebuild on those numbers too. Only when the events still in
effect pass 4.5 MiB on their own does it send no chat, and the asker keeps the chat it had. The
log grows with items and turns, not with streamed text, and keeps pasted images only on the last 8
items that have them.
`ChatView` applies the same rule as it folds events, so a connected client's view holds no more
images than a replay would give it. A daemon restart needs no journal for this: the driver restores
the session's history and the bridge replays it through the machine. When the vendor's history is
gone, Claude and Codex fall back to the journal, and the bridge opens a turn at every turn mark in
it. The vendor history readers write those marks, and a live turn now journals its own start and
end mark too (`Shadow::marking_turns`), so the fallback rebuilds the turns; journals written before
2026-11-22 have none and reload with every row outside a turn. An ACP provider that cannot load a
session falls back to the journal too. There the bridge opens a turn at each run of user text, as
it does for a loaded session, so an ACP turn journals only its end mark (`Transcript::ended_turns`,
handed to the journal by `dialect::run_through`), and a turn that was stopped or failed reloads
that way; an ACP journal written without them reloads every turn as completed. When a person
declines an ask, the shadow journals a note naming the tool (`_meta.zz.declined`) the same way,
and the fallback reads it so the tool's failed end shows as declined, as it did live; Claude and
Codex already carry the decline in the tool update they journal. The fanout's journal replay
leaves the marks and notes out (`is_zz_note`), as it does `config_option_update`, and the ACP
dialect keeps the ones a fallback replays out of the stream it forwards.

# Chat projection

- A child item inherits its parent's turn or background scope. If the parent has not arrived yet, the machine can buffer up to 256 facts. Requests open immediately, so a missing item cannot hide an approval.
- Ending a turn closes its open foreground items and requests. Background items and their requests survive. A session reset clears the old IDs so restored history can reuse them.
- An answer or withdrawal closes the matching request. Remembering the last 8,192 closed items and 1,024 closed requests lets the machine reject stale updates. When a user prompt takes over a turn the agent started, the projection emits `TurnClaimed`.
- `Shadow::step` in `chat_bridge.rs` collects the machine's `Emit` effects for the outgoing chat stream. The model also describes queue and driver effects. Its `Submit`, `SendFailed` and `Unqueue` facts are exercised by `chat_tests.rs`. Live prompts enter the host's `PanePump::prompt`, which dispatches when the phase accepts a prompt or applies queue count and byte limits. Follow-ups call `dispatch_next` or `reclaim_queue`. The live `HostCommand::Unqueue` goes directly to `reclaim_queue`.

# Evidence and remaining limits

`crates/zz-agent/src/chat_tests.rs` covers transition behavior. `crates/zz-agent/src/cut_points.rs` exercises late attach, journal reload and session resume at different points in provider recordings. The expected findings in each sweep and the scope of its comparison live beside the test cases. A recording covers only the provider paths and event shapes it contains.

A rebuild can omit chat state when the events still in effect exceed `MAX_REBUILD_BYTES` by themselves. The client retains its prior chat in that case. The November 22 campaign also recorded differences in how provider history and the journal preserve permission prompts, question options and internal turn marks. Its full observations and proposed driver follow-ups are in `git show 1ce632dce:knowledge/designs/agent-chat-machine.md`.

For how permissions, queued prompts and subagents appear in a client, see [agent panes](/concepts/agent-panes.md).
