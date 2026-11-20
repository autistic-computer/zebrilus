# zz for iOS

The app opens the session sidebar beside the attached window's panes. It uses
`zz-ui` for the shell, navigation, pane frames, headers, terminal painting, and
agent interface. Web and iOS compile the same app shell, sidebar, status bar, settings,
command palette, overlays, terminal, agent, connection reducer, and image caches
from `clients/app` (`zz-app`). The UIKit backend is `ala_platform::ios` in `libs/ala/crates/ala-platform/src/ios`;
this crate (`zz-ios`) is only the entry point, the terminal example, and the bundle files.
The sidebar is 256 points wide and respects the iOS safe area.

On iPhone, and in any window narrower than 640 points, the attached session uses the
phone shell instead: one pane fills the screen and a sideways drag on the bar's pill pages
through every pane in window order. A sideways swipe on the pane itself stays with the pane. Each page paints its pane's color, opaque, from the top of the screen
down, with no header: the status bar is transparent over it, and its text turns light or dark
to match. Landing on a pane zooms it (`resize-pane -Z`), so the daemon sizes
the window to that pane; terminal previews keep the neighbouring pane live while it
slides in, and a browser pane slides in with its page showing. The bar at the bottom is two
pieces: the pill, with a short handle on its top edge, and the plus disc. The pill holds every pane of the
session as its program's face, the one on screen stretched into a capsule with its name over what
it is doing; a pane waiting off screen has a yellow dot on its face, and the capsule says
"Needs approval" in yellow. Swipe the pill sideways to page, the capsule moving with the finger,
or tap another face to go to that pane. Over a web page the pill keeps its text color,
as Safari's bar does, and deepens its glass instead when the page under it is light. Tapping the
capsule opens the pane menu as a stack of chips: Rename and Close for the pane (Close asks first in a
sheet, Rename opens a sheet with the name), Find and All bindings in a terminal. Tapping plus opens
Chat, Terminal and Browser under "New in" and the pane's session, nearest the thumb,
with New session above. A tap on the pill's handle goes up one level. The overview is three levels you
zoom through, the way Calendar goes from year to month to day. The handle, or dragging the pill up
(past a third of the way or with a flick), zooms out to its host's page: the pane shrinks toward
the middle of the screen while the page comes in behind it, and the bar swaps to the machine's. A drag
follows the finger, and letting go short of a third springs it back to full screen. A machine's
page has the machine's name as a large title, which folds into a frosted bar
as the page scrolls, then every session under its name with its windows as cards: a window with a
terminal shows each terminal's last four lines, up to two side by side, and any other window is one
row saying what its pane says in the pill. Tap a window and it zooms into that pane; hold it for
Rename and Close window, and hold a session's name for Rename session and Close session. The pill
holds every connected machine as its platform mark, the one on screen stretched into a capsule with its
name over the session in view, following the scroll; with more machines than fit, the row
scrolls to keep the capsule in the middle. Swipe it sideways to page machines, the capsule
moving with the finger, tap another machine's mark to go there, and tap the capsule for All
machines and Settings. Plus stacks Chat, Terminal and
Browser under "New in" and that session, then New session; what it makes lands in that session. A
machine with no sessions shows the New tile, which opens the same menu under "In a new session". A machine that is not connected has no page here; one
that is connecting shows Cancel as its main action, and leaves for Machines if it fails. All machines, the handle, or dragging the pill up, zooms out to Machines: every saved machine as a small tile, two to a row, waiting ones first:
its mark, its name, and its windows as the desktop's program faces with their status dots, or its state in words; a
tile with something waiting is tinted yellow and counts it. Then Add machine and Settings. Tap a tile to zoom into the
machine, into its connecting card while it connects, or to reconnect a failed one, and hold it for its sheet. With nothing attached the app opens on Machines. Entering a host, or paging to it with the pill, puts it on screen, since the
daemon streams previews only to the client attached to it. The phone has no session tree: the sidebar toggle and `focus-sidebar` open the
overview instead. The keyboard opens only from that button or a tap on the terminal.
While it is up, a key row replaces the bar, shaped like it: a pill holding prefix, ctrl, alt,
⌘ and …, then the arrow pad and Done as discs. A pressed, latched or open key shows the same
muted wash as the pill's capsule. Prefix opens the pane menu, rising from the key, and its All
bindings sheet sends any prefix binding through the daemon's key table. Ctrl, alt and ⌘ share one
gesture: tap to latch the modifier for the next key, hold or slide up for its chords, and release
on one to send it (Interrupt, End input, Suspend, Word back and the rest; Paste, Copy, Find
and Select all for ⌘). The chips name what a chord does, not its keys. A
key sent with ⌘ runs zz's own binding. … opens Escape, Tab, / and @. The arrow pad is a
thumbstick: hold it and lean toward a direction, and it repeats faster the further the finger
goes, with a tick per arrow. Done lowers the keyboard.

Both sheets follow the finger when dragged down by the grabber or the header, or by their
content once it is scrolled to the top. Dragged past about half their height, or flicked down,
they close; a shorter drag springs back, and dragging up resists. A tap on the dimmed area above
still closes them. Small controls take taps across a 44-point box around the glyph (pane close,
settings back, tree actions, reset and stepper buttons, switches, select menus), workspace tree
rows are 44 points tall, and each key in the key row answers across the whole row height and
up to the middle of the gap to its neighbours.

The phone keeps the desktop's look and grows it for touch: settings, the workspace tree, menus,
dialogs, and daemon overlays render their text at 15 points instead of the desktop's 13
(everything sized in rems grows by 15/13). Settings keep the desktop cards, copy, and controls.
Each row puts its control beside the title and the description underneath at full width; theme
and palette tiles sit below. Switches grow to 44 by 26. A dropdown opens a bottom sheet of the
desktop's menu rows at 44 points, check in front, with a search field once the list passes 12
choices; fonts preview in their own face. A color well shows its hex and opens a sheet with the
hex field and the desktop's ten-column swatches at touch size. Every page opens with its title large,
under a bar holding only the back arrow; once the page scrolls the large title away, the bar
takes the title small and centered over a hairline. The section list follows the desktop
settings sidebar's groups. A
rightward drag anywhere on a settings page goes back, except on a horizontal strip (the palette
tiles) that can still scroll back.

Buttons, rows, menu items, and keys show presses the way Flutter's CupertinoButton and React
Native's Pressability do: a touch lights the control 100 ms after it lands unless it turns into a
scroll first, the highlight fades in over 120 ms and out over 180 ms, and a quick tap still shows
it for 130 ms. Keys in the key row light at once. Lifting a finger clears hover, so a tapped row
does not stay lit.

Pinch a terminal to change its text size, as in Blink Shell: the pinch starts once the
fingers' spread changes by 6% and by at least 8 points (ala's touch slop), then follows the
fingers from that spread. The size moves in 5% steps between 50% and 300% (the Terminal
font scale setting), and a badge shows the percentage while pinching and for a second after.
The text resizes live; the pane reports its new grid to the daemon once, after the fingers
lift, and the size is saved then. A second finger cancels the first finger's scroll, page
swipe, or tap, so two fingers never page or scroll.

## Run

Use an Apple Silicon Mac with Xcode, Rust's `aarch64-apple-ios-sim` target, and
Zig 0.16.0. From the repository root, `just ios` lists the actions. Each takes `iPhone`
(the default) or `iPad`:

```sh
just ios rig
just ios run
just ios run iPad
just ios build
```

`just ios rig` builds `zz_cli` and starts a throwaway daemon for simulator runs, with its own
home under `target/ios-rig` and a socket at `/tmp/zzios-<checkout folder>.sock`, so every
worktree gets its own. It seeds a session named `phone` with two panes in the first window and
a second window, and `just ios run` attaches to it while it is up. `just ios rig stop` stops it.
`ZZ_IOS_RIG_SOCKET` picks another socket.

To run on a paired iPhone or iPad, unlock it and use device mode:

```sh
just ios device
just ios device iPad
```

Device mode builds a release arm64 binary, signs it with your first Apple Development identity
and a provisioning profile that covers `dev.zz.ios-dev` (a team wildcard works), installs it with
`devicectl`, and launches it with `ZZ_ALA_ENDPOINT=ssh://$USER@<this Mac>.local`. That launch
connects straight away and adds the host to the saved list; later launches from the home screen
open the connection screen with it at the top. The Mac needs Remote Login
enabled and `zz-dev` on its PATH (desktop dev runs link it into `~/.local/bin`). Sign in with your
password; after a password sign-in the app offers to add its key to `~/.ssh/authorized_keys` so
later connections skip the password, and a toast says whether it was added. The key is also in the
Add host sheet to copy by hand. On iPad, Settings > Hosts names why a host failed (zz missing, too
old, or newer than the app) and offers **Install zz** or **Update zz** on ssh hosts, which runs the
installer there over the same ssh login (release builds only; dev builds refuse, as on the
desktop). An ssh host whose daemon is an older build than the app also gets **Update zz**, which
installs the app's version and restarts the daemon. Override the
choices with `ZZ_ALA_DEVICE`, `ZZ_ALA_SIGN_IDENTITY`, `ZZ_ALA_PROFILE`, or `ZZ_ALA_ENDPOINT`.

Simulator and device builds use the zz Dev icon (`assets/zz-dev.icon`, compiled with `actool`).
`ZZ_DEV_BUILD=0 just ios device iPad` installs the release identity instead: `dev.zz.ios`, named
zz, with the zz icon. Its profile has to cover `dev.zz.ios`.

## TestFlight

```sh
just ios testflight
```

TestFlight mode builds a release binary with the production identity (`zz` on the host), packages
it as `dev.zz.ios` ("zz", the bundle the native client shipped under) with the zz icon, workspace
marketing version, and a UTC timestamp build number, and wraps it in an `.xcarchive` under
`target/ios-testflight`. The `testflight` Cargo profile keeps line tables, so the archive carries a
dSYM for crash symbolication and the shipped binary is stripped afterwards. `xcodebuild -exportArchive` then signs it for App Store distribution and
uploads it for internal TestFlight testing. Signing uses the Apple account signed into Xcode; set
`APPLE_API_ISSUER_ID` to use the App Store Connect key in `../../.zz-signing` (beside the folder that holds the repos) instead.
`ZZ_IOS_UPLOAD=0` exports the signed `.ipa` without uploading, and `ZZ_IOS_BUILD_NUMBER` overrides
the build number.

Without a rig, the launcher probes zz Dev socket candidates and skips stale sockets. It does not
start a daemon. To choose a socket and session:

```sh
ZZ_ALA_ENDPOINT=/tmp/my-zz.sock ZZ_ALA_SESSION=work just ios run
```

`ZZ_DEV_SOCKET` also selects a local socket. Set `ZZ_ALA_SIMULATOR` to a simulator
UDID to choose an exact device. The bundle ID is `dev.zz.ios-dev`, its display name
is **zz dev**, and build products live in `target/ios/ZZ Dev.app`.

The tree shows the connected host, its sessions, windows, and panes. Tap a marker to expand or
collapse it, or a row to select it. Arrow keys navigate the tree; Enter selects
the highlighted row. The plus buttons create sessions and windows. Long-press a
row or window pill to open its rename and close menu; on a host row that menu has Remove host, and
Install zz or Update zz when the host offers it. Add host in the sidebar opens a dialog for the
destination and connects the host it names. Swipe to scroll. The top sidebar button hides the sidebar and remains available to reopen
it. The gear opens Settings; Command-comma also toggles Settings. The search button
opens the command palette without a hardware keyboard.

The app does not connect on launch. Until it connects, the workspace shows the connection screen:
saved hosts (most recent first) with Connect and Remove, a destination field (`user@host` or
`user@host:port`), and the app's SSH key. On iPhone it fills the screen under the status bar, with no
toolbar above it. Settings › Hosts shows the same list. On iPhone each host is one row: its name
(a socket's file name, or Local socket for the default one) and its state; tapping
it opens a sheet with the full address, the last error, and Open, Connect, Disconnect or Remove
host. Add host opens the same sheet as the Hosts page's tile: the destination field, Connect, and
Copy SSH key. Settings › Agents lists each connected host's agents and gathers the rest under Not
connected. On iPad at 640 points wide or more, Coding agents above them has a Set up row per
connected host that installs the Claude Code and Codex startup hook on that host, like
`zz tools --install-hooks` there, and shows what it changed in a toast named after the host. Hosts are saved in `Library/Application Support/ala/hosts.json`; a
successful connection moves its host to the top. `ZZ_ALA_ENDPOINT` (or `ZZ_SOCKET`) connects
for that launch only, and opening a `zz://attach/<session>` link connects to the most recent host.
A `zz://host/<machine>:<port>` link (the QR code in desktop Settings › Hosts) adds that tailnet
machine and connects it. Every connected host is asked for the other machines on your tailnet; they
are listed under On your tailnet on Machines and in Add machine, where a tap saves and connects one;
on iPad they sit above Add host on Settings › Hosts and the connection screen, each with an Add
button. In the command palette a host row says when it is reconnecting, installing zz, missing zz
(Return installs it) or speaks an incompatible version.
SSH endpoints show host-key confirmation and authentication dialogs. The terminal example shares
the native transport, key translation, and UIKit backend.

## Panes

- Terminal panes and the agent lens use the web client's working pane entities. Each pane
  keeps its own input focus, viewport, and content state.
- Split right or bottom from the header, then choose Terminal or Browser. Close
  removes the pane through the daemon. Drag a divider to resize the split.
- With a pointer, the header's Split, Float, Tile, Zoom and Close buttons show their
  prefix keys in the tooltip, read from the host's prefix table as on the desktop.
- A zoomed pane has an Exit zoom control. Drag a header grip to split or swap panes;
  the preview animates to the drop target. Mouse and touch divider drags update the daemon.
- Drag a PiP's bottom corners to resize it and its header to move it. Drag a modal float's
  top border to move it and its top or bottom border ends to resize it. The grips are 44 pt
  and stay on the card, so touches beside it reach the pane underneath. A read-only or
  disconnected client draws no grips.
- Chat, view and browser panes report their size like terminals, so a window holding only a
  chat follows the iPad's size and rotation.
- Hardware keys, search, copy/paste, terminal images, mouse tracking, and
  scrollback use the shared terminal implementation. Swipe to scroll; long-press
  and drag to select text.
- A terminal running Claude Code or Codex shows a lens button in its header. The lens
  swaps the terminal for the agent's transcript, a composer that types into the TUI,
  Claude permission cards, and queued prompt feedback. Plans, permissions, and
  structured tool output use the shared transcript reducer. Code blocks use the native
  syntax highlighter.
- An `html` block in an agent chat has an Open button, and a page an Open as a page button,
  that opens it in a browser pane beside the chat (`html -t <chat>`). A visual has Open as a
  page on iPad; on iPhone only when its preview fails.
- On iPad, in a window 640 points or wider, a tap on a visual or on an `html` block's preview
  makes it live: a web view takes the picture's place (a visual's also gets the tap), and the
  page's `zz.reply` drafts its text into the chat's composer. An up or down drag on it scrolls the
  chat, and a tap anywhere else turns it back into the picture. iPhone and narrower iPad windows
  keep the pictures.
- Chat controls the desktop shows only under the pointer always show on iPad: Copy and Fork
  under messages, a code block's buttons, a visual's corner controls, a diagram card's pop-out
  mark and a subagent row's time and PiP mark. The Copy, Fork, code block, figure and visual
  buttons take a tap 44pt across and sit far enough apart that a finger hits the one it aims at.
- On iPhone a chat's plan is a row over the composer that unfolds the plan, and the plus sheet
  shows the chat's Git branch and changes under its settings.
- An empty agent chat on iPad is the desktop's: the welcome over the dithered sky, then the
  folder button and Resume, each opening the desktop's popover menu at its desktop size. A tap
  on its search field raises the keyboard and the popover moves up to stay above it. On iPhone
  the two are rows under the title that open sheets, with no sky.
- Browser panes use native WebKit with tabs, an address field, back/forward, and reload.
  SSH connections route page traffic through the host; `localhost` addresses reach
  the host's loopback servers through port forwarding. Cookies and site storage
  persist separately for each host and browser profile on the iPad. They do not
  share the desktop browser's login or live page state.
  The element picker copies context and a cropped screenshot for pasting into an
  agent. Tap the picker, then tap a page element; Cancel exits without copying.
  `zz capture-browser` on the host captures the page as the iPad last drew it, also
  when its window is not on screen, and writes the PNG on the host, so agents can
  look at it. A daemon from before
  `browser-screenshot-v1`, or a pane this iPad has not laid out, answers with an error.
  The toolbar's more button opens the page in the default browser, copies its URL,
  switches the pane's browser profile (Default or any profile the host's browser panes
  use), zooms the page, clears site data, reloads and starts the picker. The button at
  the start of the address field opens the site menu: whether the connection is secure,
  Sound (mutes the page's audio and video elements and suspends its Web Audio, kept
  across navigations; frames from other sites keep playing) and Clear
  cookies and site data, which asks first, clears everything WebKit keeps for the site's
  domain in this profile and reloads. Addresses that are not URLs go to the search
  engine picked in Settings › Advanced › Browser, a setting kept on the iPad.
- The iPad keeps its own browser history for each host and browser profile, in
  `Library/Application Support/ala/browser-history/`, ranked the same way as on
  desktop. A new tab lists the recently visited pages; typing in the address field
  shows matching pages under it, completes the address of a page you have typed
  before, and learns which result you pick for a prefix. Up and Down move through the
  results, Escape steps back out, and Shift-Delete removes the selected page from
  the history. Rows show a globe, not the site's icon. The status bar's pane list
  names a browser pane by its page title from that history.
- `send-keys` to a browser pane, from an agent or the command line, types into the
  page: text goes into the focused field, Enter submits a form or breaks a line,
  Backspace and Delete edit, Tab moves focus, the arrows and Home and End move the
  caret in a field (not in email or number fields, which keep the caret position
  from pages) and scroll the page elsewhere, and C-a selects all. Keys mirrored
  from another client typing into the same page fire their key events without
  adding the character twice; the text follows on its own. WebKit gives the app
  no way to send real key events, so the page gets untrusted ones: handlers
  that check `isTrusted`, browser shortcuts, input methods and frames from other
  sites do not see them.
- With a hardware keyboard, a focused browser pane takes the desktop browser keys:
  Command-L for the address, Command-T for a new tab, Command-R to reload, Command-[
  and Command-] for back and forward, Command-Shift-[ and Command-Shift-] to change
  tabs, Command-1 to Command-8 for a tab and Command-9 for the last, and Command-Shift-C
  for the element picker. Command-plus, Command-minus and Command-0 zoom the page
  instead of the interface; each tab keeps its own zoom, stepped from 25% to 500% as on
  desktop. These keys are View menu commands while a browser pane is focused, because a
  focused page gets every other key before zz does. Control-Tab and
  Command-Option-arrows also change tabs, but only while the address field has focus.
- Editor panes open, edit and save the host's files through the daemon (`file-edit-v1`),
  up to 2 MiB of UTF-8 text. The folder button in the pane's header, or Command-O,
  opens the palette's file list on the file's folder, and picking a file opens it in
  that pane; a check button, or Command-S, saves once there are changes. The editor is
  empty and read-only until the file arrives, and saving is refused until it has, so a
  file that fails to open is never overwritten. A connection that drops before the host
  answers shows the error with Try again; the pane stays while the iPad reconnects.
  A pane with no file shows an Open a file button. Line numbers and soft wrap are on,
  vim mode is off, and there is no Save As. A daemon from before `file-edit-v1` keeps
  the placeholder. Editor does not appear in the new-pane picker.

Command-P, Command-K, or the search button opens the palette that desktop and web also use. Use `:` for commands,
`@` for windows, `%` for panes, and `~` for the connected host. Opening it closes
any daemon prompt or chooser, as on desktop. Daemon choosers, command prompts, menus,
confirmations, and popup terminals use the shared interface. Command output replaces
its pane's content, and display-panes labels keep their tmux styles and alignment.
Notices preserve severity, duration, and explicit clearing; a failed command reports
as `command: error`.

On iPhone the pill opens the pane menu, plus opens the New menu, and dragging the pill up opens navigation.

## Keyboard

Without a hardware keyboard, tapping a terminal, the agent composer, or any text
field raises the on-screen keyboard. For a terminal the app draws the phone's key row
across the bottom of the window, above the keys: a pill of the prefix key, ctrl, alt, ⌘
and … (Escape, Tab, `/`, `@`), then the arrow pad and Done as discs. In the wide layout (640 points and up) the
prefix key sends the prefix (⌃B unless the host rebinds it, `prefix2` when `prefix` is
`None`, and no key when both are) instead of opening the pane menu. Settings fields,
prompts, the palette, and host forms get the plain keyboard. The terminal example keeps the
backend's own row (Escape, Tab, Control, Option, the arrows, and hide), which it shows
only for an input whose Return inserts a line break.
Ctrl, alt and ⌘ latch for the next key, so ctrl then C sends Ctrl-C. The
workspace shrinks to the space above the docked keyboard, and a focused settings
field scrolls back into view above it; a focused row of a long settings page keeps
painting while it is off screen, so the keyboard stays up. A floating keyboard
leaves the layout alone; in the wide layout the key row still sits at the bottom of the
window while it is up. Attaching a hardware
keyboard hides the on-screen one, and detaching it brings it back for the focused
field. Autocorrect, smart punctuation, and capitalization stay off unless a field
asks for them.

The layout moves with the keyboard frame by frame, as Flutter's keyboard inset does: the
backend reads the keyboard's own spring from the keyboard layout guide and evaluates it on
the display link at each frame's display time, so the phone key row, the terminal's bottom
edge, popovers, and a focused settings field slide with the keys on open and close. In the
phone shell the key row takes the bar's place where the bar's top edge was and rides on the
keyboard once the keys pass it. While the keyboard opens, terminals keep their grid (the
prompt row stays above the key row) and report their new size once it stops. While it closes,
the backend reports where the keyboard will come to rest (`target_visual_viewport_bounds`),
and the phone shell sizes the terminal for that resting place from the first frame: the shell
gets its one resize at the start, the grid hangs past the top edge under the status bar, and
the rows the keyboard uncovers slide into view already filled instead of arriving in one step
after the keyboard lands.

A field picks its keyboard through ala's text input mode (the HTML `inputmode` set), which
the backend maps onto `keyboardType`: decimal and numeric pads, phone, email, URL, and web
search keyboards; `None` keeps the field focused with no software keyboard, through an empty
input view. Number fields in Settings ask for the decimal pad, and the browser client gets
`inputmode="decimal"` from the same setting. Pads without a Return key (decimal, numeric,
phone) get a Done button above them; a comma from the decimal pad is typed as a point. Done,
or Return in any settings field, commits the value and hides the keyboard. Return in the Hosts
destination field connects and hides it too.

In the wide layout, pausing after the prefix shows the which-key sheet at the bottom of the
panes: the core bindings of the prefix table, or of a custom `-T` table, grouped as on the
desktop. `?` (whatever is bound to `list-keys -N`) while the prefix is armed shows every
binding instead and keeps the prefix armed, so the next key runs its binding. Any other key
hides the sheet. The delay is a setting on this device (System > Navigation, 400 ms, 0 turns
the sheet off and gives `?` back to the host's `list-keys`). The phone keeps its All bindings
sheet.

The view implements `UITextInput`, so IME composition (Japanese, Chinese, Korean),
dead keys, dictation, and Pencil handwriting reach the focused field. While text is
being composed, hardware keys go to the input method first.

## Windows, menus, and links

- Each iPad window (Stage Manager, split view, or an external display) runs its own
  workspace with its own connection. New windows come from the system's window controls.
- The iPadOS menu bar and the Command-hold shortcut list show the zz, File, Edit, View and
  Help commands, with Command shortcuts where the desktop has them: Settings, New Session,
  New Window, Split Right, Split Down, New Browser Pane, Switch Session (one row per session
  on the host), Rename Session, Rename Window, Close Pane (Command-W), Kill Window, Kill
  Session (asks first), Detach, Find, Copy Mode and Clear Scrollback in the active terminal,
  Command Palette, Choose Window, Toggle Sidebar, Zoom Pane, Select Pane, UI zoom, Back,
  Forward and Reload in the active browser pane, and links to the docs, release notes and a
  new issue. While a browser pane is focused in the active window, Back, Forward and Reload
  take the browser's Command shortcuts, View adds Open Location, Pick Element, New Tab, Next
  Tab, Previous Tab, and Select Tab, its zoom items zoom the page, and Close Pane closes the
  tab when more than one is open. Detach leaves the session running and shows the
  empty workspace: Attach to session (New session when the host has none, Return or a tap),
  Settings, and the prefix keys a session unlocks (`c`, `%`, `"`, `s`, `?`, or the keys the
  host binds to those commands).
- Command-1 through Command-9 open the first nine windows the sidebar lists, across sessions
  and connected hosts, or the first nine chat rows in the chat view. Holding Command for a moment
  shows each row's number in the sidebar. The phone layout leaves these keys to the pane.
- Command-Shift-M stamps `user_marker seq=N` into the app log and the daemon's log and
  confirms with a toast, so a moment worth reporting can be found later.
- `zz://attach/<session>` (`zz-dev://` for development builds) switches the frontmost
  window to that session, or attaches to it after the next connect. Shortcuts can open it
  with the Open URL action. The deprecated native dev app also claims `zz-dev://`; remove
  it if links open the wrong app.
- Long-press text in a terminal to select it; the system edit menu offers Copy, Paste,
  and Select All. On iPad, letting go of a selection made with a trackpad or mouse also
  copies it, as on the desktop, unless the session's `set-clipboard` is `off`. A long-press
  selection leaves the clipboard alone, so Paste in its menu pastes what you copied.
- Ctrl-V with an image on the clipboard uploads the image to the pane instead of sending
  Ctrl-V. Text pastes over 4 MiB are refused with a warning.
- Ctrl-= and Ctrl-- on a hardware keyboard step the Terminal font scale setting by 10%
  (on the pinch's 5% grid, between 50% and 300%), so every terminal changes and the size is
  saved.
- A terminal bell plays the system alert sound, which vibrates instead on an iPhone in silent
  mode. Bells within 200 ms of the last sound are dropped. The host's `bell-action` and
  `visual-bell` options decide which bells ring.
- The cursor stops blinking after 10 seconds without input and while its pane is off screen,
  as on the desktop.
- Drop images onto the workspace to upload them to the focused pane, as with a pasted
  image; dropped text or links paste as text.
- An image pasted into an agent chat's composer goes through the desktop's step too: one
  wider or taller than 1568 pixels is scaled down and re-encoded to PNG, so a full-size
  screenshot attaches instead of being refused as over 6 MiB.
- Tap a link in a terminal to open it. On iPhone it opens the link sheet. On iPad a web link
  opens in the nearest browser pane of the terminal's window, as on the desktop, and in
  the system browser when that window has no browser pane. Tapping a pasted image's `[Image #N]`
  opens the image preview. On iPad a trackpad or mouse click selects as on the desktop:
  Command-click or Control-click a link to open it.

## Lifecycle

The connection reconnects two seconds after it drops, and immediately when the app
returns to the foreground. Leaving the app keeps the connection open for the short
background time iPadOS allows. Settings › System › Display can keep the screen
awake while connected. Under thermal pressure or Low Power Mode, frames are capped
at 60 per second. The system text size (Dynamic Type) scales the interface and applies
live. On iPad it multiplies UI zoom and can be turned off in System › Display; on iPhone it
is the only interface scale, capped at the largest non-accessibility size (135%) because it
scales controls and icons as well as text. Reduce Motion and Increase Contrast apply live.
The text system loads the iOS system fonts, so Chinese, Japanese, Korean, Arabic, Hebrew,
and other scripts render. Emoji still show as blank: ala's text system only treats Noto
Color Emoji as a color font.

When a pane on the attached host asks for input, fails or finishes while you are looking
at another pane, the app plays the request or done chime. While the app is inactive or in
the short background time, it also posts a notification naming the program and the pane;
tapping it selects that pane (on iPhone it leaves the overview and any sheet for the pane).
iOS asks for permission the first time a pane needs you, and Settings › System ›
Notifications turns them off. While they are on, the app icon badge counts the panes that
need you, failed or finished unseen; turning them off clears the badge and never asks for
permission. A suspended app gets nothing: that would need a push service.

## Frame rate

The display link asks for the screen's top rate with the range Flutter uses (half the maximum, but
at least 60, up to the maximum), so ProMotion iPhones and iPads animate at 120 Hz;
`CADisableMinimumFrameDurationOnPhone` in `Info.plist` lifts the iPhone's 60 Hz cap. The link
pauses three ticks after the last frame request and restarts when the window is invalidated, a
touch begins, or a momentum scroll, key repeat, or keyboard animation is running, so an idle
app takes no vsync callbacks and the display drops to its idle rate. The window opts out of ala
presenting the last frame again for a second after fast input, and the pager, sheets, and swipe
back end the touch fling they swallow, so a swipe stops ticking once the page settles.

To measure frames on a paired, unlocked device without touching it:

```sh
just ios bench
```

Bench mode builds and installs the device app like `device` mode, creates a session named
`iphone-bench` on the host's `zz-dev` (two panes with scrollback), launches the app three times
against it (pager swipes, terminal flings, then 25 idle seconds), kills the session, and prints one
`frames total` line per bench and the last idle link reports. `ZZ_BENCH_CYCLES` (default 16),
`ZZ_BENCH_SESSION`, `ZZ_BENCH_CLI`, and `ZZ_BENCH_OUT` (log directory) adjust it. To bench an app
built elsewhere, such as a baseline, run `scripts/ios-bench.sh <udid> <app> <endpoint>`. In the
output, `interval` is the time between new frames in milliseconds (8.33 at 120 Hz), `dropped`
counts vsyncs that passed without a new frame inside a burst, `cpu` is the main-thread time of
each display link tick, and an idle `link:` line with few ticks means the display link is paused.

`ALA_FRAME_LOG=1` prints, per burst of drawn frames, the interval between new frames (p50,
p95, max), missed vsyncs, and the CPU time of each display link tick, plus link ticks and draws
every five seconds. `ALA_BENCH=swipe`, `scroll`, `drag`, or `fling` (optionally `:count`,
default 16) waits eight seconds after launch and then plays horizontal pager swipes, vertical
terminal flings, slow 1.5 s terminal drags that hold still before letting go, each followed by a
fling back, or one terminal fling every 4 s, two toward older output and then three toward newer,
from the display link. `ALA_BENCH=sheet` opens the All bindings sheet on attach and flings
its list down and back, which with `ZZ_ALA_GLASS` set measures content moving under glass. `ALA_FRAME_STATS=frames.jsonl` writes ala's per-frame JSON stats into the app's
`tmp` directory. `ZZ_ALA_GLASS=frosted` (or `regular`, `clear`, `bubble`, `smoked`) turns
floating surfaces into liquid glass. Device and simulator launches forward these variables and `ZZ_ALA_SESSION`;
`ZZ_ALA_CARGO_PROFILE=testflight` builds the device app with line tables for Instruments.

## Settings

Settings uses the shared `zz-ui` form rows, previews, palettes, color pickers,
number fields, and switches. iPad keeps the section navigation beside the page;
iPhone opens on a list of sections; tapping one opens its page, and back returns
to the list. A drag from the left edge also goes back: the page follows the finger with
the section list sliding in underneath, and releasing past half the width or with a
rightward flick completes it. On the list, the same swipe returns to the workspace.

- **Appearance:** System/Light/Dark appearance, light and dark palettes,
  background/foreground/accent colors, UI zoom, contrast, animations, widget
  corners, font family, and shadow strength. Fresh installs use System appearance;
  saved choices survive upgrades. System appearance follows live iOS changes;
  pinned modes also update the native status bar. On iPad and the web it also has a
  terminal preview, Window follows the theme (on by default: the window wears the host's
  terminal colors when they match the shown mode), and Terminal text contrast (Follow host,
  On for 4.5:1, Off).
- **Panes:** live preview, gaps, inactive opacity, selected glow and margin.
  Frame controls apply with gaps.
- **Terminal:** a live preview, local font family, and text scale. On iPad and the web, Light
  theme and Dark theme list the terminal themes installed on the host (searchable, opening on
  themes of their own kind) and switch with the light and dark mode; the device keeps the pick
  and paints with it, and Follow host keeps the host's colors. A daemon from before
  `terminal-themes-v1` shows no theme rows. Cursor style, cursor blink, and padding X and Y
  follow the host until set on the device; an empty padding field follows the host again. A
  device cursor style replaces the host's default shape only, so a shape a program asks for
  still shows. The host's config is never written.
- **Multiplexer:** on iPad and the web, the host daemon's Prefix (typed as tmux spells it, such
  as `C-a`), Mode keys, Mouse, History limit, Clipboard and Escape time. Each change runs
  `set-option -g` on the host, or for Mouse `set-option -t` on the attached session, and lasts
  until its daemon restarts; the host's `zz/mux.conf` is not written. Changes made elsewhere show as they happen, and a value the host refuses shows its
  error and the field goes back to the host's value.
- **Hosts:** the configured endpoint, connection status, and reconnect.
- **Advanced:** command palette layout (tree or flat), host prefix, and command shortcuts
  (the palette's recents are kept across launches in `palette-recents.json` beside the
  preferences);
  Navigation: the which-key delay; Display: drawing under the home indicator, matching the
  system text size, and keeping the screen awake while connected; Notifications: Notify when
  a pane needs you.
- **About:** app version and source link.

The page omits desktop-only options and controls for unsupported pane kinds. iPhone also
leaves out Panes (the phone shell shows one pane without gaps or frames), Multiplexer, UI zoom, Match
system text size, the command palette settings, the which-key delay, and the terminal rows above
that iPad and the web add (the phone draws terminals in the app's own colors).
Interface and pane preferences are saved atomically in the app container
at `Library/Application Support/ala/preferences.json` and restored on launch.
Its `chrome_keys` list rebinds the app's own keys on this device, entries like
`{"chrome-keybind": "prefix:y=open-settings"}` or `{"chrome-unbind": "ui:D-,"}` with the
desktop's `chrome-keybind`/`chrome-unbind` grammar; there is no Settings page for it yet.
Touch controls work without a keyboard; numeric values open a decimal pad and hex colors
open the on-screen keyboard.

## Terminal experiment

The terminal uses `InteractiveClient`, `ClientCore`, and the shared `zz-ui`
painter and Kitty image cache. The daemon owns the PTY and Ghostty parser/encoder.

```sh
ZZ_ALA_DEMO=terminal just ios run
```

To attach the terminal example to a specific socket and session:

```sh
ZZ_ALA_DEMO=terminal ZZ_ALA_ENDPOINT=/tmp/my-zz.sock ZZ_ALA_SESSION=work just ios run
```

Without a saved endpoint, the terminal example opens a connection field. **Host** opens that
field again. Enter a socket path on Simulator or `ssh://user@host` for a remote
daemon. The transport exposes host-key trust and authentication prompts in the
app. The remote host needs a compatible `zz` executable. Only the endpoint is
saved by this example.

## Terminal behavior

- Hardware typing, Ctrl, Alt/Option, Shift, Command, Escape, Tab, arrows,
  Home/End, Insert/Delete, Page Up/Down, and F1 through F24.
- Held-key repeat and release events, including the Kitty keyboard protocol.
  Ghostty chooses escape sequences from the terminal's current modes.
- ANSI styles, true color, alternate screen applications, cursor state, terminal
  replies, and Kitty images use the existing daemon and shared renderer.
- Command-C/V/A copy, paste, and select all. The toolbar also exposes Copy and
  Paste. Pasted text uses the daemon's bracketed-paste handling. On iPad, when
  the window is at least 640pt wide, a copy sweeps a short shimmer over the
  copied text, as on the desktop, except under Reduce Motion.
- Drag to scroll; enable **Select** to drag a selection. **End** returns to the
  live screen. Shift-Page Up/Down and Shift-Home/End navigate scrollback.
- Touch events become terminal mouse events when the application requests mouse
  tracking. Ctrl/Command-click can activate links. Resizing updates the PTY grid.
- A trackpad or mouse drives the app like a desktop pointer: hover states, two-finger and
  wheel scrolling, clicks and drags as mouse events, secondary click for context menus, and an
  I-beam pointer over text.
- Finger flicks and two-finger trackpad scrolls coast with `UIScrollView`'s deceleration
  (0.998 per ms); wheel ticks do not coast. Scroll views, lists, and the terminal's scrollback
  rubber-band past their edges and spring back, and a fling that reaches an edge bounces off it.
  Mouse-tracking applications, copy mode, and screens without scrollback keep plain scrolling.
  The bounce lives in the ala fork (`Overscroll::Bounce` in the backend's gesture tuning).
- Terminal scrollback moves with the finger by pixels, and a fling coasts the same way. The app
  keeps up to 2000 rows above the screen (the `history-trickle` option) and draws scrolled rows
  from them without waiting for the daemon. The daemon's view trails the gesture by up to a
  screen, more rows are fetched as the view nears the oldest kept row, and once the view holds
  still for 120 ms the daemon's view is moved there. New output does not move a scrolled-back
  view; scrolling back to the bottom follows output again. This is the desktop's trackpad
  scrolling (`LocalScrollState` and `HistoryPacer` in `zz-client`). `less`, `vim`, and other
  mouse-tracking or alternate-screen programs still get wheel events.
- In the phone shell, rows of a scrolled-back terminal, or one bouncing at the bottom, keep
  drawing above the top row, under the status bar up to the top of the screen, the way content
  runs under a translucent iOS navigation bar. A tint of the pane color behind the status bar,
  fading out over 12 points below it, fades those rows so the clock stays readable. At the live
  bottom nothing draws above the top row, so the resting layout is unchanged.

The standalone example displays one active terminal pane. The main app supports
multiple terminal panes with the agent lens; full mux overlays remain a later step.

## Verification

Pane checks on iPad and iPhone Simulator covered terminal input, creating and
closing splits, dragging dividers, zoom, focus after creating a pane, scrollback,
long-press selection, and copy/paste. An isolated ACP fixture exercised the agent
composer, transcript, and permission response. Pane settings updated the preview
and workspace and survived relaunch. The shared web tests and WASM build pass.

Settings checks on iPad and iPhone Simulator covered section navigation,
light/dark/system appearance (including a live OS change), palettes and color
overrides, zoom, contrast, animation and corner controls, scrolling, reconnect,
and persistence across relaunch. Preference tests cover file round trips,
missing fields, invalid data, and numeric limits.

Sidebar checks on iPad and iPhone Simulator covered touch expansion, session and
pane selection, session/window creation, hiding and reopening the sidebar, and
scrolling a tree with 14 windows. Hardware End/Enter selected a pane in another
session and activated its containing window. The shared tree navigation tests,
native keyboard tests, iOS Clippy, and arm64 device build pass.

Simulator checks exercised raw PTY bytes for normal/application arrows,
modifiers, Tab, function/navigation keys, repeats, Kitty releases, and bracketed
paste. Vim opened, edited, and saved a file through hardware key events. A live
shell also accepted hardware input on iPhone. An ANSI fixture and a Kitty image
rendered on iPad. Automated checks cover native key
translation, terminal bindings, and the shared image cache; the web client also
builds after adopting that cache.

iPad Simulator checks on 2026-09-22 covered the on-screen keyboard, key row, Control
latch, arrows, hide and system dismiss, tap to reopen, hiding on hardware keyboard
attach, layout above the keyboard, Option-E dead keys, CJK/Arabic/Hebrew rendering,
automatic reconnect after a daemon restart, live text size changes, the Command-D menu
command, a second window scene, `zz://attach/<session>`, and long-press Copy/Paste.
Drag and drop, dictation, Pencil handwriting, CJK input methods, and the menu bar
itself were not exercised.

iPhone Simulator checks on 2026-10-06 covered pinching a terminal in the phone shell (one grid
report per pinch, after release; a two-finger sideways pan neither pages nor scrolls), live
system text size changes from XS to AX5, and the phone's Settings list. iPad Simulator kept
Panes, Status bar, UI zoom, and Match system text size. Screen recordings on iPhone 17 Pro
Simulator then followed the key row, the terminal bottom, an Interface number field, and the
color picker frame by frame while the keyboard opened and closed (one terminal resize per
keyboard move), and covered the decimal pad, Done, and Return in the Hosts field.

iPhone 17 Pro Simulator checks on 2026-10-07 covered the decimal pad and Done bar on an Interface
number field asked for through the text input mode (the pad's comma typed as a point, Done
committed and hid it), the plain keyboard on the Hosts field, and the terminal keyboard with the
phone key row. Recordings of a keyboard close over a full scrollback showed the top rows already
filled as they slid in and no change after the keys landed, and a close over a cleared screen kept
the prompt at the top; the shell logged one SIGWINCH per open and per close.

Copy/paste of text copied within the app was verified. Text injected through
`simctl pbcopy` was not readable from UIKit in this simulator session; cross-app
paste still needs a device check. Physical keyboard layouts, dead keys, SSH
authentication, rotation, suspension/reconnect, and GPU recovery have not been
verified on hardware.

```sh
cargo test --locked -p zz-app
cargo test --locked -p ala-platform --all-features --test ios
cargo test --locked -p zz-ui terminal_images::tests
cargo fmt -p zz-ios -p zz-app -p zz-ui --check
IPHONEOS_DEPLOYMENT_TARGET=26.0 cargo clippy --locked -p zz-ios -p zz-app -p ala-platform --all-targets --target aarch64-apple-ios-sim -- -D warnings
IPHONEOS_DEPLOYMENT_TARGET=26.0 cargo build --locked -p zz-ios --bin zz-ios --target aarch64-apple-ios
just web build
```

## Recovered history

- `015b3a39` (2026-08-07): original UIKit backend and ala card demo.
- `22c6bb5d` (2026-08-08): separate ala iOS client.
- `adbb9a3d` (2026-08-15): replacement with the Swift client. Its parent contains
  the last full ala iOS implementation.

`ala_platform::ios` restores the small window/display adapter and scene startup
from that work. It uses ala's wgpu renderer and CosmicText font
system, with the web client's Inter and Lilex fonts plus the memory-mapped iOS
system fonts. It explicitly selects Metal because
ala's native wgpu convenience constructor defaults to Vulkan and GL.
