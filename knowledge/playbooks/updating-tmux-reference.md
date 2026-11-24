---
type: Playbook
title: Updating the pinned tmux behavioral reference
description: Move the reference build and oracle to one tmux release and verify changes in zz's behavior.
resource: third_party/tmux-reference/UPSTREAM.md
tags: [tmux, upgrade, playbook, behavioral-reference]
timestamp: 2026-11-24T00:00:00Z
---

The reference build supplies the executable and source tree for differential tests.
The shipped multiplexer implements the supported behavior independently.
`third_party/tmux-reference/UPSTREAM.md` records the current tag, commit and source files
by feature area. [Reference](/references/tmux-upstream.md)

1. Resolve the new release tag to an exact commit. Diff the old and new source trees
   across the areas in `UPSTREAM.md`, including parsing, keys, prompts, buffers, modes,
   layouts, options, hooks and the client/server boundary. Update that file, its source
   links and the retained license.
2. Update `TMUX_TAG`, `TMUX_COMMIT` and `TMUX_VERSION` in `compat/fetch-tmux.sh`,
   their counterparts in `compat/tmux-oracle.py` and `compat/startup-diagnostics.sh`,
   and the version assertion in `compat/attached-client.sh`. Search for the old hash and
   version in active fixtures, manifests and reference pages. Dated evidence keeps its
   measured version.
3. Build the new reference, then regenerate the oracle from it:

   ```sh
   tmux_bin="$(compat/fetch-tmux.sh)"
   python3 compat/tmux-oracle.py --tmux "$tmux_bin" --write
   ```

   The fetcher checks the tag and commit, clean checkout, reported version and build stamp.
   Use its normal build for a pin change. `ZZ_COMPAT_TMUX` only checks an override binary's
   reported version. CI's cache key includes the fetcher, generated oracle and corpus fetcher.
4. Review the generated diff. Port changed behavior to the responsible zz crate, or record
   the specific difference in `compat/tmux-gaps.json` with its evidence. Run the closest
   scenarios first, then the inventory checks and full differential corpus:

   ```sh
   just compat check
   just compat run
   ```

   The full run checks terminal and attached-client behavior before writing the summary.
   For an interactive desktop check use `just run mac` or `just run linux` and exercise
   the affected commands and bindings. [Harness](/playbooks/compat-harness.md) ·
   [Commands](/tmux/commands.md) · [Key tables](/tmux/key-tables.md)
5. Keep the pin, generated inventory, behavior changes, fixtures and reference updates
   together. Plugin pins move separately. For a plugin bump update `compat/fetch-corpus.sh`,
   `third_party/tmux-plugin-corpus/UPSTREAM.md` and the retained license, then run that
   plugin's scenario.
