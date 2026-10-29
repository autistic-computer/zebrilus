---
type: Design Plan
title: Artifacts . agent-written HTML in browser panes and inline
description: "Proposed plan for showing HTML an agent writes. zz html <path> registers the page with the daemon that ran the caller, browser panes load it from a zz://<token>/ scheme whose bytes the daemon sends over the client's existing connection (so it works on ssh hosts), the daemon reloads the pane when a file the page loaded changes, pages get no bridge by default, and --reply grants one call that sends text to the pane that opened the page. In the Agent pane, svg blocks render inline through the mermaid path, and html blocks show code with a Preview toggle that one hidden browser renders as cached images; a click makes that block a live page in place, and Open opens it as a pane. A block opened with html preview shows the picture first and wraps a fragment in a page with the theme colors, which is also what Codex's visualize references become. Replaces the 2026-07-28 userpanes plan."
status: Built on desktop (2026-10-19; zz html, zz:// serving, reload, zz.reply, svg blocks, html Open, Preview and live blocks, html preview blocks and Codex visualize references; iOS and web not started; the Agent pane that hosted inline previews was removed 2026-10-23)
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
timestamp: 2026-10-23T00:00:00-03:00
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
conversation: `svg` and `html` blocks in the Agent pane preview inline (see Inline previews; the
Agent pane is gone since 2026-10-23 and the agent lens timeline keeps this).

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
zz html [--float] [--reply] [-t %N] [--] <path | ->
```

* `<path>` resolves against the caller's working directory on the daemon's host. A file serves
  that file alone; a directory serves its `index.html` and the files under it. `-` reads the page
  from stdin, as `agent-send` does; that page lives in memory and never reloads.
* By default it splits the calling pane (`split-window -d -h --kind browser`), like the Agent
  pane's output viewer, and leaves the focus where it was. `-t %N` shows the page in that pane when
  it is a browser on the `artifacts` profile (`set-browser-url`) and otherwise opens it beside that
  pane, as `split-window -t` would, so a page never loads into a profile with the user's logins. `--float` opens it with `new-pane --kind browser`, which draws as a
  PiP; with `-t` it floats over that pane.
* The opener is the calling pane: the client record's `origin`, which the CLI fills from
  `ZZ_PANE` (`ExecRequest.origin`, `crates/zz-daemon-client/src/client.rs:362`). A relative path
  resolves against the client's working directory through `buffer_file_path`, because daemon
  verbs run before `ExecutionContext` gets one. `--reply` without a calling pane fails.
* It prints the URL and the pane: `zz://3f9c…/report.html %12`.
* New panes open in a browser profile named `artifacts`, so pages share no cookies or storage with
  the user's browsing.

It is a daemon-level verb wired like `capture-browser` (`DaemonCommandDispatch`,
`crates/zz-daemon/src/daemon.rs`), with a `CommandSpec` in `crates/zz-protocol/src/catalog.rs`,
a `native-command:html` item in `compat/tmux-gaps.json`, and a section in `WORKSPACE_TOOLS_BROWSER`
so agents find it through `zz tools browser`. The handler and the table live in
`crates/zz-daemon/src/daemon/artifacts.rs`.

# Serving

**The table.** The daemon keeps the artifacts in memory beside the mux state:

```rust
struct Artifact {
    root: ArtifactRoot,                  // File(PathBuf) | Dir(PathBuf) | Inline(Bytes)
    opener: Option<PaneId>,
    reply: bool,
    served: HashMap<PathBuf, Stamp>, // what the page loaded, for reload
    replied: Option<Instant>,
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
ArtifactBegin { request_id: u64, mime: String, total_bytes: u32, reply: bool }
ArtifactChunk { request_id: u64, bytes: Vec<u8> }      // up to 1 MiB each
ArtifactUnavailable { request_id: u64, reason: String }
```

They are appended to the message enums, never reordered, and `PROTOCOL_VERSION` goes 109 → 110
(`crates/zz-protocol/src/message.rs:28`) with an entry in the [wire protocol](/protocol/wire-protocol.md)
history. Only interactive clients may fetch; the token is the capability. Files over 16 MiB
(`MAX_ARTIFACT_BYTES`) are refused, and so is anything that is not a regular file: `zz html`
refuses a FIFO or device, and the daemon opens each file non-blocking and checks the open handle,
so a file swapped for a pipe later can't stall it. Reads and sends run on a `zz-artifact-fetch`
thread, never the event loop, and each 1 MiB chunk waits until that client's queue is under 4 MiB.
The client only sends what the daemon decodes: `parse_artifact_url` takes a 32-hex token and a path
of at most `MAX_ARTIFACT_PATH_BYTES`, `zz.reply` refuses text over `MAX_ARTIFACT_REPLY_BYTES`, and
the daemon trims `ArtifactUnavailable` reasons to the path cap. An oversized field would otherwise
fail to decode and drop the whole client connection. `reply` tells the GUI whether the page may use `zz.reply`, so it
can reject one without a round trip.

**Chromium.** Before this, no custom scheme was registered; `zz://browser/element-picker.js` only
names the picker script in DevTools. `RuntimeApp` (`crates/zz-browser/src/cef_runtime.rs`) has
`on_register_custom_schemes`, registering `zz` as standard, secure and fetch-enabled but not
CORS-enabled; the single-binary re-exec runs it in every process. `start_profile_context_at`
puts an `ArtifactSchemeFactory` on the `artifacts` profile's `RequestContext` only
(`zz_browser::ARTIFACT_PROFILE`), so a site in an ordinary pane can't load or embed `zz://` at all. Its `ArtifactResourceHandler`
defers `open`, hands the token and decoded path to `zz_browser::artifact_requests()`
(`crates/zz-browser/src/artifact.rs`), and when the answer comes resumes CEF with a task posted to
the IO thread, as the cef crate's own `ResourceManager` does. The workspace view drains that channel
into `MuxClient::fetch_artifact`, which sends `FetchArtifact` on the attached connection and
assembles the chunks. Browser panes only show for the attached host, so that is always the daemon
that holds the token; a host switch fails the fetches in flight. Responses carry
`Cache-Control: no-store`. http(s) pages and other artifacts can't fetch `zz://` URLs because the
scheme is not CORS-enabled.

The URL parser (`crates/zz-client/src/url_input.rs`) adds `zz` beside http, https and file,
so `set-browser-url` and reattach take these URLs.

# Reload

The daemon records each file it serves for an artifact with its size and mtime. Every 250 ms a
`zz-artifacts` thread stats those files for artifacts some pane still shows; when one changes and
reads the same on the next scan, it posts a callback to the daemon thread that sends `EventPayload::BrowserCommand { pane, command: BrowserCommand::Reload }` to each
pane whose active tab is on that token. `Reload` already existed
(`crates/zz-protocol/src/message.rs:2370`), though no daemon verb sent it before. The thread exits
when no shown artifact has served files, and the next fetch starts it again.

Polling, not a watcher: no crate in the workspace watches files, the set is the handful of paths
the page loaded, and agents often write by rename, which a stat sees and a watch on the old file
misses.

# Trust

A page starts with:

* its own files under its root, dotfiles excluded, checked on the resolved path so a link to one is
  refused too; `dir/` serves `dir/index.html`;
* the network, through the pane's profile and, on an ssh host, that host's egress like any browser
  pane. Artifacts pull chart libraries from CDNs, so v1 does not block it;
* no zz verbs.

The only bridge is the reply, and it rides the element picker's message router rather than a
second one. `ArtifactReplyHandler` sits on that router before the picker's handler and takes only
queries that start with `page.reply:`. `on_context_created` runs a small script on main frames whose
URL is `zz://` that defines `window.zz.reply(text)` over `__zzElementPickerQuery`. The scheme and
the `zz` global come from the `RuntimeConfig` zz hands zz-browser at bootstrap
(`crates/zz/src/browser/environment.rs`). The handler
takes the token from the frame's own URL, never from the query, and answers only the main frame,
so an https page that calls the query function gets nothing. It rejects unless the page loaded with
`reply` set, then queues an `ArtifactReply` for the workspace view to send. The daemon is the gate
again: it drops the reply unless the artifact has `reply` set and its opener still exists
(`Artifacts::take_reply`).

# Replies

The daemon delivers an accepted reply by running `agent-send --submit -t <opener>`: to an Agent
pane, to Claude Code in a terminal through its peer socket, or to Codex through its queue. The text
goes in under a header naming the page (`From report.html:`), so the agent can tell a click from
the user's typing, and is capped at 64 KiB (`MAX_ARTIFACT_REPLY_BYTES`). One reply per second per
artifact; extra ones in that second are dropped. Replies keep the caller's client identity and
cannot submit from a read-only attachment.

That gives the agent a form to ask with:

```html
<button onclick="zz.reply('picked: option B')">B</button>
```

# Inline previews in the Agent pane

The Agent pane was removed on 2026-10-23 ([agent lens](/designs/agent-lens.md)). The preview code
lives in the timeline (`crates/zz-ui/src/agent.rs`, `crates/zz/src/browser/inline.rs`), which the
lens over a Claude Code or Codex terminal keeps, so "the Agent pane" below now means that timeline.
Terminal agents get the startup context without the html paragraph (`zz tools --context` calls
`workspace_context(.., false)`), so the CLIs are not taught the form, and a `zz.reply` would land in
the lens composer, which pastes into the TUI.

Small drawings belong in the answer, not in a pane. The rule is that a preview costs an image
while you scroll: Chromium never sits in the timeline unless you click a block.

**svg blocks render natively.** Mermaid already does this. A mermaid block becomes SVG,
`svg_renderer.render_single_frame` draws it into a `RenderImage` on a background thread after a
250 ms debounce (`crates/zz-ui/src/agent.rs:3585`), and `MermaidImageCache` keeps 16 images keyed
by source, theme and font (`agent.rs:3520`). An `svg` block takes the same path minus the mermaid
step, as a second markdown plugin beside `MermaidPlugin`: no Chromium, and the SVG renderer runs no
scripts and fetches nothing. svg blocks render by default and keep mermaid's controls: Copy source
and Open full diagram. Built: `SvgPlugin`, with both plugins going through `parse_figure` and
`render_figure` and sharing `FigureImageCache`.

**html blocks show code first.** Coding agents write HTML to be read as code, so an `html` block
keeps its code view and gains Preview and Open beside Copy (`agent_code_block_chrome`,
`agent.rs:2797`). The toggle is per block and lasts the session. Open runs `html - -- <text>`,
which opens the page beside the active pane with no reply. Built: Open, through
`AgentHtmlOpener`, a global the desktop sets from its workspace view; web and iOS leave it unset,
so their blocks keep only Copy. Preview is built the same way: `AgentHtmlPreviewer` is the global,
the Preview button (`AppWindow`, then `Braces` for Show code) flips the ID for that message and block occurrence in
`HtmlPreviewToggles`, and the kit's `TextView::code_block_body` hook lets `agent_code_block_body`
draw the picture in place of the code.

**One hidden browser makes the pictures.** Preview hands the block to a single CEF browser the GUI
owns with no pane (`crates/zz/src/browser/controller/preview.rs`, a `Previews` field on
`BrowserController`, not a fake pane, so nothing pane-shaped sees it). It is created without
shared textures, so its frames are plain BGRA that become a `RenderImage` with no copy back from
the GPU. Blocks go through it one at a time:

1. The block is served from the client's memory: `zz_browser::host_local_page` mints a token, and
   the `zz://` handler answers from that table before asking the daemon. Nothing crosses the wire.
   The browser uses the `artifacts` profile on a white canvas (`set_background_color` around its
   creation, since pane browsers default to the dark backdrop), and `zz.reply` rejects.
2. It loads the page in a viewport the block's width wide and 801 px tall, then reads the content
   height through DevTools `Runtime.evaluate` (`BrowserSession::evaluate_number`): the bottom of
   `<body>`, or of its contents in quirks mode, where the body stretches to the viewport.
   `Page.getLayoutMetrics` can't be used, since the root never reports less than the viewport.
3. It resizes to that height, clamped to 8–800 px. 801 is never a result, so the resize always
   paints a fresh frame. It keeps the newest frame of the new size and finishes once no frame has
   come for 150 ms, or 1.5 s after the resize for pages that never stop, so Chart.js and other
   animated charts show their finished state. Taller pages show their top 800 px.
4. A block that takes over 3 s shows "Preview timed out".

The browser starts on the first Preview and closes after 60 s with nothing queued. While it works
the controller keeps the pump at 16 ms and sends it BeginFrames each turn; idle, it costs nothing.
Previews render only when asked, so a slow page costs a wait, never scrolling.
Each preview job owns its hosted page and removes it when the job ends or leaves the queue,
including when its caller cancels before polling the result. An active job keeps its page until
the job finishes or fails.

**The cache.** Images are keyed by the block's source, its width rounded to 32 px, light or dark,
and the scale, so a resize or a mode switch renders again after the debounce. The budget is decoded
bytes, 64 MiB as for terminal images (`clients/app/src/terminal_images.rs:5`), least recently drawn
first out. A 640×400 block at 2x is about 4 MB, so about sixteen stay resident. The timeline is a
ala `list` that lays out only visible rows, so rows off screen cost nothing, and an evicted block
renders again when it comes back.

**Click to go live.** A preview is a still picture; clicking it makes that block live in place, and
Open still opens it as a pane. Built in `crates/zz/src/browser/inline.rs` (`InlineBrowser`, one
for the window, behind the `AgentHtmlLive` global): a click asks for a controller session under a
synthetic pane id the mux never allocates (`PaneId(u64::MAX - 1)`, a fresh `TabId` each time,
since a closed session stays in the controller's map until its `Closed` event), readback only and
on a white canvas (`BrowserSessionRequest::readback_only` and `with_background`), sized to the
picture so the page lays out the same. `BrowserElement` is generic over `BrowserSurfaceHost`, so
the inline host and `BrowserView` paint and take IME input the same way; readback frames go through
`paint_image`, which clips to the timeline's scroll area (the macOS underlay layer would not). The
host takes its own focus handle; mouse, wheel, keys, text and the edit actions go straight to the
controller, since the daemon's `BrowserSurfaceKey` path rejects a pane it doesn't know. Escape or
losing focus (clicking elsewhere, switching panes) closes the session and the picture returns.

Two places had to learn about it: the workspace's title subscriber now ignores panes it doesn't
own, or a page's `<title>` would run `select-pane -T` on the fake pane, and `focus_active_pane`
leaves the focus with the live page when it sits inside the active pane, instead of handing it back
to the composer. Measured on macOS: first live frame 0.22 s after the click (480×200 CSS at 2x), on
top of the preview it replaces. While live it costs what a small readback browser pane costs; at
most one block is live.

**Blocks meant as pictures.** An agent that wants to show something rather than hand over code
opens the block with ```` ```html preview ````. The kit keeps the words after the language as
`CodeBlock::meta`, and such a block shows the picture first; Show code flips it, through the same
`HtmlPreviewToggles` entry. A fragment in one of these blocks is wrapped in a page before it renders
(`visual_page_head`): the theme's colors as CSS variables, a small stylesheet, and a
`window.openai` object. The variable and class names are the ones Codex's visualize skill writes
against (`--background`, `--foreground`, `--card`, `--muted-foreground`, `--viz-series-1`,
`.card`, `.btn-primary`, `.viz-grid` and the like), since those are names models already use. A
whole page (`<!doctype` or `<html>` first) renders as it is, and a plain `html` block is never
wrapped, because its code may be meant for another page. Agent panes learn the form from one
paragraph the native Claude and Codex drivers add to the startup context
(`workspace_context(.., true)`).

**Codex's visualize references.** Codex's bundled visualize skill writes the fragment to a file and
puts `\u{e200}visualize\u{e202}{"path": ...}\u{e201}` in its reply, which only Codex's own app
draws. The pane host rewrites it before anything else sees the update
(`crates/zz-daemon/src/agent/visualize.rs`, run on each runtime event in `PanePump`): the
reference becomes an `html preview` block holding the file, read by the daemon that ran the agent,
so it works on ssh hosts and the transcript keeps the page after the file is gone. A reference
split across chunks is held until it closes, and released as it was if another event comes first.
Only absolute `.html` paths up to 1 MiB are read; anything else becomes a one-line quote saying
why. `widgetState` is not kept.

**Replies from a live block.** `window.openai.sendFollowUpMessage({prompt})` calls `zz.reply`, and
any live block may use it: the text lands in the composer of the Agent pane that drew the block,
as `agent-send -t %N` drafts it, and is not sent. Codex's own app asks before sending a follow-up;
here the composer is that question, so a page can never start a turn by itself, and plain `html`
blocks follow the same rule as `html preview` ones (nothing to choose between when the user sends
it either way). The pane comes from the timeline: the desktop `AgentView` makes its store with
`AgentTimelineStore::owned_by(pane)`, the markdown view hands the owner to the body hook, and
`AgentHtmlLive.activate` gets it in `HtmlLiveBlock`. The live page is hosted with reply rights
(`host_local_page(html, true)`; pictures stay without), and the workspace sends replies whose token
is a local page to `InlineBrowser::take_reply`, which answers only for the live page, at most once a
second, and drops the rest; other tokens still go to the daemon. An html block inside a rendered
`markdown` block has no owner and can't reply.

Rejected: a live browser per block (one renderer each, all drawing while you scroll), and a native
HTML engine in ala (no JS, so Chart.js and D3 pages, most of what agents draw, would not run).

Measured 2026-10-19 on macOS (M-series, 2x, release-like dev bundle, the app's own call path):

| Page | Preview to picture |
|---|---|
| first preview after launch, CEF not started yet | about 0.4 s (0.54 s in another run) |
| a static chart of styled divs, warm | 0.19 s |
| a tall page (capped at 800 px) | 0.29 s |
| a one-line page | 0.18 s |
| Chart.js from a CDN with its 1 s animation | 1.35–1.85 s |

Warm static pages are dominated by the 150 ms quiet wait. With no browser pane open, the first
Preview starts CEF: about 90–130 MB per renderer process, 140–150 MB for the GPU process and
70–90 MB each for the network and storage helpers (RSS, which counts shared pages more than once).
Closing the preview browser after 60 s drops its renderer; the runtime's other processes stay, as
they do after closing a browser pane. Each preview is its own origin, so Chromium swaps renderer
processes between previews; one shared origin for previews would save that swap if it ever shows
up in the numbers.

# Other clients

* **iOS** shows browser panes in `WKWebView` (`clients/app/src/ios_browser.rs`). A
  `WKURLSchemeHandler` for `zz` would do what the CEF handler does over the iOS transport, and the
  navigation allowlist (`ios_browser.rs:195`) adds `zz`. Previews use an offscreen web view and
  `takeSnapshotWithConfiguration`, which the phone pill's glass already calls
  (`ios_browser.rs:754`). Next after desktop.
* **Web** shows a placeholder for every browser pane (`clients/app/src/app.rs:1834`). The web
  client already runs in a browser, so artifacts and previews can be live sandboxed
  `<iframe srcdoc>` frames without same-origin, placed over the canvas the way the accessibility
  mirror places its nodes (`libs/ala/crates/ala-platform/src/web/a11y.rs`); no pictures needed. Single
  files only, since relative assets have no origin to resolve against. Later.

Until then those clients show an artifact like any browser pane they can't show, and html blocks
as code.

# Error handling

| Failure | Behavior |
|---|---|
| path missing at `zz html` | the verb fails naming the path |
| path is a FIFO, device or `<(cmd)` | the verb fails and suggests `zz html -` |
| file gone later, outside the root, or a dotfile | not-found page |
| unknown token (daemon restarted) | not-found page that says to run `zz html` again |
| file over 16 MiB | error page with the size |
| reply without `--reply`, opener gone, or too soon | `zz.reply` rejects with the reason |
| svg block the renderer can't parse | the code view with the error, as mermaid shows |
| preview over 3 s | "Preview timed out" with Open |
| daemon without the verb | `unknown command: html`, as for any new verb |
| `zz://` in a GUI without the handler | `UnsupportedScheme`, as today |

# Testing

Built and covered:

* `daemon::artifacts` tests: path resolution over `..`, `\`, empty segments, symlinks out of the
  root and dotfiles; a file root serving only itself; token reuse per path; URLs and tokens; MIME
  types; the 16 MiB cap; the `html` argument parser and `html_reads_stdin`; `take_reply`'s flag,
  opener and one-second gap.
* `zz_browser::artifact` tests: URL parsing and percent-decoding, escaped error pages.
* Agent pane tests: `an_svg_block_renders_as_a_figure`,
  `an_html_block_opens_as_a_page_when_the_app_can_show_one`.
* The catalog, tools-text and compat-manifest tests that pin every verb, and the wire fixtures at
  v110.

Verified live on macOS (2026-10-19) with the worktree's zz Dev run apart from the shared one
(`HOME=<scratch> zz --socket /tmp/<name>.sock app`, CDP on a spare port): `zz html --reply DIR`
opened a split showing the page, which fetched its `data.json`; `.env` and `../etc/passwd` came
back 404; another token's URL failed to fetch; rewriting the page and then `data.json` each
reloaded the pane within a scan; `zz.reply` resolved on a `--reply` page; `--float`, `-` from
stdin and `-t` on a browser pane worked; a missing file, `--reply` outside a pane and `-t` on a
terminal (before `-t` learned to open beside a pane) failed with their messages. On the final
build `-t` on a terminal opened the page beside it, `--float -t` floated over it, and the Open
button's call (`html - -- <text>`) opened its page. Not verified
live: a reply reaching an agent, since that submits a real turn, and the Open button in a real
Agent pane.

Live blocks: `a_previewed_html_block_goes_live_on_click` (zz-ui, with a stand-in live element). A
temporary probe activated `InlineBrowser` in the running app: first frame in 0.22 s, a white canvas,
the page's button answered a click over CDP, and the daemon logged nothing about the synthetic pane.
Not checked live: clicking and typing into a live block in an Agent pane, since driving the window
was not possible next to another session's zz Dev.

Replies from live blocks: `a_live_page_may_reply_until_dropped` (zz-browser),
`a_live_page_replies_to_its_agent_pane_at_most_once_a_second` (the reply gate in `inline.rs`), and
`a_previewed_html_block_goes_live_on_click` now checks the block's owner pane. Not checked live: a
click in a live block drafting into a real composer.

Preview blocks and Codex references: `a_preview_block_shows_as_a_themed_picture_without_a_toggle`
and `whole_pages_skip_the_visual_wrapper` (zz-ui); the `agent::visualize` tests (a reference
becomes a block with a longer fence than the file's backticks, a reference split across chunks,
an unclosed one released before another event, a missing file, other references left alone). The
Codex fragment that prompted this rendered correctly in both themes inside the wrapper. Not checked
live: a real Codex turn through the rewrite.

Previews: `an_html_block_previews_as_a_picture_when_toggled` (zz-ui, with a stand-in renderer),
`local_pages_answer_without_the_daemon_until_dropped`, `evaluated_numbers_come_from_the_result_value`
and the height and frame-size helpers in `controller::preview`. The hidden browser was checked with
a temporary probe in the app that sent six pages through `render_html_preview`, wrote the pictures
and logged the times above; it was removed afterwards. Not covered by a test: the cache's byte
budget, and a real click on Preview in a running app.

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
