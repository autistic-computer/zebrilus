---
type: Design Plan
title: Simulator pane . a booted iOS simulator in a PiP
description: "A pane kind that shows a booted iOS simulator's screen and takes touches and keys. It opens as a PiP at centre-right and docks with Tile back like any pane. The desktop client on macOS does the work; the daemon only keeps the pane's device. The kind travels as `Other { kind }` with a gated descriptor event, so the wire epoch stays."
status: Built (2026-11-20; desktop on macOS. Rotate, the keyboard toggle, the title's picker popover and agent verbs are not built)
resource: crates/zz/src/simulator/live.rs
tags:
- ui
- panes
- ios
- design-plan
timestamp: 2026-11-20T00:00:00-03:00
---

# Rulings (fabrico, 2026-11-20)

1. It is a pane kind, like the browser, not a new surface. It opens as a PiP; Tile back and Float move
   it between the split and the card like any pane.
2. The board is `design/mockups/simulator-pane/`, with the numbers the build copied.

# What is built

File > New Simulator Pane runs `new-pane --kind simulator` on the active pane. The pane picker lists
Simulator (`s`), which turns the picker into a tiled simulator pane with `select-pane-kind simulator`.
Both show on macOS only.

`new-pane --kind simulator [DEVICE]` opens a float of 35 by 36 cells at `centre-right`
(`SIMULATOR_FLOAT_WIDTH`, `SIMULATOR_FLOAT_HEIGHT` and `float_place` in `crates/zz-mux/src/command.rs`),
clamped to the window. `split-window` and `new-window` take the kind too, with no size of their own.
`set-simulator-device [-t %N] DEVICE` changes the device; an empty device shows the list.
`#{pane_kind}` prints `simulator`.

The daemon keeps only `SimulatorDescriptor { device }`. Snapshots show the pane as
`PaneKindSnapshot::Other { kind: "simulator" }`, and the descriptor travels in
`EventPayload::SimulatorPane`, gated by `simulator-pane-v1`
([wire protocol](/protocol/wire-protocol.md)). The daemon sends it after the tree that holds the
pane, on every change, and again on resync.

The desktop view is `SimulatorView` (`crates/zz/src/simulator/live.rs`), on `crates/zz-simulator`:

- **No device:** the list of installed simulators, booted ones first, shut down ones dimmed. Picking
  one sets the pane's device; a shut down one is booted first.
- **Screen:** the simulator's own surface, painted with `Window::paint_surface` at the surface's
  aspect, as large as fits: 6 px in from the sides and bottom in a PiP, 4 px from the top and 16 px
  from the sides and bottom when tiled. Corners are 14.6% of the screen's
  width, 3.5% for a tablet, with the theme's border as a 1 px edge. The view repaints only when the
  simulator reports a new frame.
- **Input:** press, drag and release become one touch at the same point of the screen; the wheel
  becomes a drag that ends 120 ms after the last wheel event; a key press becomes its keyboard
  usage, with Shift held around it. Keys with Command, Control, Option or Fn stay with zz.
- **Booting:** the phone's outline in the theme's border color with a 16 px loader in the middle.
- **Header:** the pane's own header, with the device's name as its title and Home, Lock, Screenshot
  (to the clipboard) and Devices as kind buttons, which fold into the ellipsis in a narrow pane.

Other platforms draw a one-line note in the pane.

# How the simulator is reached

`crates/zz-simulator` loads the simulator frameworks of the selected Xcode at run time and talks to
them through the Objective-C runtime. Nothing is linked.

- **Screen:** the device's display port hands out a shared surface and calls back on each new frame.
  The surface is replaced once right after registering, so the view re-reads it when told.
- **Input:** two routes. The simulator's own input service takes small dictionaries per event over a
  connection opened from the device's service lookup; it is used whenever the lookup succeeds. The
  older message route is the fallback. Once any tool has connected to the newer service, the older
  route stops working until the simulator's display server restarts, so the newer one is preferred.
- Every selector that was not proven on a live device is sent only after `respondsToSelector:`. A
  missing one took the app down during the first live check.

# Measured (iPhone 17e simulator, iOS 27.0, 2026-11-20)

| What | Value |
| --- | --- |
| Surface | 1170 by 2532, BGRA |
| Attach | about 6 ms |
| Frame callbacks while scrolling | 79 to 86 per second |
| Frame callbacks on a static screen | 0 |
| Opening the newer input route, with its liveness check | about 205 ms, once; the view skips the check |

# Not built

- Rotate and the software keyboard toggle; the ellipsis menu the board draws. Rotation is not part
  of the input service the pane uses.
- The picker as a popover under the title: Devices returns to the list in the pane instead.
- Stopping frame callbacks while the pane is hidden.
- Verbs for agents (capture, tap, type), a `#{simulator_device}` format variable, and a mention in
  the tools catalog.
- Two-finger touch, a remote client, Android.
