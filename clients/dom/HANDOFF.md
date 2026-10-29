# HTML client port: status

The five lanes (terminal, workspace, overlays, settings, agent) are merged. `just design compare`
serves both clients on a scratch daemon and `just design parity` diffs them. On the merged tree:
the workspace 17/17 in dark and light, a split 24/24, settings 52/52, all within 2px. Each lane also
reported its own scenarios (palette, choosers, menus, find bar, kitty images, the agent lens,
permission cards, every settings page) matching within 2px.

Logic the ala client decided outside of drawing moved into `zz-client` and both clients call it:
`sidebar`, `pane_picker`, `floating` (PiP geometry, float placement), `chooser`, `palette`
(pure helpers), `tmux_style`, `menu` (hit cells, row heights), `terminal_view`, `terminal_color`,
`local_scroll`, `agent_lens`, `agent_timeline`, `agent_question`, `mend`, `providers`.

## Open

- Sync debt: the places palette (`clients/dom/src/palette/places.rs`) runs on `zz_client::place`
  and `zz_client::verbs`, but its rows, chips and drafts still copy zz-ui's `palette_places.rs` and
  `palette_view_places.rs`, and `palette.rs` copies the daemon `:` prompt and chooser views, because
  those types carry ala types. Move them into `zz-client` with plain types and delete the copies.
- The DOM palette has no GUI settings verbs, no `insert path` file level and no vim mode; recent
  places live only for the page's lifetime.
- Settings' Theme group on ala is now a Mode segmented control and a theme picker with Import and
  Delete in a more menu; the DOM page still draws mode tiles and a theme strip (Save is gone).
- A split pane reports about 2 fewer terminal columns than ala for the same width, so long lines
  wrap earlier. Check the column math in `terminal.js` against `clients/app/src/terminal.rs`
  (padding, scrollbar gutter, rounding).
- Terminal: popup terminals have a view but nothing mounts them; the find bar input has no focus
  ring (the kit has no focused field token); bold and italic font families and ligatures are not
  used.
- Workspace: throwing or stashing a float, spring motion, program icons (no artwork in the kit),
  pane stack hover in window tabs, tooltips, the narrow (phone) layout.
- Overlays: which-key UI (ala draws it only on phone layouts), the Unzoom chip for
  `choose-tree -Z`.
- Settings: narrow layout, ala's own color picker and font list, minimum contrast in the terminal
  preview, the Advanced fade, toasts drawn by the overlays module.
- Agent: syntax highlighting in code blocks, HTML/Mermaid/SVG previews, the `@` picker; question
  cards, plans, background tasks, the queue chip, error cards and jump-to-bottom are shared logic but
  unchecked visually because no test agent produced them.
