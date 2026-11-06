# Mojo and Bend migration

The migration remains in progress. The target is the complete zz application using Mojo and mjui, with applicable protocol and core logic implemented and formally verified in Bend2, and mjui visually compatible with current penna and ala.

The repositories were synchronized from `main`: zebrilus `27b789d56`, mjui `b6655d1`, agents `74fcbc9`, penna `b0ffcae`, and ala `abe9829`.

## Current software

- `bend2/` contains executable envelope validation, layout operations, attention logic, replay reduction and terminal patch preflight. `LAWS.bend` states the proved invariants; `PROOF.bend` and its supporting modules provide machine-checked proofs. `just port check core` independently rechecks the proofs and compares executable results with the actual Rust implementation. See [the core port boundaries](../bend2/PORT.md).
- `mojo/` contains the native command client and an mjui workspace. Every received envelope passes through a persistent compiled Bend validator, and terminal patches use its native cell reducer within the documented size bound. Typed snapshot and tree-delta reduction, navigation, visible splits, resize reporting, native input, raw local cwd/environment publication, and packed terminal rendering work against the existing daemon. The remaining application features require migration. See [the client boundaries](../mojo/docs/port.md).
- The adjacent mjui checkout imports all six current penna appearances, 95 roles, 213 recipes and seven states. Numeric agreement covers 8,946 direct appearance records. Built-in context inheritance and selection controls use exported native values and source behavior. Native raster agreement, remaining widget variants and integrated glass rendering still require work; complete visual identity is unproved. See `mjui/docs/penna/compatibility.md`.

## Commands

```sh
just port setup
just port check core
just port check app
just port check appearance
just port build
just port run /tmp/zz.sock
```

Verification logs and scoped results are written under `target/port/`. Pixel reports preserve whether exact identity was achieved; generating a report does not establish visual identity. `python3 scripts/port/appearance_pixels.py --require-exact` rejects differing images.

Builds use `scripts/port/guard.py`: one shared build slot, a maximum 4 GiB memory budget and at most 256 MiB of swap in a Linux user cgroup. The guard reserves 2 GiB of available device memory at startup and stops its own scope if available memory drops below 1 GiB. Mojo test tasks run sequentially. Generated appearance dispatch reads indexed data through a small out-of-line function rather than expanding hundreds of branches into widget callers.
