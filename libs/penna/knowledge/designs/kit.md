---
type: Design Plan
title: "The kit: penna owns every piece of UI chrome"
description: "How apps compose penna components, choose surface looks and read theme values for their own content."
status: Built (2026-11-02)
resource: src/foundation/surface.rs
tags:
- ui
- theme
- design-system
timestamp: 2026-11-24T00:00:00Z
---

# The kit

Penna owns UI chrome. Apps choose components, lay them out and supply their content and behavior.
Terminal cells, a pane's material, colors supplied by tmux and the positions of computers on a
desk belong to the app. The keyboard and pointer behavior shared by several components belongs in
penna. Ala provides elements, layout, input and drawing.

Use `Space` for gaps and insets, `TextStyle` for type and `Size` for controls. Their dimensions
are in design pixels and scale with the root rem. The medium control and row heights follow
`Density`. The scales, exceptions for drawing geometry and optical offsets, and each component's
interaction rules live in [Foundations](foundations.md).

## Looks

A widget's typed look picks a penna recipe. For example, `TabLook::Window` selects `tab.window`
and `FieldLook::Composer` selects `field.composer`. The mappings live beside the components in
`src/components/`. Penna defines recipe IDs and patches in `src/foundation/roles.rs`.
Each theme supplies settings such as tint, radius and glass. `platform_theme` starts from
`macos` on macOS and iOS and `default` elsewhere. Appearance applies the person's settings.

Pass the ground a component sits on to `.on(ground)` so its colors resolve against that surface.
`Theme::ground_of(look)` reads the resting ground and `ground_at(look, state)` reads a state.
`radius_of`, `shape_radius`, `style_of` and `casts_shadow` expose the geometry and style for
content that follows a look. `ContentLook` covers panes, floating panes, PiP, pane stacks and
the window. `field_chrome` and `field_paint` take a `FieldLook` for a field laid out by the app.

## Lint

`SurfaceRole` constructors and fields are crate-private. Theme tooling can enumerate existing
roles with `roles::all`, look one up with `roles::find` and inspect its id, kind or patch.
`Surface::new(role)` and `Theme::style(role)` serve tools that sample roles by name.

Zebrilus's `clippy.toml` and nidus's `crates/nid-app/clippy.toml` disallow those two methods,
manual surface painting and hand-set text sizes and weights in app code. A content painter that
needs an exception scopes it to the function and gives its reason. Review gaps and insets by
hand because ala's spacing setters accept both a token and a literal.

## Changing the kit

Update the component's specification in Foundations and show its states in the storybook on
each theme, in light and dark. Inspect the worktree's copy with `storybook/build.sh` and
`storybook/serve.py`. Check parity for the changed story and update its baseline only for
intentional visual changes. From Zebrilus, `just libs check penna` checks the subtree and
its storybook. Pull library changes into one app at a time. When a widget's API changes,
update each app in the commit that pulls the change.
