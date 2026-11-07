# Third-party notice

zz is licensed `MIT OR Apache-2.0` ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

This file records what zz took from other projects, what it bundles, and what it only read.
Each entry says which of the three it is, because the obligations differ. Provenance lives next
to the code it describes; this is the index, and the linked files are the record.

## Forked and vendored source

Code that lives in this repository or is built from a pinned fork. These are the entries that
carry license obligations into zz's own builds.

| Project | What zz carries | License | Record |
| --- | --- | --- | --- |
| [gpui](https://github.com/zed-industries/zed) (Zed Industries) | The UI framework, split out of upstream and renamed ala, in `libs/ala` (subtree of [autistic-computer/ala](https://github.com/autistic-computer/ala)) with zz's changes on top. | Apache-2.0 | [`libs/ala/crates/ala/LICENSE-APACHE`](libs/ala/crates/ala/LICENSE-APACHE), [ala](knowledge/references/ala.md) |
| [gpui-component / gpui-kit](https://github.com/longbridge/gpui-kit) (Longbridge) | `libs/penna` (subtree of [autistic-computer/penna](https://github.com/autistic-computer/penna)), a full fork of the widget layer taken at `b004e595`. Upstream is no longer a dependency. `crates/zz/src/window/frame.rs` is adapted from its Linux client-side window border. | Apache-2.0, © 2024–2025 Longbridge | [`libs/penna/LICENSE-APACHE`](libs/penna/LICENSE-APACHE), per-module port notes in [`libs/penna/UPSTREAM.md`](libs/penna/UPSTREAM.md) |
| [Pele](https://github.com/obsidianmd/pele) (kepano and contributors) | `libs/pele` (subtree of [autistic-computer/pele-rs](https://github.com/autistic-computer/pele-rs)), a Rust port of the Mermaid renderer taken at `36e584d5`, which draws the agent chat's diagrams. | MIT, © 2026 kepano | [`libs/pele/LICENSE`](libs/pele/LICENSE), [`libs/pele/NOTICE`](libs/pele/NOTICE), [`libs/pele/UPSTREAM.md`](libs/pele/UPSTREAM.md) |
| [libghostty-rs](https://github.com/uzaaft/libghostty-rs) (Uzaaft) | Source snapshots of `libghostty-vt` and `libghostty-vt-sys` 0.2.1 from the `demfabris/libghostty-rs` fork, adapted to build the Zig 0.16 VT library. | MIT OR Apache-2.0 | [`third_party/rust/libghostty-vt/`](third_party/rust/libghostty-vt/UPSTREAM.md), [`third_party/rust/libghostty-vt-sys/`](third_party/rust/libghostty-vt-sys/UPSTREAM.md) |
| [Ghostty](https://github.com/ghostty-org/ghostty) (Mitchell Hashimoto and Ghostty contributors) | The VT state machine itself, compiled from the trimmed Ghostty source in `third_party/ghostty` through the snapshots above. Separately, `crates/zz-terminal/src/x11-rgb.txt` is copied from Ghostty's `src/terminal/res/rgb.txt` at `cf60af28`, which sources it from the X.Org `rgb` project. | MIT | [`third_party/ghostty/LICENSE`](third_party/ghostty/LICENSE), [`third_party/ghostty-reference/`](third_party/ghostty-reference/UPSTREAM.md) |

zz's own source carries no comments. The two notices that used to sit beside the code they
describe are kept here verbatim:

`crates/zz/src/window/frame.rs`

```
Adapted from gpui-component's Apache-2.0 Linux client-side window border:
https://github.com/longbridge/gpui-component/blob/b004e595cf5de98a73b6b561394a559a94ae1e2a/crates/zz-ui/src/window_border.rs
```

`crates/zz-terminal/src/appearance.rs` (`x11_colors`, which reads `x11-rgb.txt`)

```
Sourced from Ghostty's MIT/X11-licensed terminal/res/rgb.txt.
```

zz builds against the Apache-2.0 `gpui` and `gpui_platform` crates (now `ala` and `ala-platform`) and the upstream utility crates they need only. No GPL-licensed Zed
code is linked into any zz binary.

## Bundled binaries

| Project | What ships | License |
| --- | --- | --- |
| [Chromium Embedded Framework](https://bitbucket.org/chromiumembedded/cef) (Marshall A. Greenblatt, portions © Google) | Browser panes. Release builds bundle a pinned CEF minimal distribution, verified by SHA-1 against CEF's published index before extraction. | BSD-3-Clause, retained at [`third_party/cef/LICENSE.txt`](third_party/cef/LICENSE.txt). The distribution carries Chromium's own licenses, which are bundled with it. |
| [cef-rs](https://github.com/tauri-apps/cef-rs) (Tauri) | The Rust bindings over CEF, as a normal crate dependency. | MIT OR Apache-2.0 |

Pinned artifacts, target matrix, and hashes: [`third_party/cef/ARTIFACTS.md`](third_party/cef/ARTIFACTS.md).

## Rust dependencies

Around 1,085 package versions resolve into the workspace graph across every platform and
feature. They are not listed individually here; the lockfiles are the record, and the current
spread is reproducible:

```bash
cargo metadata --format-version 1 --all-features \
  | jq -r '.packages[] | "\(.license // "UNKNOWN")\t\(.name) \(.version)"' | sort -u
```

The distribution is overwhelmingly permissive: 952 of the 1,085 are `MIT`, `Apache-2.0`, or a
dual of the two, with smaller groups under `Unicode-3.0`, `BSD-2/3-Clause`, `ISC`, `Zlib`,
`0BSD`, `Unlicense`, and `CC0-1.0`. Seven packages are MPL-2.0 (`cbindgen`, `cssparser`,
`cssparser-macros`, `dtoa-short`, `dwrote`, `option-ext`, `selectors`), all reached through
ala and all used unmodified, so MPL's file-level reciprocity is satisfied by leaving them
alone. Nothing in the graph is GPL or AGPL.

A few dependencies are worth naming because they come from neighboring projects rather than
from the general crate ecosystem:

| Crate | Source | License |
| --- | --- | --- |
| `portable-pty` | [wezterm](https://github.com/wezterm/wezterm) | MIT |
| `agent-client-protocol` | [Agent Client Protocol Rust SDK](https://github.com/agentclientprotocol/rust-sdk) | Apache-2.0 |
| `wgpu` | [gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) | MIT OR Apache-2.0 |

## Fonts

All bundled fonts are under the SIL Open Font License 1.1, with the full text retained beside
each face.

| Font | Ships in | Copyright |
| --- | --- | --- |
| [Lilex](https://github.com/mishamyrt/Lilex) | Browser and ala iOS clients | The Lilex Project Authors |
| [Inter](https://github.com/rsms/inter) | Browser client | The Inter Project Authors |
| [Fantasque Sans Mono](https://github.com/belluzj/fantasque-sans) | zzmux.sh | Jany Belluz |

The desktop clients ship no fonts; they render with the system text stack.

## Icons

| Set | Used for | License |
| --- | --- | --- |
| [Solar Icons Set](https://www.figma.com/community/file/1166831539721848736) by 480 Design | Every glyph in the app and the site except the brand marks: `libs/penna/src/components/icon/solar.json` and `site/src/icons`, adapted to follow the theme radius. Icons Solar lacks are zz's own, drawn in its style. | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Credit and changes in `LICENSE-SOLAR` beside the set. |
| [Tabler Icons](https://tabler.io/icons) | The Chrome brand mark, `crates/zz-ui/assets/icons/brand-chrome.svg`. | MIT, © 2020–2026 Paweł Kuna. Retained as `LICENSE-TABLER` beside it. |
| [Simple Icons](https://simpleicons.org) | The vendor brand marks `crates/zz-ui/assets/icons/openai.svg` and `claude.svg`. | CC0-1.0 |

## Interface themes

The `macos` and `yaru` interface themes in `libs/penna/themes` are named for the platforms whose
feel they imitate. zz is not affiliated with Apple or the Ubuntu project. Their colors are the
macOS system colors as `NSColor` reports them, Yaru's palette and zz's own picks; no artwork or
code comes from either.

## Protocols and formats

zz speaks these; it ships none of their implementations.

- **tmux command language, control mode, and key tables.** zz's multiplexer is a Rust
  implementation checked against the tmux 3.8 tag (`7f2a35ad`). See the reference section below.
- **[Agent Client Protocol](https://agentclientprotocol.com)** for agent panes, through the
  Apache-2.0 Rust SDK.
- **Claude Code's peer bus.** The daemon speaks the wire format so agent panes register as
  peers and `agent-send` reaches a Claude Code session. None of Anthropic's code is copied, and
  the daemon hosts no vendor's agent loop.
- **Google Chrome's on-disk profile, cookie, and history formats**, read-only, in
  `libs/ala-cef/src/chrome_import`. Reading the format required the platform keychain and DPAPI
  storage-key handling; no Chrome code is used.
- **The Kitty graphics protocol** ([kovidgoyal/kitty](https://github.com/kovidgoyal/kitty)) and
  **OSC 52**, both implemented by the VT engine.

## Reference material

Read, pinned, and cited. None of it is compiled, linked, or shipped.

| Project | What it settles | License |
| --- | --- | --- |
| [tmux](https://github.com/tmux/tmux) | Command names, aliases, key-table behavior, format strings, and config syntax, pinned at the 3.8 tag (`7f2a35ad`). No tmux C source is copied into the Rust implementation. | ISC, retained at [`third_party/tmux-reference/`](third_party/tmux-reference/UPSTREAM.md) |
| tmux plugin corpus: [tpm](https://github.com/tmux-plugins/tpm), [tmux-sensible](https://github.com/tmux-plugins/tmux-sensible), [vim-tmux-navigator](https://github.com/christoomey/vim-tmux-navigator), [tmux-yank](https://github.com/tmux-plugins/tmux-yank), [tmux-resurrect](https://github.com/tmux-plugins/tmux-resurrect), [tmux-continuum](https://github.com/tmux-plugins/tmux-continuum), [tmux-fpp](https://github.com/tmux-plugins/tmux-fpp), [Oh My Tmux](https://github.com/gpakosz/.tmux) | The alias compatibility suite runs real plugin initialization against zz at immutable revisions. | MIT, except Oh My Tmux which is MIT and WTFPLv2. Originals retained in [`third_party/tmux-plugin-corpus/`](third_party/tmux-plugin-corpus/UPSTREAM.md) |
| [Ghostty](https://github.com/ghostty-org/ghostty), [kitty](https://github.com/kovidgoyal/kitty), [Alacritty](https://github.com/alacritty/alacritty), [cmux](https://github.com/manaflow-ai/cmux) | Throughput and rendering baselines in `bench/`. | Respective upstream licenses |
| [DOOM-fire-zig](https://github.com/const-void/DOOM-fire-zig) | One of the benchmark fixtures, cloned at a pinned revision into a gitignored cache by `bench/gen-fixtures.sh`. | Upstream license |

## Prior art

Projects zz studied and wrote down before building the equivalent. No code was taken from any
of them. These acknowledgments record the sources of the ideas.

- [zenbu-labs/terminal-browser](https://github.com/zenbu-labs/terminal-browser) validated
  putting a real browser in a PTY, and its damage-driven frame transport shaped the TUI
  client's design.
- [anomalyco/opencode](https://github.com/anomalyco/opencode) informed the agent-harness
  rendering work.
- [herdrdev/herdr](https://github.com/herdrdev/herdr) informed the agent-to-agent messaging work.
- [cmux](https://github.com/manaflow-ai/cmux) is the source of the pane drag-and-drop model zz
  measured its own against.

## Trademarks

tmux, Ghostty, Zed, Chromium, Google Chrome, Claude, Claude Code, OpenAI, and the other product
names here belong to their respective owners and are used only to say what zz is compatible
with or built on. zz is not affiliated with, endorsed by, or sponsored by any of them.

## Corrections

If something here is wrong, missing, or attributes your work incorrectly, open an issue at
<https://github.com/autistic-computer/zebrilus/issues> and it gets fixed.

## Ported path matcher

The path matcher in `mojo/zzmojo/path_case.mojo`, `path_pattern.mojo`, `path_score.mojo` and `path_cpu.mojo`, together with the source oracle in `crates/zz-client/examples/path_rank_vectors.rs`, is adapted from [neo_frizbee 0.13.3](https://github.com/saghen/frizbee), copyright 2024 Liam Dyer, under the MIT License. Its license is reproduced below.

```
MIT License

Copyright (c) 2024 Liam Dyer

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The pivot sampling, cyclic partition and heap fallback in `mojo/zzmojo/path_rank.mojo`, the zero-key SipHash-1-3 implementation in `mojo/zzmojo/key_table_hash.mojo`, and the slice partition-point compatibility routine in `mojo/zzmojo/desktop_key_tables.mojo` are adapted from the Rust standard library, copyright The Rust Project Contributors, available under the MIT License reproduced below.

```
MIT License

Copyright (c) The Rust Project Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

`mojo/zzmojo/path_case_data.mojo` contains Unicode character properties and case mappings generated through the native Rust Unicode API. The Rust Unicode data attribution names copyright 1991–2024 Unicode, Inc.; the Unicode License V3 notice is reproduced below.

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```

The Unicode character properties and case-context masks in `mojo/zzmojo/which_key_caps_data.mojo` are generated through the native Rust Unicode API. The Unicode License V3 notice above also applies to these data.
