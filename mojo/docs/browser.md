# Browser state port

The browser port is a bounded, renderer-independent foundation. It does not yet
activate a browser pane in the Mojo workspace or claim a live CEF replacement.
The authoritative desktop behavior is `crates/zz/src/browser/{view,controller}.rs`.
The thin-client browser implementation is a separate consumer.

Run `python3 mojo/scripts/browser_check.py` from the repository root. The wrapper
acquires the shared one-job, 4 GiB build guard and uses the locked offline source
graph. It builds an actual Rust oracle and an independent Mojo candidate for each
source comparison. No expected source state is supplied to candidate transitions.

| Module | Responsibility |
| --- | --- |
| `browser_state` | Stable tab IDs, lazy restored tabs, active versus saved metadata, right-first close neighbor, wrapping selection, ordered publication snapshots |
| `browser_url` | Exact existing URL/address/search/diagnostic parser through a small native boundary; exact profile trimming and control rejection |
| `browser_host` | Current `(pane, tab, CEF session)` ownership and retained actor receipts |
| `browser_controller` | Navigation, descriptor/profile synchronization, active/background metadata, pending omnibox use and typed history effects |
| `browser_delivery` | Event-time native owner capture, missing-session disposition and one-shot effect acknowledgement |
| `browser_wire` | All eight actual BrowserCommand variants, full-width IDs, key tokens and repeated counts |

The desktop source calls the extracted `zz-client::browser_state` restoration,
switch, neighbor, cycle, load-error and session-lookup helpers. The state oracle
covers 75 descriptors, 132 complete metadata switches and six sanitized load
errors. The URL corpus has 140 cases and 13 profile cases. The ownership oracle
covers 792 source lookups across replacement/removal. The wire oracle compares
all eight variants in 29 actual encoder frames and rejects truncation/trailing
bytes. The controller additionally exercises history start/completion, active
versus inactive errors, profile recreation, stale model/host plans, missing CEF
sessions and refusal without live mutation.

Use `prepare_browser_transition` and `prepare_browser_event` for admitted state
changes. Preparation stages a candidate; commit requires the original bounded
state identity, original actor and current host receipt. The lower-level model
methods are pure transition helpers used by the source comparisons. Their native
effects are deliberately ineligible for delivery until event-time ownership has
been captured. Re-preparing delivery never redirects an earlier missing session
to a later replacement. Acknowledgement frees the captured effects once and may
follow host session creation during dispatch.

Publication effects retain their exact URL list and active index. Browser
requests retain URL, profile and Float64 zoom. History effects retain the profile,
URL, title or query, selected flag and favicon bytes needed by the existing
history consumer. Changing current tab/profile afterward does not change these
captured values. Active tab metadata remains separate from saved tab metadata
until the source save/switch boundary.

Admission is explicit: 4096 tabs/sessions, 4 MiB aggregate retained text and binary
payload, 32768 pending effects and 65536 aggregate retained records. URL/event
inputs are at most 65536 bytes and favicon data at most 8192 bytes. The URL parser
output is capped at 256 KiB. Wire retention admits at most 4096 tokens and 4 MiB;
these resource limits can refuse larger source-admitted workloads and never
truncate them. Candidate copies and identity receipts are additional bounded
storage; this is not a bound on native Chromium processes.

The browser descriptor already retains URLs, active index and profile in the Mojo
tree model. BrowserCommand events currently remain in the existing bounded raw
interactive-event queue; the new wire codec has no live dispatch hook yet. CEF
frame/session creation, popup adoption, zoom/edit commands, context menus, picker,
IME and native clipboard consumers must be completed and proved on a real host
before that gate can open. `mjui.web.WebPage` remains the available platform shim.
The desktop history store and omnibox ranking/persistence remain existing native
consumers; emitting typed history effects alone does not establish their full
Mojo parity.
