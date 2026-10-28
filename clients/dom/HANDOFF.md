# HTML client port: status

The five lanes (terminal, workspace, overlays, settings, agent) are merged. `just design compare`
serves both clients on a scratch daemon and `just design parity` diffs them. On the merged tree:
the workspace 17/17 in dark and light, a split 24/24, settings 52/52, all within 2px. Each lane also
reported its own scenarios (palette, choosers, menus, find bar, kitty images, the agent lens,
permission cards, every settings page) matching within 2px.

Logic the gpui client decided outside of drawing moved into `zz-client` and both clients call it:
`sidebar`, `pane_picker`, `floating` (PiP geometry, float placement), `chooser`, `palette`
(pure helpers), `tmux_style`, `menu` (hit cells, row heights), `terminal_view`, `terminal_color`,
`local_scroll`, `agent_lens`, `agent_timeline`, `agent_question`, `mend`, `providers`.

## Open

- Sync debt: `clients/dom/src/palette.rs` copies about 1,600 lines of zz-ui's palette model, view
  rows and `@` target grammar, because those types carry gpui types. Move the palette model and
  target grammar into `zz-client` with plain types, point zz-ui, the desktop palette, `clients/app`
  and the storybook at it, then delete the copy.
- The copy predates main's place palette: sigil chips (`^ $ @ %`), recents, the `&` files level,
  `>` GUI actions, `?` ask and vim mode. The parser, resolver and recents already live in
  `zz_client::place` and `zz_client::recents`, so port the DOM palette onto them.
- Settings' Theme group on gpui is now a Mode segmented control and a theme picker with Import and
  Delete in a more menu; the DOM page still draws mode tiles and a theme strip (Save is gone).
- A split pane reports about 2 fewer terminal columns than gpui for the same width, so long lines
  wrap earlier. Check the column math in `terminal.js` against `clients/app/src/terminal.rs`
  (padding, scrollbar gutter, rounding).
- Terminal: popup terminals have a view but nothing mounts them; the find bar input has no focus
  ring (the kit has no focused field token); bold and italic font families and ligatures are not
  used.
- Workspace: throwing or stashing a float, spring motion, program icons (no artwork in the kit),
  pane stack hover in window tabs, tooltips, the narrow (phone) layout.
- Overlays: Driver mode in the palette, which-key UI (gpui draws it only on phone layouts), the
  Unzoom chip for `choose-tree -Z`.
- Settings: narrow layout, gpui's own color picker and font list, minimum contrast in the terminal
  preview, the Advanced fade, toasts drawn by the overlays module.
- Agent: syntax highlighting in code blocks, HTML/Mermaid/SVG previews, the `@` picker, an accent
  token for `chat.bubble.mine`; question cards, plans, background tasks, the queue chip, error cards
  and jump-to-bottom are shared logic but unchecked visually because no test agent produced them.
