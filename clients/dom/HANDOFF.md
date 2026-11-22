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
- Terminal: popup terminals have a view but nothing mounts them; separate bold and italic font
  families are not overridden independently.
- Workspace: throwing or stashing a float, spring motion, pane stack hover in window tabs, tooltips,
  the narrow (phone) layout.
- Overlays: which-key UI (ala draws it only on phone layouts).
- Settings: narrow layout, ala's own color picker and font list, minimum contrast in the terminal
  preview, the Advanced fade.
