---
type: Design Plan
title: Artifacts . agent-written HTML in browser panes and inline
description: "Proposed plan for showing HTML an agent writes. zz html <path> registers the page with the daemon that ran the caller, browser panes load it from a zz://<token>/ scheme whose bytes the daemon sends over the client's existing connection (so it works on ssh hosts), the daemon reloads the pane when a file the page loaded changes, pages get no bridge by default, and --reply grants one call that sends text to the pane that opened the page. In the Agent pane, svg blocks render inline through the mermaid path, and html blocks show code with a Preview toggle that one hidden browser renders as cached images; a click makes one block live or opens it as a pane. Replaces the 2026-07-28 userpanes plan."
status: Proposed
resource: crates/zz-browser/src/cef_runtime.rs
tags:
- browser
- cef
- panes
- scheme-handler
- agent-canvas
- agent-pane
- artifacts
- design-plan
timestamp: 2026-10-18T00:00:00Z
---

# Overview

Claude's artifacts, ChatGPT's canvas and MCP Apps all do one thing: the model writes HTML and the
host shows it in a sandboxed frame next to the conversation. zz has Chromium in every desktop
window, and every agent running in zz already knows HTML and the shell. So the feature is: an agent
writes a file, runs `zz html`, and a browser pane opens beside it showing the page, and shows it
again each time the agent rewrites the file.

```sh
cat > /tmp/report.html <<'EOF'
<!doctype html><title>Bench</title> ...
EOF
zz html /tmp/report.html
```

The model learns nothing new: HTML, a file path, one verb. Smaller drawings stay in the
conversation: `svg` and `html` blocks in the Agent pane preview inline (see Inline previews).

# What changed since the July plan

The 2026-07-28 version of this page planned "userpanes": HTML panes users install under
`~/.config/zz/panes`, with a `window.zz` bridge that could run any verb. Agent canvases were one
use among several. Three things it assumed no longer hold.

1. **The file is not on the client's disk.** It served `zz://file/<abs-path>` from the GUI's own
   filesystem. Since then zz attaches to daemons on other machines over ssh. An agent on a devbox
   writes `/tmp/report.html` there, and the Mac running Chromium has no such file. The bytes have
   to come from the daemon that ran the agent.
2. **The verb catalog is not a safe boundary.** The plan gave pages `zz.run(cmd)` with no
   allowlist, because the catalog had no exec verbs. It has two now: `run-shell`
   (`crates/zz-daemon/src/daemon.rs:4514`) and `send-keys` into a shell pane. A page an agent
   wrote, or one a prompt injection wrote through the agent, would get a shell. Pages get no
   general bridge.
3. **Reload is the main loop.** It deferred file watching. An agent writes, looks, rewrites; a
   page that needs a manual reload after every write is a page agents stop using.

The installed-panes registry and the general bridge are out of this plan (see Deferred). What
stays from July: an artifact is a browser pane, not a new pane kind.

## Goals

* `zz html <path>` opens a page an agent wrote, on local and ssh hosts alike.
* The pane reloads when a file the page loaded changes.
* A page reaches its own files and the network, and nothing in zz unless the opener asks for
  replies.
* With `--reply`, the page can send text to the pane that opened it, so an agent can ask a
  question as a form.
* `svg` and `html` blocks in the Agent pane preview inline and cost an image while scrolling.
* No new pane kind, no new escape sequence, one small protocol addition.

## Non-goals (v1)

* User-installed panes, a manifest, sharing or packaging.
* Any verb from page JS other than the reply.
* Web and iOS clients (see Other clients).
* Pushing data into a live page without a reload.

# Decision: a browser pane on a zz:// URL

An artifact is `PaneKind::Browser` with a tab on `zz://<token>/<file>`. `BrowserDescriptor`
(`crates/zz-protocol/src/snapshot.rs:205`) holds tabs as plain URL strings, so splits, tabs,
titles from `<title>`, and reattach work as they do for any page. Floats come free:
`new-pane --kind browser` already makes one, and every non-modal float draws as a
[PiP](/designs/pip-panes.md).

Rejected: an HTTP server on the daemon's loopback, reached through browser egress. Egress is a
setting and is off on Windows, and every daemon would open a port.

# The verb

```
zz html [-t %N | --float] [--reply] [--] <path | ->
```

* `<path>` resolves against the caller's working directory on the daemon's host. A file serves
  that file alone; a directory serves its `index.html` and the files under it. `-` reads the page
  from stdin, as `agent-send` does; that page lives in memory and never reloads.
* By default it splits the calling pane (`split-window -h --kind browser`), like the Agent pane's
  output viewer. `-t %N` points that browser pane at the page instead (`set-browser-url`, keeping
  that pane's profile). `--float` opens it with `new-pane --kind browser`, which draws as a PiP.
* The opener is the calling pane, from `ZZ_PANE` (`ExecRequest.origin`,
  `crates/zz-daemon-client/src/client.rs:362`). `--reply` without a calling pane fails.
* It prints the URL and the pane: `zz://3f9c…/report.html %12`.
* New panes open in a browser profile named `artifacts`, so pages share no cookies or storage with
  the user's browsing.

It is a daemon-level verb wired like `capture-browser` (`DaemonCommandDispatch`,
`crates/zz-daemon/src/daemon.rs:4477`), with a `CommandSpec` in
`crates/zz-protocol/src/catalog.rs` and a line in `WORKSPACE_TOOLS_BROWSER`
(`crates/zz-daemon/src/daemon.rs:52545`) so agents find it through `zz tools browser`.

# Serving

**The table.** The daemon keeps the artifacts in memory beside the mux state:

```rust
struct Artifact {
    root: ArtifactRoot,                  // File(PathBuf) | Dir(PathBuf) | Inline(Bytes)
    opener: Option<PaneId>,
    reply: bool,
    served: HashMap<PathBuf, FileStamp>, // what the page loaded, for reload
}
artifacts: HashMap<ArtifactToken, Artifact>
```

The token is 128 random bits in lowercase hex. It is the URL's host, so each artifact is its own
origin and pages can't read each other's storage, and it is the only name an artifact has, so a
page can't guess another's. Opening the same path again reuses its token. The table dies with the
daemon, like the rest of mux state; a pane left on a dead token shows the not-found page.

**Paths.** A request path is percent-decoded, joined to the root, resolved with `realpath`, and
must still sit under the root. Names starting with `.` are refused at any depth, so
`zz html ~/notes.html` serves only that file and `zz html ~` still can't hand `.ssh/` to a page.
The MIME type comes from the extension.

**The wire.** It follows the pasted-image fetch (`crates/zz-protocol/src/message.rs:3895`), with
a request id like `PathListRequest`:

```rust
// client -> daemon
FetchArtifact { request_id: u64, token: ArtifactToken, path: String }
ArtifactReply { token: ArtifactToken, text: String }
// daemon -> client
ArtifactBegin { request_id: u64, mime: String, total_bytes: u64 }
ArtifactChunk { request_id: u64, bytes: Vec<u8> }      // up to 1 MiB each
ArtifactUnavailable { request_id: u64, reason: String }
```

They are appended to the message enums, never reordered, and `PROTOCOL_VERSION` goes 109 → 110
(`crates/zz-protocol/src/message.rs:28`) with an entry in the [wire protocol](/protocol/wire-protocol.md)
history. Only interactive clients attached to a session may fetch, the check pasted images make.
Files over 16 MiB are refused.

**Chromium.** No custom scheme is registered today; `zz://browser/element-picker.js`
(`crates/zz-browser/src/cef_runtime.rs:75`) only names the picker script in DevTools.
`RuntimeApp` (`cef_runtime.rs:3738`) gains `on_register_custom_schemes`, registering `zz` as
standard and secure and not CORS-enabled; the single-binary re-exec runs it in every process. A
scheme handler factory goes on each profile's `RequestContext` as `start_profile_context_at`
(`cef_runtime.rs:788`) makes it. Its resource handler passes the request to the GUI, which sends
`FetchArtifact` on the attached connection (`MuxClient::attached_connection`,
`crates/zz/src/mux/client.rs:1323`) and streams the chunks back. Browser panes only show for the
attached host, so that is always the daemon that holds the token. http(s) pages can't fetch or
frame `zz://` URLs because the scheme is not CORS-enabled.

The URL parser (`crates/zz-client/src/url_input.rs:127`) adds `zz` beside http, https and file,
so `set-browser-url` and reattach take these URLs.

# Reload

The daemon records each file it serves for an artifact with its size and mtime. Every 300 ms it
stats those files for artifacts some pane still shows; when one changes and then holds still for
100 ms, it sends `EventPayload::BrowserCommand { pane, command: BrowserCommand::Reload }` to each
pane whose active tab is on that token. `Reload` already exists
(`crates/zz-protocol/src/message.rs:2370`), though no daemon verb sends it today. Chromium
restores the scroll position on reload.

Polling, not a watcher: no crate in the workspace watches files, the set is the handful of paths
the page loaded, and agents often write by rename, which a stat sees and a watch on the old file
misses.

# Trust

A page starts with:

* its own files under its root, dotfiles excluded;
* the network, through the pane's profile and, on an ssh host, that host's egress like any browser
  pane. Artifacts pull chart libraries from CDNs, so v1 does not block it;
* no zz verbs.

The only bridge is the reply. Like the element picker, the GUI registers a message router
(`__zzArtifactQuery`, beside `element_picker_router_config` at `cef_runtime.rs:79`), and
`on_context_created` (`cef_runtime.rs:3827`) defines `window.zz.reply(text)` on main frames whose
URL is `zz://`. The router answers only the main frame, and only while its URL is still on that
token, by sending `ArtifactReply`. The daemon is the gate: it drops the reply unless the artifact
has `reply` set and its opener still exists. On a page opened without `--reply`, `zz.reply`
rejects and nothing reaches the mux.

# Replies

The daemon delivers an accepted reply as `zz agent-send --submit -t <opener>` would
(`crates/zz-daemon/src/daemon.rs:16738`): to an Agent pane, to Claude Code in a terminal through
its peer socket, or to Codex through its queue. The text goes in under a header naming the page
(`from artifact report.html`), so the agent can tell a click from the user's typing, and is capped
at `MAX_AGENT_SEND_BYTES`. One reply per second per artifact; extra ones in that second reject.

That gives the agent a form to ask with:

```html
<button onclick="zz.reply('picked: option B')">B</button>
```

# Inline previews in the Agent pane

Small drawings belong in the answer, not in a pane. The rule is that a preview costs an image
while you scroll: Chromium never sits in the timeline unless you click a block.

**svg blocks render natively.** Mermaid already does this. A mermaid block becomes SVG,
`svg_renderer.render_single_frame` draws it into a `RenderImage` on a background thread after a
250 ms debounce (`crates/zz-ui/src/agent.rs:3585`), and `MermaidImageCache` keeps 16 images keyed
by source, theme and font (`agent.rs:3520`). An `svg` block takes the same path minus the mermaid
step, as a second markdown plugin beside `MermaidPlugin`: no Chromium, and the SVG renderer runs no
scripts and fetches nothing. svg blocks render by default, with a Code toggle beside Copy.

**html blocks show code first.** Coding agents write HTML to be read as code, so an `html` block
keeps its code view and gains Preview and Open beside Copy (`agent_code_block_chrome`,
`agent.rs:2797`). The toggle is per block and lasts the session. Open runs `html -` with the
block's text, the Agent pane as opener and no reply, which makes it a real artifact pane.

**One hidden browser makes the pictures.** Preview hands the block to a single CEF browser the GUI
owns with no pane, sized to the timeline's content width at the window's scale. Blocks go through
it one at a time:

1. The block is served from the client's memory under a token the client mints, through the same
   `zz://<token>/` handler and rules (the handler looks in the client's table before asking the
   daemon). The text is already in the client, so nothing crosses the wire. The browser uses the
   `artifacts` profile, and `zz.reply` rejects, since a preview has no opener.
2. It loads the page, waits for load and a settled frame, reads the document's height, resizes to
   it (capped at 800 px; a taller page shows its top with a fade into Open), and copies the next
   painted frame into a `RenderImage`.
3. A block that takes over 3 s shows "Preview timed out" with Open. Animated pages show their
   first settled frame.

The browser starts on the first Preview and closes after 60 s with nothing queued, so it is one
renderer process while previews render and none otherwise. Previews render only when asked, so a
slow page costs a wait, never scrolling.

**The cache.** Images are keyed by the block's source, its width rounded to 32 px, light or dark,
and the scale, so a resize or a mode switch renders again after the debounce. The budget is decoded
bytes, 64 MiB as for terminal images (`clients/app/src/terminal_images.rs:5`), least recently drawn
first out. A 640×400 block at 2x is about 4 MB, so about sixteen stay resident. The timeline is a
gpui `list` that lays out only visible rows, so rows off screen cost nothing, and an evicted block
renders again when it comes back.

**Click to go live.** A preview is a still picture. Clicking it makes that block live in place: it
gets its own browser at the block's size, drawn as browser panes are (on macOS through
`Window::paint_underlay`, which clips to the timeline's scroll area,
`crates/zz-gpui/src/window.rs:5747`), and it takes keys and the mouse. One block is live at a time;
Escape, clicking another block or scrolling it out of view turns it back into a picture. While live
it costs what a small browser pane costs.

Rejected: a live browser per block (one renderer each, all drawing while you scroll), and a native
HTML engine in GPUI (no JS, so Chart.js and D3 pages, most of what agents draw, would not run).

To measure before shipping: time from Preview to picture for a typical block, and the hidden
renderer's memory.

# Other clients

* **iOS** shows browser panes in `WKWebView` (`clients/app/src/ios_browser.rs`). A
  `WKURLSchemeHandler` for `zz` would do what the CEF handler does over the iOS transport, and the
  navigation allowlist (`ios_browser.rs:195`) adds `zz`. Previews use an offscreen web view and
  `takeSnapshotWithConfiguration`, which the phone pill's glass already calls
  (`ios_browser.rs:754`). Next after desktop.
* **Web** shows a placeholder for every browser pane (`clients/app/src/app.rs:1834`). The web
  client already runs in a browser, so artifacts and previews can be live sandboxed
  `<iframe srcdoc>` frames without same-origin, placed over the canvas the way the accessibility
  mirror places its nodes (`crates/zz-gpui-platform/src/web/a11y.rs`); no pictures needed. Single
  files only, since relative assets have no origin to resolve against. Later.

Until then those clients show an artifact like any browser pane they can't show, and html blocks
as code.

# Error handling

| Failure | Behavior |
|---|---|
| path missing at `zz html` | the verb fails naming the path |
| file gone later, outside the root, or a dotfile | not-found page |
| unknown token (daemon restarted) | not-found page that says to run `zz html` again |
| file over 16 MiB | error page with the size |
| reply without `--reply`, opener gone, or too soon | `zz.reply` rejects with the reason |
| svg block the renderer can't parse | the code view with the error, as mermaid shows |
| preview over 3 s | "Preview timed out" with Open |
| daemon without the verb | `unknown command: html`, as for any new verb |
| `zz://` in a GUI without the handler | `UnsupportedScheme`, as today |

# Testing

* Unit: path resolution over `..`, encoded slashes, symlinks out of the root and dotfiles; token
  reuse per path; `url_input` takes `zz://<token>/x.html`; the `html` argument parser; preview
  cache keys and the byte budget.
* Daemon: `FetchArtifact` chunks and `ArtifactUnavailable`; a changed served file sends one
  `Reload` to each pane showing the token after the debounce; `ArtifactReply` dropped without
  `--reply` and delivered to the opener with it.
* Agent pane: an `svg` block renders through the plugin and its Code toggle shows the source; an
  `html` block starts as code.
* Headless: `zz_cli` against a throwaway daemon opens a page, rewrites it, and sees the reload
  event (recipe in [running zz](/playbooks/running-zz.md)).
* Desktop: `zz_browser_fixture` loads a `zz://` page that fetches a sibling file and calls
  `zz.reply`, and renders an html block to a picture through the hidden browser.

# Deferred

* **A design page**, the Figma-like step. A page zz ships, served on an artifact's own origin so it
  can reach the framed page's DOM, frames the agent's page, draws selection handles and lets you
  change text, colors and spacing. Edits go back to the agent through `--reply` as a selector plus
  the CSS or text change, and the agent edits the file, so the page writes no files. The
  [element picker](/browser/element-picker.md) is its selection half already. Needs artifacts and
  replies first.
* **Installed panes** (`~/.config/zz/panes`, a picker entry) and a **general bridge**. If they
  come back, page verbs need an allowlist: with `run-shell` and `send-keys`, the full catalog is a
  shell.
* **Pushing data** into a live page (`pane-send`), for dashboards that shouldn't reload.
* **MCP Apps**: showing the `ui://` resources a tool returns. Needs the drivers to pass the tool's
  `_meta` through; unverified.
* Hiding the URL bar on `zz://` pages.
* iOS and web, as above.
