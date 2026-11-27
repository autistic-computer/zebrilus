---
type: Design Plan
title: "HTML pages and inline chat previews"
description: "The daemon serves HTML files to browser panes. Desktop, iOS and web also render inline chat previews."
status: "Implemented on desktop, iOS and web with different capabilities. Android shows inline HTML as code."
resource: crates/zz-daemon/src/daemon/artifacts.rs
tags:
- browser
- cef
- panes
- scheme-handler
- agent-canvas
- agent-pane
- artifacts
- design-plan
timestamp: 2026-11-27T00:00:00-03:00
---

# Overview

`zz html <path>` opens a file written on the daemon's machine in a browser pane. Opening a directory serves its index and relative assets. Desktop and iOS clients fetch the bytes over their existing daemon connection, including when the opener runs on a remote machine. The pane reloads when a served file changes. An inline page can be supplied on standard input.

The daemon registers each page under its own `zz://<token>/` origin. Replies to the opening pane require `--reply`. A chat also supports inline HTML and SVG previews. The desktop, iOS and web implementations are described under [Other clients](#other-clients).

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
  `ZZ_PANE` (`ExecRequest.origin`, `exec_request` in `crates/zz-daemon-client/src/client/command.rs`). A relative path
  resolves against the client's working directory through `buffer_file_path`, because daemon
  verbs run before `ExecutionContext` gets one. `--reply` without a calling pane fails.
* A pane keeps one page pane. The pane a page opens beside (`-t`, else the opener, else the
  active pane) owns the pane that call created, and its next `zz html` shows the new page there
  instead of splitting again (`Artifacts::pages`). `-t` naming a page pane still wins.
* It prints the URL and the pane: `zz://3f9c…/report.html %12`.
* New panes open in a browser profile named `artifacts`, so pages share no cookies or storage with
  the user's browsing.

It is a daemon-level verb wired like `capture-browser` (`DaemonCommandDispatch`,
`crates/zz-daemon/src/daemon/command_routing.rs`), with a `CommandSpec` in `crates/zz-protocol/src/catalog.rs`,
a `native-command:html` item in `compat/tmux-gaps.json`, and a section in `WORKSPACE_TOOLS_BROWSER`
so agents find it through `zz tools browser`. The handler and the table live in
`crates/zz-daemon/src/daemon/artifacts.rs`.

# Serving

The daemon's `Artifacts` registry in `crates/zz-daemon/src/daemon/artifacts.rs` indexes tokens, roots and the open page pane per opener. Each entry keeps the file, directory or inline bytes, opener pane, reply permission, served paths and their last known stamps.

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
names the picker script in DevTools. `RuntimeApp` (`libs/ala-cef/src/cef_runtime.rs`) has
`on_register_custom_schemes`, registering `zz` as standard, secure and fetch-enabled but not
CORS-enabled; the single-binary re-exec runs it in every process. `start_profile_context_at`
puts an `ArtifactSchemeFactory` on the `artifacts` profile's `RequestContext` only
(`ala_cef::ARTIFACT_PROFILE`), so a site in an ordinary pane can't load or embed `zz://` at all. Its `ArtifactResourceHandler`
defers `open`, hands the token and decoded path to `ala_cef::artifact_requests()`
(`libs/ala-cef/src/artifact.rs`), and when the answer comes resumes CEF with a task posted to
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
URL is `zz://` that defines `window.zz.reply(text)` over `__alaCefElementPickerQuery`. The scheme and
the `zz` global come from the `RuntimeConfig` zz hands ala-cef at bootstrap
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

# Pages a chat keeps changing

Boarded in `design/mockups/artifacts/` (`final.html` carries the numbers). The page is a file the
agent keeps rewriting, the pane follows the file through the reload above, and the path is the
page's identity, so zz stores nothing about it.

* **Who opens the pane.** Any agent that runs `zz html`. Antigravity without being told: its ACP
  `tool_call` for a page it wants shown carries `rawInput.ArtifactMetadata.UserFacing: true` and
  the file's `Path`. The fan-out keeps that path on the tool call when it ends in `.html`, `.htm`
  or `.svg` and a turn is live (`user_facing_page`, `crates/zz-agent/src/fanout.rs`), and
  the agent inbox runs `html -t <chat> -- <path>` once the call completes
  (`Shared::show_agent_page`). A session being restored opens nothing.
* **A pane you closed stays closed.** `show_agent_page` does nothing when the chat's page pane is
  gone. A row click or the agent's own `zz html` opens one again, and from then on the agent's
  writes reach it.
* **The row.** `zz_client::chat_rows::turn_rows` keeps a `Unit::Page` outside a finished turn's
  fold for each page file the turn edited, in the order the files were first touched, each
  pointing at its last completed edit. zz-ui draws it as the kit's activity row with
  `ActivityEnd::Open` (`page_row`, `crates/zz-ui/src/agent/chat.rs`): Edited, the file name, the
  open mark. Step rows of page edits inside an opened run carry the same mark. A click runs
  `html -t <chat> -- <path>` through `ChatRefs::page`; a relative target resolves against the
  chat's folder (`ChatRefs::page_path`). A client that sets no `page` opener draws no row, which
  is every thin client today.
* **Context.** The agent context tells an agent to run `zz html PATH` once for a page it will keep
  changing. Grok gets that context as `_meta.rules` on `session/new` (`Dialect::rules`). Cursor's
  ACP server documents no way to pass instructions, so it gets none.
* **Codex.** Its visualize references stay inline pictures: the skill writes fragments meant to
  sit in the reply.

# Inline previews in the Agent pane

The previews live in the agent chat (`crates/zz-ui/src/agent.rs`, `crates/zz/src/browser/inline.rs`),
which an agent pane draws since 2026-11-01 ([agent panes](/concepts/agent-panes.md)), so "the Agent
pane" below means that chat. Claude Code or Codex running in a terminal gets the startup context
without the html paragraph (`zz tools --context` calls `workspace_context(.., false)`), so the CLIs
there are not taught the form.

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

1. The block is served from the client's memory: `ala_cef::host_local_page` mints a token, and
   the `zz://` handler answers from that table before asking the daemon. Nothing crosses the wire.
   The browser uses the `artifacts` profile on a clear canvas (`set_background_color` around its
   creation, since pane browsers default to the dark backdrop), and `zz.reply` rejects. CEF hands
   back premultiplied color, which `straighten_alpha` undoes before the frame becomes a picture, so
   a visual's washes keep their strength over the chat. A plain `html` block's picture and a page
   frame's get a white backing in the chat instead.
2. It loads the page in a viewport the block's width wide and 801 px tall, then reads the content
   height through DevTools `Runtime.evaluate` (`BrowserSession::evaluate_number`): the bottom of
   `<body>`, or of its contents in quirks mode, where the body stretches to the viewport.
   `Page.getLayoutMetrics` can't be used, since the root never reports less than the viewport.
3. It resizes to that height, clamped to 8–800 px. 801 is never a result, so the resize always
   paints a fresh frame. It keeps the newest frame of the new size, and each time no frame has come
   for 150 ms it reads the height again: a page that grew (a chart drawn by a script that loads
   after the page) is resized and painted again, and one that did not is finished. It stops
   5 s after the first resize whatever the page does. Taller pages show their top 800 px.
4. A job asked for a fixed size (a page frame's 1280 × 720) skips the measuring: it loads one
   pixel taller, resizes to its height, and finishes at the first 150 ms quiet or after 1.5 s.
5. A block that has not started painting after 3 s shows "Preview timed out".

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
on a white canvas, or a clear one for a visual (`BrowserSessionRequest::readback_only` and
`with_background`; a clear page's frames are straightened like the pictures), sized to the
picture so the page lays out the same. A visual's first click also reaches the page: the block
passes where it was clicked (`HtmlLiveBlock::click`), and the inline browser presses there on the
first frame after the page has loaded. `BrowserElement` is generic over `BrowserSurfaceHost`, so
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

**Blocks meant as pictures are visuals.** An agent that wants to show something rather than hand
over code opens the block with ```` ```html preview ````. Where the app can make pictures
(`AgentHtmlPreviewer` is set, so the desktop), `VisualPlugin` (`crates/zz-ui/src/agent/visual.rs`)
takes such a block out of the code block path and draws it bare on the chat's ground: a
`CardLook::Figure` card with no fill, the picture, and three controls in its top right corner under
the pointer (Show code, which swaps the picture for the highlighted code through the same
`HtmlPreviewToggles` entry, Open as a page, Copy code). While it draws, and while the block is still
streaming, it is a wash with a spinner at the last picture's height, 148 px the first time.

A fragment is wrapped in a page before it renders (`visual_head`, built once per theme): the live
theme's tokens for the roles a visual uses (`penna::foundation::theme_css::roles_stylesheet` over
`button`, `button.quiet`, `field`, `badge` and `card.activity`, with their tones), penna's HTML kit
(`penna.css` and the button, card, badge, progress and segmented component sheets, about 70 KB with
the tokens), a small sheet of its own, and a script. The page's ground is clear. The names a
model writes are the ones Codex's visualize skill uses, since models already know them, and the
script dresses them as kit parts when the page loads and whenever classes change:

| A model writes | Drawn as |
|---|---|
| `.btn`, `.btn-primary` (or a `.btn` that is `.active`, `.is-selected` or `aria-pressed`) | `button`; prominent with the primary tone |
| `.nav-pills` with `.nav-link`s | penna's segmented control, small; the pill follows `.active` |
| `.form-range` | the native range input at the slider's sizes, filled to its value |
| `.form-control`, `.form-select` | `field` |
| `.card` | `card.activity` |
| `.viz-badge`, with `data-tone` | `badge` |
| `.progress` with a `.progress-bar` | penna's progress bar, small |
| `.viz-stat-value`, `.text-muted`, `.text-small`, `table`, `hr`, `.viz-grid`, `.viz-row` | the sheet's own rules on penna's inks, divider and type sizes |
| `--viz-series-1` to `6` | accent, success, warning, danger, then two hues between them |
| `--background`, `--foreground`, `--card`, `--border`, `--primary` and the like | clear, the chat's ink, the activity wash, the divider, the primary tone |

The script also rounds corners the way the kit's own script does and places the segmented pill. A
whole page (`<!doctype` or `<html>` first) in an `html preview` block renders as it is. Agent panes
learn the forms from three lines the native Claude and Codex drivers add to the startup context
(`AGENT_PANE_CONTEXT`, through `workspace_context(.., true)`): an `html preview` fragment for
something shown in the chat, with the names in the table above, the color variables, the note to
set no background, font or colors, and `zz.reply`; a whole document in a plain `html` block for a
page being built; `zz html PATH` for a page with files. That is about 1.3 KB on every agent
session, and a Claude session started with its own system prompt flag gets none of it. Checked on
2026-11-16 with a Haiku chat: asked to compare three algorithms it wrote an `html preview` fragment
using `table`, `.progress`, `.text-muted` and `--viz-series-N` with no colors of its own, and asked
for a landing page draft it wrote a whole document in a plain `html` block. Without the
plugin (a markdown view, a client with no previewer)
the block falls back to the code block with its picture.

**Whole documents are pages.** An `html` block that starts with `<!doctype` or `<html>` and is not
a `preview` block is the thing being built, so it keeps its own look: `PagePlugin` draws a
`CardLook::Code` card holding a 32 px strip (a globe, the page's `<title>` or Page, Show code, Open
as a page) over a picture of the page at 1280 × 720, scaled to the column on a white backing. A
click on the picture opens the page in a browser pane beside the chat (`html - -- <text>`); it is
never live in place. Show code opens the code in the chat's view pane. Fragments in plain `html`
blocks stay code blocks and cards with the Preview toggle described above, and the card plugin
leaves whole documents to the page plugin.

**Codex's visualize references.** Codex's bundled visualize skill writes the fragment to a file and
puts `\u{e200}visualize\u{e202}{"path": ...}\u{e201}` in its reply, which only Codex's own app
draws. The Codex driver rewrites it before anything else sees the update
(`crates/zz-agent/src/visualize.rs`, run at the top of the runtime's `emit`, ahead of the
chat machine whose events clients draw): the reference becomes an `html preview` block holding the
file, read by the daemon that ran the agent, so it works on ssh hosts and the transcript keeps the
page after the file is gone. Codex streams the reference a few characters per delta; a reference
split across chunks is held until it closes, and released as it was if another payload reaches
`emit` first.
Only absolute `.html` paths up to 1 MiB are read; anything else becomes a one-line quote saying
why. `widgetState` is not kept.

**Replies from a live block.** `window.openai.sendFollowUpMessage({prompt})` calls `zz.reply`, and
any live block may use it: the text lands in the composer of the Agent pane that drew the block
(`AgentController::draft`, the same pending-composer path reclaimed prompts take), and is not sent.
Codex's own app asks before sending a follow-up;
here the composer is that question, so a page can never start a turn by itself, and plain `html`
blocks follow the same rule as `html preview` ones (nothing to choose between when the user sends
it either way). The pane comes from the chat: each client passes its pane to `chat_surface`
(`ChatLook.pane`), the chat's markdown hands that owner to the body hook, and
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

* **iOS** (iPhone and iPad) draws the pictures since 2026-11-15: visuals and page frames show in
  its chats the way they do on the desktop. `clients/app/src/ios_preview.rs` sets
  `AgentHtmlPreviewer` to one hidden `WKWebView`, kept under the app's own view at the window's
  origin with a store that is never written to disk, and closed after 60 s with nothing queued.
  It loads the page with `loadHTMLString`, waits for `document.readyState` (3 s, then "Preview
  timed out"), reads the height with the desktop's script, resizes, and reads it again every
  150 ms until it stops changing or 5 s have passed; a page frame's 1280 × 720 skips the
  measuring. `takeSnapshotWithConfiguration` gives the picture, whose color is straightened like
  the desktop's. Three things it needed: the visual page carries a `width=device-width` viewport
  tag, or the web view lays it out 980 px wide and shrinks it; the scroll view's edge effects are
  hidden, or the blur iOS draws under the status bar bleeds into the top of every picture; and
  buttons inherit their color, or WebKit paints their text in its own blue. On the phone shell a
  visual is laid out narrower and drawn larger by 17 / 14, the ratio of the chat's touch text to
  the desktop's, so its type matches the reply around it. The corner controls are left out on
  the phone; on iPad they always show, since a finger cannot hover.

  iOS browser panes also load `zz://` pages since 2026-11-15, so `zz html` works there. The web
  view's delegate is its `WKURLSchemeHandler` for `zz` (`start_page` and `stop_page` in
  `ios_browser.rs`): a request sends `FetchArtifact` on the pane's host connection, the
  connection hands `ArtifactBegin`, `ArtifactChunk` and `ArtifactUnavailable` to
  `ios_browser::artifact_message`, and the task is answered when the bytes are all in, or failed
  with the daemon's reason. Relative files, reload on change and the navigation allowlist all go
  through it. A user script gives `zz://` pages `zz.reply`, which posts to the delegate and goes
  out as `ArtifactReply`; the daemon decides whether the page may reply, and the page's promise
  resolves either way, where the desktop's rejects with the reason.

  Open as a page works from an iOS chat too. Without the desktop's opener, a page frame, a
  visual's Open as a page (iPad only: the phone hides a visual's tools, and offers Open as a page
  only when its preview fails) and a plain `html` block's Globe button (and on iPhone a tap on its
  preview picture) ask the chat's own view opener with a `ViewRequest` whose `page` is set
  (`zz_ui::agent::html_opener`, only where `AgentHtmlPreviewer` is set), and
  `ViewRequest::command` turns that into `html -t <chat> - -- <text>`, so the page opens beside
  the chat on the chat's connection; the phone sends it to the daemon in place of its view sheet. The phone stays on the
  chat and the page is the next pane.

  On iPad a visual, and an `html` block's preview, goes live on a tap since 2026-11-16, as a click
  does on the desktop (`clients/app/src/ios_live.rs`, which sets `AgentHtmlLive` with `overlay`
  on; iPhone does not set it, and the phone layout on iPad ignores it, so both keep the pictures
  and the Open as a page tap). One `WKWebView` with a store never written to disk loads the page
  at the picture's size and zoom, inside a clipping view added over the app's view; once a
  visual's page has loaded, the tap is replayed where it landed (an `html` block's preview, as on
  the desktop, takes the next tap). It follows the block every paint, clipped like the web's
  frames (`html_overlay_clip`), and hides while its chat is covered or its pane is not shown. The
  web view does not scroll itself: a drag that is more up or down than sideways goes to the app as
  a scroll, so the chat moves under the finger. A tap anywhere else in the app, or a paint without
  the block, closes it and the picture comes back. While it is live it covers the visual's corner
  controls. A user script gives the page `zz.reply`, whose text reaches `draft_page_reply` and is
  drafted into the chat's composer, one reply per second at most and none over 64 KiB, as the
  desktop drafts a live block's reply. Only the page itself replies: a site it navigates to is
  shown, but its replies are dropped, as on the desktop.
* **Android** has the same shape as iOS, but its snapshot copies pixels from the window, so the web
  view has to be on screen; a hidden picture-maker needs another capture path. Not started.
* **Web** draws visuals, page frames and an `html` block's preview in its chats since
  2026-11-16, as live sandboxed `<iframe srcdoc>` frames placed over the canvas rather than
  pictures. zz-ui sets no picture task there: `zz_ui::agent::AgentHtmlFrames` asks the host for a
  frame's height and hands it each frame's bounds, the chat's clip and whether it takes input
  every time the chat paints. `clients/app/src/web_frames.rs` keeps one `<iframe>` per block in a
  fixed layer after the canvas, `sandbox="allow-scripts"` and no `allow-same-origin`, so a page's
  scripts run on an opaque origin and cannot reach the app's page, its cookie or its socket. A
  script appended to the page posts its height (the desktop's height script, 8 to 800 px) and the
  wheel events it cannot scroll itself; the host sizes the frame from the first and replays the
  second on the canvas (ala's web window places a wheel event by its client coordinates, so a
  replayed one lands where the pointer is), so the chat scrolls over a visual. A visual and an `html` block take
  pointer input, and a page frame is laid out at 1280 × 720, scaled to the column and takes none.
  On a touch-only browser no frame takes input, so a drag over one scrolls the chat. The canvas
  cannot draw over a frame, so frames step aside for whatever it draws on top. The chat clips its
  frames above the composer's band and above Jump to latest while it shows (`html_frame_clip`),
  and cuts a tooltip's rows out of them. A chat's frames hide while its @ or slash menu, a phone
  sheet or an undocked subagent PiP is open (`ChatHost::chat_covered`), and while its pane is
  not in the set iOS shows browser panes in: a floating pane over a tiled one, or another pane's
  modal, hides it. Every frame hides while settings, a dialog, a prompt, a menu or a tmux chooser
  is open. A frame not painted in a draw is hidden at the end of that draw (and removed after
  30 s). A visual is laid out at the column's width over the UI zoom and scaled up by it (17/14
  in the phone layout), as the desktop draws it. While an agent streams a block, the frame keeps
  the last page and its height until the page has stopped changing for 250 ms, as the desktop
  waits before drawing a picture. A visual's Show code and Copy sit in a row above it, always
  shown, since the pointer over a frame never reaches the canvas. A visual's or `html` block's
  frame in a chat also gets `zz.reply`: a script put first in the page posts `{zz: "reply",
  token, text}` to the app, which takes it only from that frame, only with the random token that
  page was written with (a site the frame navigates to never has it), only while the frame takes
  input and has the focus (the user clicked into it), and drafts the text into the chat's
  composer with iPad's limits. Frames are single documents: relative assets have no origin
  to resolve against, and the page inherits the app's cross-origin isolation, so a resource from
  another site loads only if that site sends a cross-origin resource policy. There is no Open as
  a page: the web's browser panes are placeholders.

Android still shows html blocks as code.

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
* `ala_cef::artifact` tests: URL parsing and percent-decoding, escaped error pages.
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

Replies from live blocks: `a_live_page_may_reply_until_dropped` (ala-cef),
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

Checked live on macOS (2026-11-15) in a zz Dev built from main, on its own socket, against a
Claude Code chat (Haiku) told to echo fixed blocks and a Codex chat asked to use its visualize
skill: an `svg` block drew as a figure; an `html preview` block showed its picture, went live on a
click, took typing and clicks (a counter moved); a plain `html` block's Preview showed the page and
went live; a long `html` block became a card; Open opened the page in a browser pane beside the
chat; a live block's Reply drafted into the composer without sending; Codex's visualize reference
became a themed chart block. Three bugs turned up and were fixed then: Reply ran `agent-send`,
which submits to a chat since 2026-11-01, so a page could start a turn; Open showed the `html`
verb's output in view mode over the chat (the daemon now skips that output for interactive
clients, as for `new-session`); and Codex references reached the screen raw, because the rewrite
ran in `PanePump` on updates only while clients draw the driver's chat events, and the chat events
sent after each delta released every held reference. Still open: the picture of a page that draws
after load (Codex charts load their code from Codex's CDN) is measured before it finishes, so the
chart's axis is cut off at the bottom; the live page in a browser pane shows it whole.

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
* Markdown, mermaid and image files as pages a chat keeps changing (Antigravity's plans and
  walkthroughs). They would draw natively; where that pane sits is undecided.
* A list or tabs of a chat's pages. Older pages reopen from their rows.
* Page rows on the thin clients, and the agent context for Cursor.
* Claude Code's own `Artifact` tool: it is not offered in the `-p` stream-json mode zz runs it in
  (checked on 2.1.329, 2026-11-20), so Claude pages go through `zz html` and the rows.
* Android inline HTML previews. The iOS and web behavior and recorded verification limits are described under [Other clients](#other-clients).
