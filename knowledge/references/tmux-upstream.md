---
type: Reference
title: tmux behavioral reference pin
description: The pinned upstream tmux commit zz's Rust multiplexer reimplementation is checked against, and where the per-behavior file map lives.
resource: third_party/tmux-reference/UPSTREAM.md
tags: [tmux, reference, pin, behavioral-compatibility]
timestamp: 2026-11-24T00:00:00Z
---

# Overview

zz implements tmux commands, key tables and configuration in Rust. The compatibility
baseline is tmux 3.8. The [differential harness](/tmux/tmux-compat.md) runs both
implementations to compare behavior.

# Where the detail lives

`third_party/tmux-reference/UPSTREAM.md` maps behaviors to the upstream parser,
command handlers, key tables and copy-mode files. It also records later references
used for hooks and event formats, and the pane mode used for `display-panes`.
The upstream license sits beside it in `LICENSE.txt`.

# Citations

- `third_party/tmux-reference/UPSTREAM.md` . in-repo source of truth for the pin and the file map
- Pinned commit: tag `3.8`, [`7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53`](https://github.com/tmux/tmux/tree/7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53)

# Related

- [tmux compat concept](/tmux/tmux-compat.md) . the Rust reimplementation checked against this pin
- [Updating this reference](/playbooks/updating-tmux-reference.md) . how to bump the commit and re-verify
- [`mux` crate](/crates/zz-mux.md) . where the reimplementation lives
