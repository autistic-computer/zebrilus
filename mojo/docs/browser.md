# Browser pane port

The browser port follows `crates/zz/src/browser/{view,controller}.rs` and the
pure helpers they call in `zz-client::browser_state` and `ala-cef`'s page zoom.
`BrowserPane` in `zzmojo/browser_pane.mojo` is the one entry point the
workspace needs; `WorkspaceView` drives it in a browser build, as described
at the end of this page.

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
  page took; releases outside the content, `BROWSER_POINTER_UP` or
  `BROWSER_POINTER_UP_OUTSIDE`, reach it only while it holds a button;
  presses and leaves outside the content are not forwarded and a press there
  holds no button; dragged and released points clamp to the content.
- Submitting the address (`navigate`) focuses the page, as accepting the
  omnibox does; a daemon Navigate command does not.

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

## In the workspace

`zzmojo/browser_workspace.mojo` holds the pane set the workspace drives. Its `WorkspaceBrowser` trait has two implementations, picked at compile time: `BrowserDeck` when the build defines `ZZ_BROWSER` and links `libmjcef.a` and `libzzbrowserurl.a`, and `NoBrowser` otherwise, which owns no pane and links no CEF symbol. `WorkspaceView.browser` is `ActiveBrowser`, so the default app and smokes build without the host. `bash mojo/scripts/ui.sh build-browser-app` builds `target/port/zz-mojo-browser-app` with it.

- Runtime: `main` calls `web_subprocess_check()` first in a browser build. The first browser pane in a snapshot starts one `WebRuntime` with its cache under `$XDG_DATA_HOME/zz-mojo/browser` (else `~/.local/share`); a failure is kept and drawn in each browser pane. The app watches the runtime's fd on the main queue, and `wake` pumps it and wakes on pending page events.
- Panes: after each poll, `sync_browser` reads every kind 2 pane's descriptor (tab URLs, active tab, profile) from the snapshot, creates a `BrowserPane` per new pane with its own 1,024 image ids, calls `descriptor` when one changed, and closes panes that left the tree, keeping them until their pages are gone. At most 64 browser panes are held; more are refused with a notice. It then polls every pane into `ui.image_cache`.
- Drawing: a browser pane draws a header with one button per tab (a click calls `select_tab`) and the address, then `BrowserPane.build` in the rest of the pane's box. The panes drawn this frame are the visible ones; the input pane gets `set_focused` while the terminal focus is on it.
- Pointer and wheel: `mouse` gives an event over a browser pane's content, or any event while that pane holds a button, to `BrowserPane.pointer` with the left, right and middle buttons and the held mask; a press selects the pane, a release outside the content goes as `BROWSER_POINTER_UP_OUTSIDE`. The wheel over the content goes to the page through `BrowserPane.wheel`, which clamps the position to the content box.
- Keys: with the browser pane focused, `route_workspace_key` follows the router's order for a press. The `ui` table runs first. A press the prefix claims (the prefix key, any key while the prefix is armed or a key table is active) or one made while the window captures input is not a browser key: it goes to the daemon as a plain key, so prefix commands work from a page. Then the `browser` table resolves and runs `desktop_native_binding(2, action)` through `BrowserPane.action`; close-pane with one tab left runs `kill-pane`. Other keys go to `BrowserPane.key`: page keys go to the daemon as browser surface keys (`input_key(..., browser=True)`, then `input_browser_text` for their text) and their release follows; pass keys continue as usual. A window that goes inactive forgets the held page keys, so a release that arrives later is not sent to the pane.
- Daemon commands: `stream_transport` keeps BrowserCommand events (event kind 7) in a 4,096-entry queue instead of the unhandled list; the backend moves them to the view, which calls `BrowserPane.command`. A refused command, such as the screenshot command, is a notice.
- Outbox: `BROWSER_PUBLISH` runs `set-browser-tabs -t %pane -a <active> <urls>`, `BROWSER_FOCUS_PAGE` selects the pane and focuses the terminal slot, `BROWSER_NOTIFY` is a notice. Address focus, clipboard writes and the history kinds have no consumer yet.

`python3 mojo/scripts/browser_workspace.py` (pixi task `browser-workspace-smoke`) builds the smoke with `ZZ_BROWSER` against the source CEF 156.0.15 and runs it on a private daemon with a browser pane: the page loads and draws while the terminal pane is active, a click on the page selects the browser pane, focuses the page and reaches it (the page sets its title after its first two animation frames, since the engine drops input sent before the page has drawn: three runs against a page that set its title while loading each lost the first click, three runs against this one each took it; the check still repeats the click until the page acknowledges it in its title and prints how many it sent), a typed key reaches the page through the daemon, the prefix key arms the prefix table from the focused page and Escape cancels it without reaching the page, Ctrl+T publishes a second tab, Ctrl+W closes it and then closes the pane.

The address field and omnibox, the tab strip's close buttons, back/forward/reload buttons, the error panel's address focus and the remaining items under "Not native yet" stay open.
