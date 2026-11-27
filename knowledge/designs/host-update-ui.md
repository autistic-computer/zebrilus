---
type: Design Plan
title: Host updates in the app . outdated hosts in the sidebar and Settings
description: "The desktop app shows which hosts run an older zz and updates them with one click, over ssh or the tailnet. A host is outdated when its running daemon's version is below this app's, read from the facts `zz update --check` already asks for. The app's Update runs the same code as `zz update <host> --restart`, so the owned copy and `restart-server` replace the shell script the app piped over ssh, and tailnet hosts get the offer for the first time. An outdated host carries an arrow in a ring on its sidebar row, and Settings > Hosts is a table with each host's version and Update or Restart beside it. Nothing updates on its own. Merged 2026-11-25, drawn in the storybook, never run against a real host."
status: Merged 2026-11-25; the update run is unchecked on real hosts
tags:
- update
- daemon
- hosts
- sidebar
- settings
- design-plan
timestamp: 2026-11-27T00:00:00-03:00
---

# Overview

[Daemon update](/designs/daemon-update.md) gave every machine an owned headless copy and one verb,
`zz update`, that installs it over ssh or the tailnet. The desktop app uses none of it. This plan
points the app at that code and gives an outdated host a place on screen.

Three parts:

1. **One meaning of outdated.** The running daemon's version against this app's version.
2. **One route.** The app's Update is `zz update <host> --restart`, called as a function.
3. **Two places.** An icon on the host's sidebar row, and a version with a button in
   Settings > Hosts.

Not in this plan: automatic updates, updating on a schedule, the phone and web clients, a button
for this machine's owned copy, `--remove`, `--beta` and picking a version, and a way into a tailnet
daemon on another epoch.

# Today

Read on 2026-11-24 at `cb7fc27c3`.

- **The offer is for ssh hosts only.** `MuxClient` raises the update offer when a host connects or
  fails, and only when its endpoint is `Endpoint::Ssh` (`crates/zz/src/mux/client.rs`, the
  `install_offered` block). `install_zz_on_host` returns at once for any other endpoint.
- **The route is a shell script.** Accepting the offer runs `install_remote`, which pipes
  `install.sh` through an ssh shell and then sends `kill-server` (`remote_install_script` in
  `crates/zz-daemon-client/src/endpoint.rs`). A tailnet endpoint has no shell, which is why the
  gate above exists. The dialog says so: "Updating installs zz {version} in ~/.local there"
  (`update_prompt` in `crates/zz-daemon-client/src/host_failure.rs`). On a host whose
  `~/.local/bin/zz` belongs to a package, the script exits with `zz-install-elsewhere`.
- **A tailnet host gets a menu item that does nothing.** The sidebar's right-click menu adds
  "Update zz" for any host with an offer, with no look at its endpoint
  (`render_tree_row_context_menu` in `crates/zz/src/workspace/sidebar.rs`). On a tailnet host the
  dialog opens, and "Update and restart" runs `install_zz_on_host`, which returns.
- **Outdated means revision.** `MuxClient::daemon_behind` compares the daemon's `REVISION` from the
  handshake with this build's. The wire carries no version. 0.29.0, 0.30.0 and 0.31.0 are revisions
  6, 8 and 9, so the app noticed those; a release that leaves `REVISION` alone is invisible to it,
  while `zz update --check` sees it through `#{zz_version}`.
- **The sidebar row says nothing about it.** `MuxTreeHost::indicator` gives a spinner while a host
  connects and a warning triangle when it failed. A connected host that is behind shows neither.
- **Settings > Hosts shows no version.** Each row is a globe tinted by connection state, the
  endpoint, and a remove button (`hosts_section` in `crates/zz/src/config/settings.rs`).
- **The owned copy is never used.** An update from the app does not install an owned copy, does not
  send `update-server`, and restarts with `kill-server`, not `restart-server`.

# Decisions

## 1. Outdated is the daemon's version against this app's

The target is always this app's version, the rule `zz update <host>` already has. A host is never
compared with the newest release: when this app is old, its own update toast says so, and the hosts
follow once it has updated.

The app asks each host for the facts the CLI asks for,
`#{zz_version}|#{zz_revision}|#{server_sessions}|#{zz_exit_rule}|#{zz_owned_version}|#{zz_owned_epoch}`,
with `display-message -p` through a command client on the host's endpoint, off the UI thread. It
asks when a host connects, when it reconnects, and after each install and restart.

| State | When | Shown as |
| --- | --- | --- |
| Unknown | not connected, or the facts have not come back | nothing |
| Current | daemon version is this app's or newer | nothing |
| Outdated | daemon version is older, and the owned copy is older than this app's version or missing | the icon, Update |
| Restart pending | daemon version is older, and the owned copy is this app's version or newer | the icon, Restart |
| Updating | an install or a restart is running | a spinner |
| Failed | the last install or restart failed | the icon with the reason, Try again |
| Cannot update from here | the host has no `update` verb or `update-server`, or it is a tailnet host on another epoch | the icon, "Update zz on {host} itself" |

- A daemon that answers `#{zz_version}` with nothing predates the variable. It counts as older.
- A daemon with a lower revision counts as outdated whatever its version says, so a build between
  releases is still caught.
- A host that cannot attach (`Incompatible`, `DaemonTooOld`) keeps its warning triangle. Its action
  is Update when a route exists: always for ssh, never for a tailnet host on another epoch.
- A host on a newer zz (`ClientTooOld`, or simply a higher version) is not outdated. Its text stays
  "Update zz on this machine, then reconnect."
- The state is a pure function of the facts and this build's version, epoch and revision. It lives
  beside `DaemonBehind` in `crates/zz-daemon-client/src/host_failure.rs`, so another client can
  use it later.

## 2. The app's Update is `zz update`, called as a function

`crates/zz` already depends on `zz-cli`. The per-host steps in `crates/zz-cli/src/update.rs` become
functions the CLI and the app both call:

| Step | ssh host | tailnet host |
| --- | --- | --- |
| facts | remote CLI, `display-message` | command client, `display-message` |
| install | remote CLI, `update --version V` | `update-server --version V` |
| restart | remote CLI, `update --restart -y` | `restart-server`, then wait for the successor |

- The CLI keeps the printing, the questions and `-a`. The functions return values: the facts, the
  installed version, the version the daemon came back on.
- Progress lines from `update-server` reach the caller as they do the CLI's stderr today. The app
  keeps the last one for the row in Settings.
- The app runs a host's update on a thread of its own, as `install_zz_on_host` does now, and ssh
  prompts go through `ssh_prompts` as they do now.
- **A host too old for the verb.** An ssh host whose zz lacks `update` falls back to the script
  route that exists today, so what works now keeps working. A tailnet host in that state is
  "Cannot update from here".
- **Install zz** for a host with no zz at all stays on `install_remote`. That is not an update.
- The dialog text loses "in ~/.local": an update no longer writes there.

## 3. Nothing runs until asked

- No update starts on its own, on connect or later.
- The dialog that opens by itself when an outdated ssh host connects goes away. The icon is the
  notice. `install_offered` and `RemoteInstallOffer` go with it.
- Update on a host with no sessions runs at once: install, then restart.
- Update on a host with sessions asks first, once: "Update zz on {host}?" with "This ends
  {n} sessions there." and three buttons: **Update and restart**, **Install only**, **Not now**.
  Install only leaves the host in Restart pending with its sessions running.
- Restart, from Restart pending, asks the same way with **Restart** and **Not now** when the host
  has sessions.
- The session count in the question comes from the facts fetched for that click, not from a
  snapshot that may be stale.

## 4. The sidebar row carries an icon

- The board is `design/mockups/host-update/index.html`. fabrico picked take 2, the ring, on
  2026-11-24: Outdated is a new glyph, an arrow up in a ring, in accent. Restart pending is a new
  restart glyph in accent. Cannot update from here is the ring in muted. Failed keeps the warning
  triangle.
- The icon sits where the spinner and the warning triangle sit today (`tree_host_indicator` in
  `crates/zz-ui/src/navigation/sidebar.rs`), so a row never shows two of them. A failed connection
  wins over outdated.
- Its tooltip is the two versions: "zz 0.30.0, this zz is 0.31.0". Restart pending says
  "zz 0.31.0 installed, restart to finish".
- A click runs the state's action, as the menu item does.
- The right-click item follows the state: "Update zz to 0.31.0", "Restart to finish the update",
  or no item when the host cannot be updated from here.
- While it runs, the row shows the spinner it shows while connecting.
- This machine's row gets the same icon when the local daemon is behind the app. Its click opens
  the "Restart the zz daemon?" question that exists today.

## 5. Settings > Hosts shows the version and offers the update

- The card is a table, take B on the board, like the Agents card: platform mark and name, address,
  a zz column, the buttons. fabrico confirmed it on 2026-11-24.
- Each connected host's row shows its daemon's version, with a check when it is current.
- Outdated: an **Update** button before the remove button.
- Restart pending: a **Restart** button, and the row says the new version is installed.
- Updating: the last progress line and a spinner in place of the button.
- Failed: the reason under the row, and **Try again**.
- Cannot update from here: "Update zz on {host} itself", no button.
- With two or more hosts outdated, the section header carries **Update all**, which asks once with
  the hosts and the total of sessions, then runs them in parallel.

## 6. zz Dev

`zz update` refuses on a zz Dev build, and a Dev build's version means nothing to a host. A Dev
build fetches no facts, shows no icon and offers no update.

# Build order

1. The state in decision 1 as a pure function with its tests.
2. The per-host functions out of the CLI's `update.rs`, with the CLI calling them. `zz update`
   prints what it printed before.
3. `MuxClient`: fetch facts on connect, hold a state per host, `update_host` and `restart_host`
   on the new functions. The ssh gate, `install_offered` and the dialog on connect go.
4. The board for the icon and the Settings rows, then the sidebar icon, its tooltip and menu item.
5. The Settings rows and Update all.
6. Pages: the sidebar and settings concept pages, and [daemon update](/designs/daemon-update.md)
   loses "a desktop button for any of it" from its exclusions.

# Done when

1. With the app on a newer version than alienware (tailnet), alienware's sidebar row shows the
   icon within 5 s of connecting, and its tooltip names both versions.
2. Clicking it with no sessions on alienware leaves `#{zz_version}` there at the app's version with
   no further click, and the icon gone.
3. With sessions there, Install only leaves the sessions running and the row in Restart pending;
   Restart then ends them and clears the icon.
4. The same on xps over ssh and on win-desktop over the tailnet.
5. Settings > Hosts shows each connected host's version, and Update there does what the icon does.
6. A host updated from another machine loses its icon when its daemon comes back, with no click
   here.
7. A release that does not move `REVISION` still marks the hosts outdated.
8. `zz update --check -a` prints the same versions the app shows.

# Tests

- The state function as a table of exact picks: same version; older daemon with no owned copy;
  older daemon with an older owned copy; older daemon with the owned copy at the target; empty
  `#{zz_version}`; same version with a lower revision; newer daemon; tailnet host on another
  epoch; ssh host on another epoch.
- The dialog text for one host with 3 sessions and for two hosts with 0 and 2, as exact strings.

The rest is checked on xps, alienware and win-desktop against the list above.

# As built

Built on 2026-11-24 on branch `host-update`, in the build order above. Decided where the plan was
silent, or built differently:

- **Settings > Hosts is take B, the table.** fabrico confirmed it on 2026-11-24. The zz column is
  176 wide, not the board's 150: "0.28.0, update it there" did not fit. A failed update shows its
  reason under the row, as decision 5 says, not in the cell as the board drew it. A host that is
  not connected shows its connection state in the zz column, since the table has no tinted globe.
- **The question's text** follows the board: "This ends 3 sessions there." for one host, and for
  several "This ends 2 sessions: 0 on xps, 2 on alienware.", hosts with no sessions listed too.
  Its accept button is the primary one, as on the board, not the red one the old dialog had.
- **Update all** covers hosts that are outdated and hosts that only need the restart, and shows
  when there are two or more of them. It asks only when one of them has sessions. When every
  host in the run only needs the restart, the question is the restart one.
- **A click fetches the facts again** before anything runs, for the session count. The row shows
  the spinner from the click on, and Not now puts the icon back.
- **Same version, lower revision** is Outdated with the Update action. The install answers
  "already zz {v}" and the restart then starts whatever binary the host has on disk.
- **A daemon that answers `#{zz_version}` with nothing** is Outdated on both routes. A tailnet
  host in that state turns into "Cannot update from here" when its daemon answers `unknown
  command: update-server` or has no `restart-server`, and stays so until the app restarts.
- **A host that cannot attach** takes its state from the failure, not from facts: an ssh host on
  an older epoch or below the daemon floor is Outdated, a tailnet host on another epoch cannot be
  updated from here, a newer host shows nothing. Its row keeps the warning triangle, so its
  action is in the row's menu and in Settings.
- **A failed update** shows the warning triangle with the reason as its tooltip, and a click
  tries again. The failure is forgotten when the app restarts.
- **This machine's row** takes its state from the same facts. Its click opens "Restart the zz
  daemon?", and it has no menu item.
- **No toast on success.** The icon going away is the notice. A failure toasts its reason.
- **ssh progress.** The remote CLI prints no progress without a terminal, so an ssh host shows
  "Updating" with no line. A tailnet host shows `update-server`'s lines: "downloading zz 0.31.0",
  "checking the download", "unpacking zz 0.31.0", not the byte counts the board drew.
- **The script fallback** for an ssh host without the `update` verb also covers Install only: the
  script then installs without restarting.
- **`install_offered` stayed as `auto_installed`**, for the one thing it still does: a host with
  no zz gets one automatic install per app run.
- **The thin clients keep `update_prompt`** and the script route. The desktop's questions are
  `host_update_question` and `host_restart_question`; "in ~/.local" is gone from those only.
- **penna** got two glyphs (`RingUp`, `Restart`) and `AlertDialog::action` for the third button.
- **A command client's stdout handler** is a closure now, so the app can keep the last progress
  line per host.

Checked: the state table and the question strings as tests, the existing `zz update` tests
unchanged, and the Hosts table and the four sidebar marks drawn in the storybook
(`#/settings/hosts`, `#/sidebar/markers`). fabrico picked the storybook over a Dev switch for open
item 5, so no Dev build shows any of this.

Never run: every item of Done when. No facts fetch, install or restart has run against a daemon,
the question dialog has not been seen in the app, and the sidebar row has not been seen with the
mark in place.

A WSL host gets no mark: `zz update` does not reach a WSL distribution, so the app reads no
version there. `zz fleet install NAME` updates it.

# Open

1. Whether the question should also open by itself when a host cannot attach at all, since that
   is the one case where the user is stopped.
2. A failed update and a host found to lack the verbs are both forgotten on app restart.

# Citations

- `crates/zz/src/mux/client.rs` `daemon_behind`; `mux/client/connection.rs` `install_zz_on_host`;
  `mux/client/commands.rs` `execute_on_host`
- `crates/zz/src/mux/client/host_update.rs` `host_update`, `fetch_host_facts`,
  `request_host_updates`, `run_host_updates`, `HostUpdateQuestion`
- `crates/zz/src/mux/nav.rs` `MuxTreeHost::install_offer`, `MuxTreeHost::indicator`,
  `MuxTreeHost::update_label`, `HostIndicator`
- `crates/zz/src/mux/hosts.rs` `HostState`
- `crates/zz/src/workspace/sidebar.rs` `render_host_indicator`, `render_tree_row_context_menu`
- `crates/zz/src/workspace/host_update.rs` `ask`
- `crates/zz/src/config/settings.rs` `hosts_section`, `machines_stack`
- `crates/zz-ui/src/settings/hosts.rs` `hosts_table`, `hosts_heading`, `HostZz`
- `crates/zz-ui/src/navigation/sidebar.rs` `tree_host_indicator`, `tree_host_update_mark`
- `crates/zz-daemon-client/src/host_failure.rs` `DaemonBehind`, `InstallOffer`, `update_prompt`,
  `UpdateState`, `host_update_question`, `host_restart_question`
- `crates/zz-daemon-client/src/endpoint.rs` `install_remote`, `remote_install_script`
- `crates/zz-cli/src/update.rs` `DAEMON_FACTS`, `Daemon`, `parse_daemon_facts`,
  `endpoint_daemon`, `ssh_install`, `tailnet_install`, `update_server`, `restart_server`,
  `daemon_behind`, `host_daemon`, `host_install`, `host_restart`, `HostUpdateError`
- `crates/zz-daemon/src/status/hooks.rs` `zz_version`, `zz_revision`, `zz_owned_version`
- `crates/zz-protocol/src/message.rs` `REVISION`
- [daemon update](/designs/daemon-update.md), [split updates](/designs/split-updates.md)
