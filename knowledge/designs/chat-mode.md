---
type: Design Plan
title: "Chat mode: chat windows first, the rest piled"
description: "The flat sidebar became chat mode: each machine lists its chat windows (a window running Claude Code or Codex) as rows with a face per pane, then folds the windows without a chat into one last row, the pile; a window with two or more panes and the pile open into one-line rows on a track. New chat opens the agent picker with no window behind it, which starts the agent in a chosen folder inside that folder's session. The sidebar view decides what a new pane offers: agents in chat view, pane kinds in the session tree. Desktop, web and iPad share the zz-client model; the HTML client still lists only chat windows."
status: Built (2026-10-28; picker by sidebar view 2026-11-01; New chat row and machine choice 2026-11-06; machines inline 2026-11-09; another machine keeps the New chat screen 2026-11-08; browser in a split 2026-11-07; a New chat screen 2026-11-08; windows as rows and the pile 2026-11-13; the HTML client has no file picker and no pile yet)
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

The sidebar has two views, switched by the chat button beside the sidebar toggle at the top: the
session tree (raw mode) and chat mode,
which replaced the flat window list. Chat mode keeps the [flat sidebar](flat-sidebar-data.md)'s
two-line rows, pane stack, attention and ages. The list is windows, and a stack is a pile you can
open (`window_summary::summarize_chat_machine`, drawn by `zz_ui::navigation::flat`; the board with
every number is `design/mockups/chat-rows/index.html`):

- Per machine, with no session headers, each window holding a chat is a row, in session then
  window order. A chat is a pane whose process resolves to Claude Code or Codex
  (`window_summary::pane_agent`).
- The windows without a chat fold into one last row, the pile: a face per window, with the front
  window's title and second line. The front is the plain window used last, which the client
  remembers (`ChatHints::last_plain`). A machine with one plain window shows that window as a row
  instead. A window whose panes are all pane pickers is left out (`chat_view_lists`).
- A stack shows one face per pane, each with its own status dot. A chat window's faces are its
  chats, then its other panes in window order; the front is the active pane when it is a chat, else
  the chat that last had focus there (`ChatHints::last_chat`), else the first chat. A plain
  window's front is its active pane.
- A chat window's title, second line and age follow the front chat; a chat behind it that needs
  approval or failed takes the second line, its status word then its title. The spinner follows the
  front chat only; a chat running behind shows only its dot.
- A plain window's title is the name it was given (`rename-window`, as the session tree shows it),
  else its front pane's title: the terminal's title, its program, its shell, or a browser's host.
  Its second line is folder and branch, and says more only when a pane rang its bell or a command
  failed. In the pile a bell or failure behind the front window takes the second line the same way.
- A window with two or more panes, and the pile, has a chevron after its title, shown on hover and
  while open, that opens and closes it. Opened, it lists one-line rows, 32 high, on a track from
  the stack's foot: a pane each (face, title, then the short status word, a spinner or the age), or
  in the pile a window each, led by its own small stack, that does not open further. Opening keeps
  the parent row, except that a second line taken by a pane behind goes back to the front pane's
  own.
- A window row shows its window, a nested pane row focuses that pane, a nested window row in the
  pile shows that window, a face focuses its pane, and the pile row shows its front window. Rename
  and Close window are on window rows only.
- The current marker is on the window row, or on the focused pane's nested row while the window is
  open, with the track lit; on the pile row, or on the window's nested row while the pile is open
  (`ChatMachine::current`).
- ⌘ numbers go to chat window rows only, top to bottom across the connected machines
  (`number_chat_rows`); in chat view the desktop's window shortcut keys open those same rows.

Desktop, web and iPad draw this. The HTML client still draws one row per chat window
(`summarize_chats`). An agent splitting its own window with `zz split-window` needs nothing new:
the pane lands in the agent's window and becomes one of its faces. Raw mode shows the same window as
a tree.

# Switching views

Chat view is a place, not a filter over the window on screen: the window in the workspace is always
one the sidebar lists. Every window but a picker-only one has a row, so turning chat view on stays
on the focused window. With no focused window (web, iPad and the HTML client), or on the desktop a
picker-only one, it switches to the window that was on screen when chat view was last turned off,
else the most recently active chat (`entering_chat_view`), and with no chat it shows the New chat
screen. The desktop looks on the attached machine first, then on any connected machine. A client that starts in
chat view does the same once its first snapshot arrives. Turning chat view off from a chat keeps that
chat: the session tree lists every window. Web, iPad and the HTML client keep this state in
`ChatViewPlace`: the New chat screen and the window it covers, the chat left last, and whether the
view has been entered.

# Starting a chat

New chat at the top of the chat view shows the New chat screen: the agent picker with no window
behind it (the empty workspace's picker), so nothing is made until an agent is picked. Picking one
opens a window for it in the folder's session (`new-window -c <folder>`, then `select-pane-kind
agent` on its shell) or, with no session for that folder, a session named after it
(`first_chat_commands`); both are `landing_chat_commands`. The screen goes away once another listed
window, a chat or a plain window, is on screen or a row is pressed, Escape on web, iPad and the HTML client closes it, and turning chat
view off from it shows the window that was there.
Picking another machine on it keeps the New chat screen and attaches the desktop to that machine
(`attach_to_host_default`), so its folders, the folder browser and the chat are that machine's, and
the screen stays up over whatever window the machine had focused: it goes away for a window that was
not there when the screen opened, or for another listed window on the same machine
(`sidebar::flat::landing_follows`). Picking another machine on a picker pane closes that pane and
shows the New chat screen on that machine the same way, so a new chat on any machine starts from one
screen. Pressing another row in
chat view while a picker window is on screen closes that window (`leave_picker_command`), so a New
chat left unpicked leaves nothing behind.

# What a picker offers

The sidebar view decides, not the picker's window (`agent_picker_place`, which every client asks
with its own view: the desktop's `flat-sidebar` setting, zz-app's `flat_sidebar` preference on web,
iPad and iPhone, and the HTML client's preference passed in with `set_chat_view`):

- **Chat view.** Every picker pane offers the agents (`ChatPicker`, drawn by
  `zz_ui::pane::chat_picker_rows`): every machine (desktop only), the folder line, and Claude
  Code (`c`) or Codex (`x`). Each machine is a row with its system's mark (`platform_icon`, a
  laptop or server when the host has not said), the attached one checked and an offline one dimmed
  (`MachineChoice`). A click on another, or `m` for the next live one (`ChatKey::NextMachine`,
  `MachineChoice::next`, whose row shows the `m` key), closes this picker and shows the New chat
  screen on that host, so the folders and the chat are that machine's. A
  picker that is a split (its window holds another pane, `picker_in_split`) on the desktop also
  offers Terminal (`t`, `ChatKey::Terminal`) and Browser (`b`, `ChatKey::Browser`) under Pane,
  which turn the pane into one as the session tree's picker does, so a chat keeps its terminals and
  browsers beside it. A picker alone in its window offers only the agents, since New chat is for
  starting a chat; a terminal or browser there would only add a plain window to the pile. A terminal
  is always a terminal: no client
  draws the picker over a shell. Escape closes only picker panes (`close_picker_command`), and the
  folder line starts on the folder of the pane the picker split from (`picker_folders`).
- **Session tree.** Every picker offers pane kinds, even alone in its window, and a terminal is a
  terminal. Agents start there by typing the CLI into a terminal.

Chat view with nothing open shows the agent picker too. On the desktop that is the empty workspace
(no sessions): the picker takes the place of the New session card, and picking runs
`first_chat_commands` (`new-session -d -s <folder name> -c <folder>`, `select-pane-kind -t <name>:
-c <folder> [--model m] [--effort e] agent <cli>`, `attach-session -t <name>`). Web and iPad never
sit on an empty host: the daemon makes a session when a client attaches to none, so a host with no
chats lands on a shell, which they show as a shell.

Tab or a click on the folder opens the file picker (the command palette's insert path browser,
`CommandPaletteView::new_folder_pick`) one folder up from the current one; Enter there hands the
highlighted folder, or the folder holding a highlighted file, back to the picker as where the agent
starts (`PaletteActionLevel::picked_folder`) instead of inserting it. The folder browser lists with no
terminal open: a folder named from its root goes out with no pane, or through a pane that is not a
terminal (wire 118, see [the wire protocol](/protocol/wire-protocol.md)). The HTML client has no file
picker yet, so its Tab lists the folders the host's panes have been in and takes a typed path
(`folder_rows`).

# Starting the agent

Choosing an agent runs `select-pane-kind -t <pane> -c <folder> [--model m] [--effort e] agent
<claude|codex>`
(`start_agent_commands`), which turns the picker pane into an agent pane running the driver, then puts the window in that folder's session:

- the folder's session is the picker's own: nothing moves;
- another session is the folder's: `move-window` into it;
- no session is the folder's: `new-session -d -s <folder name>`, `move-window` into it and
  `kill-window -a` its starting shell; a session holding only the picker is renamed instead.

The desktop switches to the window if it moved sessions. No client draws a chat over a terminal.
`flip-agent` switches a pane between the CLI and the chat on the same session (see the
[agent panes](/concepts/agent-panes.md)). The iPhone's pill menu offers it; the pane headers on the
desktop, web, iPad and the HTML client dropped their button on 2026-11-08 until the chat view
settles. The command palette lists the chats too (see the
[palette](/concepts/command-palette.md)).
A new chat starts on the model and effort last picked: `start_agent_commands` and
`first_chat_commands` add `--model` and `--effort` from `zz_client::agent_launch::AgentLaunches`
(`agent_flags`). The desktop remembers them per CLI in `agent-launch.json` from the chat's model
and effort menu; web, iPad, iPhone and the HTML client remember none yet, so their chats start on
the CLI's default.

# Daemon changes

Both are additive to native surface: `new-window` accepts `--kind picker`, and `select-pane-kind`
takes `-c start-directory` and a trailing command for the `terminal` kind (a leading `~` names the
daemon's home). A remote host on an older daemon answers a picker window with a parse error. The agent
flip adds the `agent <claude|codex>` kind to `select-pane-kind`, for a picker or a terminal pane.

# Not built

- A way back to the CLI from a desktop agent pane.
- A file picker in the HTML client.
- Remembering the last picked model and effort on web, iPad, iPhone and the HTML client.
- Agent CLIs beyond Claude Code and Codex; the list follows the programs zz can drive.
- The HTML client draws agent tiles with kit glyphs rather than program marks.
