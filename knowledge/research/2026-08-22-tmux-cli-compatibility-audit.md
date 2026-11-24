---
type: Research Report
title: tmux CLI compatibility and alias boundary
description: A commit-pinned inventory of the tmux command, flag, option, format, hook, key, packaging, and native zz command surfaces, with the exact boundary around alias tmux=zz.
git_commit: 3109d83c056e1ae691ad83cf606847654feb2e9e
resource: compat/tmux-gaps.json
tags: [tmux, compatibility, cli, alias, commands, options, formats, hooks, key-tables, research]
status: complete
timestamp: 2026-08-22T16:12:36-03:00
---

# Alias compatibility at the August pin

The audit compared zz `3109d83c0` with tmux `d77c9dc6` (next-3.8). It counted accepted names separately
from usable behavior and from measured client output. The full per-name inventories, flag pairs, config
and packaging evidence are in the
[commit-pinned audit](https://github.com/autistic-computer/zebrilus/blob/918e62838/knowledge/research/2026-08-22-tmux-cli-compatibility-audit.md).

| Surface | Result at `3109d83c0` |
| --- | --- |
| Commands | 80/92 tmux commands executed. All 78 aliases resolved, 71 to executable commands. Of 170 total spellings, 151 executed and 19 reported unsupported. Fourteen zz-only commands shared the catalog and five more lived in daemon dispatch. |
| Flags | 51/80 executable commands had no catalog-declared missing flag. 29 commands refused 113 command/flag pairs. Accepted syntax could still differ in output, ordering, streams, target resolution or client state. |
| Options | All 180 non-hook options stored and read back. 104 had a behavior consumer and 76 had storage only. |
| Formats | All 198 names parsed. 34 were always unavailable, 13 depended on a client/list context and 151 were backed in the intended context. |
| Hooks | All 68 names stored, with 57 automatic producers. Missing producers covered queue/resize, client lifecycle, focus/theme and pane clipboard events. |
| Default prefix keys | zz had 60 bindings, the pin 92, with 59 shared. Split-picker and sidebar actions differed from the tmux commands that imported bindings could still invoke. |
| Differential coverage | 56 scenario files contained 816 executable steps (42 normal, two known divergences, 12 plugin/config). The checked-in report covered 48 scenarios/678 steps. |

**Alias boundary.** Shell aliases affect commands parsed by that shell. Direct process lookup of `tmux`
requires an executable on PATH. In the audited packages zz installed its own launcher, and Unix jobs
spawned by zz received a private PATH entry. Configuration import and the zz wire protocol remained
separate from executable-name resolution. The packaged bare command initially attached on an existing
daemon but failed on an empty one. A TTY could create and attach using `new -s NAME`.

**What the initial harness proved.** It checked exit class, topology, geometry, format/query stdout and
plugin warning streams. It did not compare every ordinary command's stdout. The invalid-octal config
fixture matched whole-file abort while control-client warnings differed. Headless comparisons left
prompts, choosers, stock key dispatch, actual terminal interaction and executing a packaged launcher
untested.

## Follow-up at the old pin

Later August slices reached 83 executable tmux commands plus 19 native catalog commands and 157/170
executable tmux spellings. The recorded rejected flag total moved from 113 to 85 across 23 commands.
Prompt history, manual resize, empty-pane creation, environment overlays, paste-buffer capture routing
and scoped flags gained differential coverage. The always-unavailable format count moved from 34 to 32.
The packaged bare command became `new-session -A`. Explicit `attach-session` still reported an empty
daemon.

The full strict run then checked 71 scenarios/1,058 executable steps. Each known row had one geometry
difference and none for topology, formats, output or warnings. A separate attached-PTY fixture checked
root/prefix/prefix2, copy mode, rename, tree keys, buffer actions and nested attach refusal. A macOS
bundle under a path with spaces passed six bare/new/attach cases. Those results applied to these
fixtures and build revisions.

## Where to check current behavior

At `1ce632dce`, `third_party/tmux-reference/UPSTREAM.md` records the tmux 3.8 release tag `7f2a35ad`.
`crates/zz-protocol/src/catalog.rs` lists only `link-window` and `unlink-window` plus their aliases in
`UNIMPLEMENTED_TMUX_COMMANDS`, and `crates/zz-cli/src/lib.rs` uses `new-session -A` for an empty command
chain. The old flag counts do not describe this pin. Consult `compat/tmux-gaps.json` for individual
behavior exceptions. [3.8 reassessment](/research/2026-10-09-tmux-compat-revisit.md) ·
[Compatibility harness](/playbooks/compat-harness.md)

Related contracts and decisions are in [tmux compatibility](/tmux/tmux-compat.md),
[divergences](/tmux/divergences.md), [commands](/tmux/commands.md), [key tables](/tmux/key-tables.md),
[status formats](/tmux/status-line.md), the [drop-in plan](/designs/tmux-drop-in.md) and the
[superset roadmap](/designs/tmux-superset-roadmap.md).
