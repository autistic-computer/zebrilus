---
type: Design Plan
title: "Delegating from an agent chat"
description: "The composer can mention a child agent or an existing chat. The daemon retains the parent and routes replies and asks."
status: "Built 2026-11-23"
resource: crates/zz-daemon/src/daemon.rs
tags:
- agent
- agent-pane
- mentions
- delegation
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

# Mentions and ownership

Selecting an agent from the composer's `@` menu inserts a mention. The host agent writes a brief and opens the child with `zz new-pane -d --kind agent NAME 'BRIEF'`. A mention of an existing chat can send it a message. The picker, model selection and target grammar are described in [agent panes](/concepts/agent-panes.md).

A child starts in the folder a terminal would inherit unless the host supplies `-c`. The daemon records its parent. Closing the host closes its children, and a child cannot open children of its own. A child uses the full auto-approve tier unless the global `agent-auto-approve` setting is off. Questions return to its host. With auto-approve off, permission asks also return to the host.

# Messages

The host can send a prompt to an existing chat with `agent-send -t %N 'TEXT'` or read it with `capture-pane -p -t %N`. Replies and asks queue behind the host's active turn without consuming its user-prompt slots. The relay includes the request ID in each offered `agent-respond` command, and checks at dispatch that the request is still pending. This prevents a delayed answer from resolving a newer ask.

A child stays in its card until selected. The card opens its [floating pane](/designs/pip-panes.md). Child notifications are muted while the sidebar still shows the pane's status. A message to a separate chat creates sent and reply rows.
# UI choices

The desktop behavior lives in `crates/zz-ui` and `crates/zz`.

- **`@` menu.** Agents is the first group on a bare `@`; typed text ranks agents in the flat
  list. The row's dim model is the last pick, else the catalog's current model, else `default`.
  Tab opens the provider's models from `agent-catalog`, its first client; a typed `codex/` or
  `+codex/` opens the models level. Agent picks are not kept as recents. In a transcript
  `@+codex` shows as `@codex`, tinted and not clickable.
- **Child card.** Built from the host's own transcript with no wire change. A pane is a child only
  when the timeline holds its `opened %N` line. An exchange starts at a tool call whose output
  carries the opened or sent line for a child and absorbs the asks, reports and the host's
  `agent-respond` calls for that pane; a second send to the same child gets its own card. A
  decision counts only when its call succeeded; a failed call followed by the same call succeeding is hidden with it; the
  header counts every decision (allowed in green). The latest card reads the pending request and
  closed state from the child's own chat. The peek shows the tool steps of the child's last turn;
  the reply is drawn as markdown; a closed card shows no time; a denial shows as `denied by NAME`.
- **Peer rows** (2026-11-23, board `design/mockups/peer-message/takes.html`, take 3). An
  `agent-send` to a pane this chat did not open stays a tool row with its own title and names the
  peer with a chip: the peer's tile at 16 with its status dot, working until the reply arrives,
  and the chat's title, or `%N` once the pane is gone; the chip takes the place of `%N` in the
  title, or follows it, and clicking it focuses the peer through the pane link path. The finished
  report is one row: a reply arrow, "Reply from", the chip, the time since the send and the
  reply's first line; clicking the row opens the whole reply under it with a rule. Stopped,
  failed and closed reports and relayed asks use the same row. Before, such a send folded into a
  child card whose click ran `select-child-chat` and failed with `%12 is not a chat %6 opened`
  (`f7a85787c`; `zz_client::peer`, `crates/zz-ui/src/agent/chat.rs`).
- **PiP.** On the desktop a child is not drawn as a float until it is the active pane, which the
  card's click makes it; stashing or throwing it hides it again with no strip
  ([PiP panes](/designs/pip-panes.md#piles-and-stashing)). The card collapses to its header while
  the child is in view.
- **Quiet.** A child plays no sound and posts no banner when it finishes, fails or asks; the
  desktop learns it is a child from `AgentParents` and drops its edges in
  `zz_client::attention::announced`. Its sidebar badge stays.

# Reopening a child

A child is its session, not its pane (fabrico, 2026-11-17). The X on its PiP keeps killing the
pane like every PiP's X; nothing asks first. Clicking the card runs `select-child-chat -t HOST
%CHILD`, which brings the child into view and prints its pane: the pane that holds it now, or a new
float in the host's window resumed on its session with the same provider, folder, model, effort and
account, parented to the host. The daemon keeps one record per child under the host's session, next
to the host's journal, so this works after a restart and after the host is resumed in another pane,
and the records go when the host's session is deleted. A child closed before its session started
cannot come back: `%N was closed before its session started; it cannot be opened again`. Once
reopened, notices and reports name the new pane, so the host's transcript gets a card for each id
the child has had. The record keeps every one of them (the original and each reopened pane), and
`select-child-chat` and the host's `agent-send -t %N` accept any of them: the send reaches the pane
that holds the child now and counts toward one set of 8 messages per child, and while the child is
closed it fails with `%N was closed; open a new chat
with: zz new-pane -d --kind agent NAME 'BRIEF'`. See
[Agent panes](/concepts/agent-panes.md#agent-panes) for the record and lookup order.

The card keeps the pane the verb printed in the chat view's state (`ChatOpen::children`), so a
reload or restart forgets it and the next click asks the daemon again. Its current pane is the
original one if alive, else that remembered one if alive, else none; the peek, the live state and
the in-view collapse read it. With no current pane a card keeps the state its report gave (done,
stopped, failed); a card closed by its report, or the latest card with no report, shows closed. Every
card keeps its PiP mark and stays clickable, and only a click the daemon refuses shows a message,
as the command's error toast. The desktop reads the printed pane from the command's response
(`MuxClient::execute_printed`), the thin client from its connection's command response.


# Recorded checks and open work

The reports and per-run results live under `bench/agents/mentions/results/`. The 2026-11-23 desktop check covered the child appearing on selection and hiding on leaving, sidebar stack colors, peer sent and reply rows, and focusing a peer through its chip. That check did not exercise expanding a reply, notification silence, the typed `@` picker and model level, a real permission ask on the card, phones, very long tool output or the first zz command in a real Codex chat outside the rig.

The remaining design questions are whether to pass a child context beyond its brief and working directory, and whether to limit `agent-send` to chat panes. The November review also identified candidate protocol messages without callers. Each needs its own dispatch and compatibility check before removal.

Daemon routing is in `crates/zz-daemon/src/daemon.rs`, `daemon/agent_inbox.rs` and `daemon/agent_relay.rs`. See [agent panes](/concepts/agent-panes.md) for current CLI behavior and [native drivers](/designs/native-agent-drivers.md) for provider behavior.
