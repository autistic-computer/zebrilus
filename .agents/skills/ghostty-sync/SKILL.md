---
name: ghostty-sync
description: Maintain the vendored native Ghostty in third_party/ghostty and its libghostty-vt wrapper and sys crates in third_party/rust. Use when changing Ghostty's Zig code, pulling an upstream Ghostty change, regenerating the C bindings, sending a terminal patch upstream, or when the user says "bump ghostty", "sync ghostty" or "vendor ghostty".
---

# Native Ghostty: `third_party/ghostty`

Ghostty lives in `third_party/ghostty`: a trimmed snapshot of upstream `ghostty-org/ghostty`
with zz's commits on top. `third_party/rust/libghostty-vt-sys/build.rs` builds it with Zig;
`GHOSTTY_SOURCE_DIR` still points the build at another checkout. Cargo reruns the native build
when `build.zig`, `build.zig.zon`, `include`, `pkg` or `src` change under the source dir. Zig
unpacks Ghostty's packages into `third_party/ghostty/zig-pkg` (ignored), once per checkout.

The trim keeps what the libghostty-vt build reads on every target: `build.zig`,
`build.zig.zon`, `LICENSE`, `include/`, `src/` without fonts and crash dumps, every
`pkg/*/build.zig` and `build.zig.zon`, and the pkg dirs it compiles (apple-sdk, highway,
simdutf, translate-c, wuffs). The list is `keep` in `scripts/vendor-ghostty.sh`.

History:

- `Import Ghostty <sha>` commits hold pristine trimmed upstream and end with a
  `Ghostty-Upstream: <full sha>` trailer. Each import's parent is the previous import.
- zz's changes are ordinary commits after it: `git log <last import>..HEAD -- third_party/ghostty`
  lists them, and `third_party/rust/libghostty-vt-sys/UPSTREAM.md` says what each one is for.

The safe wrapper and the sys crate are vendored next to each other in `third_party/rust`
(`libghostty-vt`, `libghostty-vt-sys`); their `UPSTREAM.md` files own the commit IDs,
rationale, validation, and removal conditions.

## Syncing Ghostty upstream

1. `just vendor ghostty <full sha|branch|tag>`. It fetches into `~/.cache/zz/ghostty.git`,
   commits the trimmed snapshot on top of the last import in a throwaway worktree, and merges
   that commit. The merge base is the last import, so conflicts are exactly the places where
   upstream changed lines zz changed.
2. Resolve conflicts under `third_party/ghostty` and `git commit`.
3. If Zig fails on a missing file the trim dropped, add a pathspec to `keep`, commit the script,
   and rerun step 1 with the same rev: the new import adds the file and merges cleanly.
4. If the C headers changed, regenerate `src/bindings.rs` with the sys crate's tool, then
   `rustfmt --edition 2024` it (the tool's formatter output differs from the workspace's), and
   update the wrapper in `third_party/rust/libghostty-vt` to the new API:

   ```bash
   GHOSTTY_SOURCE_DIR=$PWD/third_party/ghostty cargo run --manifest-path third_party/rust/libghostty-vt-sys/Cargo.toml --features bindgen-tool --bin gen-bindings
   ```

5. Recheck that C ABI code does not create Zig-owned workers or install a Zig signal stack,
   drop zz commits upstream now covers, and update the sys README, the UPSTREAM records, and
   the knowledge pages.

Sending a change upstream: `git format-patch <last import>..HEAD --relative=third_party/ghostty
-- third_party/ghostty` gives patches that apply to a Ghostty checkout; open the PR from a
branch on `demfabris/ghostty`.

Validate the actual archive and daemon: exported C symbols, terminal tests,
signal-handler/alternate-stack behavior, and a normal macOS bundle build with
no `GHOSTTY_SOURCE_DIR` or pkg-config bypass. Zig's default test runner uses its
own `std_options`, so its pass alone does not exercise this C ABI option. Cross-check
`cargo check -p zz-terminal --all-features --target <t>` for `aarch64-apple-ios` and
`x86_64-unknown-linux-gnu` from a Mac; `x86_64-pc-windows-msvc` needs a Windows host or CI,
because Zig has no MSVC C headers on a Mac.
