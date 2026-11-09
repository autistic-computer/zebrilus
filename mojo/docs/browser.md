# Browser pane port

The browser port follows `crates/zz/src/browser/{view,controller}.rs` and the
pure helpers they call in `zz-client::browser_state` and `ala-cef`'s page zoom.
It is not wired into `WorkspaceView` yet. `BrowserPane` in
`zzmojo/browser_pane.mojo` is the one entry point the workspace needs; the
wiring steps are at the end of this page.

## Checks

From the repository root, each acquires the shared one-job 4 GiB build guard:

| Command | What it proves |
| --- | --- |
| `python3 mojo/scripts/browser_check.py` | URL parser, tab restoration, metadata, neighbor and cycle helpers, session lookup and the eight BrowserCommand variants against Rust oracles; the controller and actions probes |
| `python3 mojo/scripts/browser_actions_check.py` | native bindings, zoom steps, popups, picker results, failures, retry, daemon commands, input ownership and presentation, exact values |
| `python3 mojo/scripts/browser_pane_check.py` | `BrowserPane` on the real engine (CEF 156.0.15 under `target/debug/build`): restore, zoom, tab switch, popup adoption and close, load error with drawn retry, select all |
| `python3 ../mjui/scripts/web/zoom_check.py` | mjui's page zoom sanitizer, percent, steps and native level against `crates/zz-client/examples/browser_zoom_vectors.rs` (3117 factors) |
| `python3 ../mjui/scripts/web/host_check.py`, `popup_check.py` | the native host: isolated and shared profiles, zoom, load error details, 66 owned views, popup adoption and refusals |

## Modules

| Module | Responsibility |
| --- | --- |
| `browser_state` | Tabs, active versus saved content, close neighbor, cycling, publication, effect kinds (`BROWSER_SHOW` .. `BROWSER_CLIPBOARD`) |
| `browser_url` | URL/address/search/diagnostic parser through the Rust boundary; profile trimming |
| `browser_host` | Current `(pane, tab, session)` ownership and receipts |
| `browser_controller` | Page events (`BROWSER_EVENT_*`), navigation, tab and profile synchronization, address entry, popup adoption, script-closed popups, picker results |
| `browser_actions` | The 22 native bindings, picker start results, runtime and pane failures, retry, daemon Navigate/Reload/Back/Forward |
| `browser_input` | Pointer ownership (pressed buttons, clamping, focus), the Escape chain, omnibox keys, whether typed text follows a page key |
| `browser_presentation` | Tab labels, address, zoom percent, page/empty/error state, retry, picker status |
| `browser_delivery` | Event-time session capture for native effects and one-shot acknowledgement |
| `browser_wire` | BrowserCommand codec |
| `browser_pane` | `BrowserPane`: the model, host, input and the pane's mjui `WebPage`s |

Transitions stage a candidate and commit against the original state identity,
actor and host receipt. Native effects (`BROWSER_CANCEL_PICK`,
`BROWSER_NAVIGATE`, `BROWSER_ZOOM` .. `BROWSER_START_PICK`) capture the tab's
session when prepared and are never redirected to a later session.

## Behavior carried from the source

- Native bindings follow `desktop_native_bindings` context 2: zoom in, out and
  reset step through 25..500 % on a live session only, and restart an active
  picker with the new zoom; devtools needs a live session; new tab opens a
  blank foreground tab and focuses the address; next, previous, numbered and
  last tab activate; back and forward need history; reload clears the error;
  the element selector toggles; the seven edits go to the focused frame; close
  pane closes the active tab while more than one is open and is otherwise not
  handled, so the workspace closes the pane.
- A popup opened by a page is adopted by the host first, then becomes a popup
  tab with its opener's zoom; foreground popups activate and focus the page.
  A popup that closes itself closes its tab, opening a blank tab first when it
  was the only one. Ctrl and middle clicks open a tab with the URL.
- Load errors read `<diagnostic URL>: <description>` on the active tab and
  `Could not load <diagnostic URL>: <description>` on a background tab. A
  runtime failure is retryable only while the runtime runs; a pane failure,
  including a refused page creation, is retryable. Retry recreates the active
  session with its URL, zoom and profile.
- Escape cancels the picker, then restores the omnibox query, then clears
  suggestions, then leaves the address for the page. Up, Down and Shift-Delete
  drive suggestions while the address is edited. Page keys go to the daemon as
  browser surface keys; text follows only unmodified printable keys.
- A pointer press focuses the page; moves reach the page only for buttons the
  page took; releases outside the page reach it while it holds a button;
  presses and leaves outside the content are dropped; dragged and released
  points clamp to the content.

## Limits

4096 tabs and sessions, 4 MiB retained text and payload, 32768 pending
effects, 65536 retained records; URLs and event text at most 65536 bytes;
favicons 8 KiB and picked element screenshots 2 MiB. Native page event queues
hold 4096 events and 4 MiB and fault instead of dropping. Send-keys repeats
stop at 9999. All limits refuse; none truncate.

## Not native yet

Devtools, the element picker (a start answers "Element picker is
unavailable"), context menus, favicons, renderer crash reports, IME
composition, cookie import and site data, and the screenshot command (refused
with an error) have no mjui host consumer. History recording and omnibox
ranking stay with the history store: `BROWSER_VISIT`, `BROWSER_TITLE`,
`BROWSER_FAVICON` and `BROWSER_HISTORY_USE` reach the outbox only.

## Wiring BrowserPane into the workspace

The integrator owns `view.mojo`, `backend.mojo`, `workspace_keys.mojo`,
`input_timeline.mojo` and `stream_transport.mojo`. Steps:

1. Runtime. Call `web_subprocess_check()` first in `main`, start one
   `WebRuntime` with `cache_dir` under the app's data directory, and watch its
   fd on the main queue (`WebRuntime.watch`). The app's wake returns
   `runtime.pending()`. Call `runtime.pump()` once per wake.
2. Panes. Keep `Dict[UInt64, BrowserPane]` beside `self.model.terminals`. When
   the tree reports a browser pane (`tree_model`'s browser descriptor: URLs,
   active index, profile), create
   `BrowserPane(pane, urls, active, profile, cache_dir, image_base)`, giving
   each pane its own range of image ids (two per page, a few hundred per pane).
   On later descriptor changes call `descriptor(urls, active, profile)`. When
   the pane leaves the tree call `close()` and drop it after `poll` reports its
   pages gone.
3. Drawing. In the pane loop, before the "Connecting to terminal…" fallback,
   add a branch for browser panes calling `pane.build(ui, theme, key)` inside
   the pane's content box. Call `pane.poll(ui.image_cache)` once per frame or
   wake before drawing, and `set_visible` when the pane is hidden or shown.
4. Focus. When the workspace focuses or unfocuses the pane call
   `set_focused`. Route the pane's pointer events through `pointer(phase,
   button, pressed, x, y, mods, clicks)` with `BROWSER_POINTER_*` and
   `BROWSER_BUTTON_*`, using `BROWSER_POINTER_UP_OUTSIDE` for releases outside
   the pane, and wheel events through `wheel`.
5. Keys. Resolve configured chrome keys first: a browser-table chord gives a
   `ChromeAction`; `desktop_native_binding(2, action)` gives the binding and
   `pane.action(binding)` runs it. A False result means the workspace handles
   it (close pane with one tab). Other keys go to `pane.key(event)`:
   `BROWSER_KEY_PAGE` sends `input_key(pane, mjui_key_input(...),
   route.text_follows, browser=True)` to the daemon; typed text that follows
   goes to the daemon as browser surface text once the protocol front encodes
   it, or `pane.text(text)` until then; `BROWSER_KEY_ADDRESS` and the omnibox
   kinds go to the address field; `BROWSER_KEY_PASS` continues normal routing.
6. Daemon commands. Decode BrowserCommand events with `browser_command_event`
   and call `pane.command(command, provider)` for the pane they name.
7. Outbox. After each call drain `take_outbox()`: `BROWSER_PUBLISH` sends the
   tab URLs and active index to the daemon, `BROWSER_FOCUS_PAGE` selects the
   pane (`select-pane -t`) and moves focus to it, `BROWSER_FOCUS_ADDRESS`
   focuses the address field, `BROWSER_NOTIFY` shows a toast (`flag` marks an
   error), `BROWSER_CLIPBOARD` writes text and PNG, and the history kinds go to
   the history store.
8. Chrome. The tab strip and toolbar draw from `presentation()`: `tabs`
   (label, detail), `active`, `address`, `loading`, `can_go_back`,
   `can_go_forward`, `page_zoom_percent`. The address field submits through
   `pane.navigate(text, provider)`; tab clicks call `select_tab`, close buttons
   `close_tab`.
