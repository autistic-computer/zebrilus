---
type: Design Plan
title: "Chat mode: the sidebar lists agents, not windows"
description: "The flat sidebar became chat mode: each machine lists one row per window running Claude Code or Codex, the agent's tile leads the pane stack with the panes it split behind it, sessions and agentless windows stay in the session tree, and a host's New agent opens a picker window that starts the CLI in a chosen folder inside that folder's session. Desktop, web, iPad and the HTML client share the zz-client model."
status: Built (2026-10-28; the HTML client has no file picker yet)
resource: crates/zz-client/src/new_chat.rs
tags:
- sidebar
- agent
- chat-mode
- picker
- design-plan
timestamp: 2026-10-28T00:00:00-03:00
---

# Overview

The sidebar has two views, switched from the footer: the session tree (raw mode) and chat mode,
which replaced the flat window list. Chat mode keeps the [flat sidebar](flat-sidebar-data.md)'s
two-line rows, pane stack, attention and ages, and changes what a row is:

- A row is a window that runs an agent: a terminal whose process resolves to Claude Code or Codex
  (`window_summary::pane_agent`). Windows without one are left out, and so are session headers;
  rows sit straight under their machine. `summarize_chats` flattens the sessions in order.
- The agent's own tile leads the stack whatever pane has focus, and the panes it split (a browser,
  a dev server) are the tiles behind it, so pointing at the stack still opens any of them. The
  title and second line come from the agent pane (`summarize_chat`).
- A window holding nothing but a picker is a chat being started: it shows as "New agent / Pick a
  CLI" with a picker tile, on every client, with no client state.

An agent splitting its own window with `zz split-window` needs nothing new: the pane lands in the
agent's window and so belongs to the chat. Raw mode shows the same window as a tree.

# Starting a chat

A connected host's header shows New agent on hover (`FlatEvent::NewChat`). It opens a picker
window, `new-window -t <session>: --kind picker`, in the session of the folder the host's agents
used last (`recent_folders`, `session_for_folder`); a host with no session gets one named after
that folder. The desktop follows the new window once it appears.

A picker alone in its window offers the agents instead of pane kinds (`ChatPicker`, drawn by
`zz_ui::pane::chat_picker_rows`): the folder line on top, and Claude Code (`c`) or Codex (`x`).
Tab or a click on the folder opens the file picker (the command palette's insert path browser,
`CommandPaletteView::new_folder_pick`) one folder up from the current one; Enter there hands the
highlighted folder, or the folder holding a highlighted file, back to the picker as where the agent
starts (`PaletteActionLevel::picked_folder`) instead of inserting it. The HTML client has no file picker
yet, so its Tab lists the folders the host's panes have been in and takes a typed path
(`folder_rows`).

Choosing an agent runs `select-pane-kind -t <pane> -c <folder> terminal <cli>` (`start_agent_commands`),
then puts the window in that folder's session:

- the folder's session is the picker's own: nothing moves;
- another session is the folder's: `move-window` into it;
- no session is the folder's: `new-session -d -s <folder name>`, `move-window` into it and
  `kill-window -a` its starting shell; a session holding only the picker is renamed instead.

The desktop opens the agent's lens as soon as the CLI is detected in that pane, and switches to the
window if it moved sessions. In chat mode every agent pane opens with its lens on, not only new
ones; turning a lens off keeps that pane on its terminal until the agent exits. The command
palette lists the chats too (see the [palette](/concepts/command-palette.md)).

# Daemon changes

Both are additive to native surface: `new-window` accepts `--kind picker`, and `select-pane-kind`
takes `-c start-directory` and a trailing command for the `terminal` kind (a leading `~` names the
daemon's home). A remote host on an older daemon answers New agent with a parse error.

# Not built

- A file picker in the HTML client.
- Agent CLIs beyond Claude Code and Codex; the list follows the programs the lens supports.
- The HTML client draws agent tiles with kit glyphs rather than program marks.
