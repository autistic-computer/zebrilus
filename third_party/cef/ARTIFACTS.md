# CEF artifact lock

zz resolves `cef` and `cef-dll-sys` to `156.3.0+156.0.15` in `Cargo.lock`.
That release maps to CEF `156.0.15+gcf671c0+chromium-156.0.8078.80`.
`download-cef` verifies the selected minimal distribution against the SHA-1
published in CEF's official `index.json` before extracting it.

| Rust target | Minimal distribution | SHA-1 |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_linux64_minimal.tar.bz2` | `09235427222e026a4f34399742df27c5d98a4f8a` |
| `aarch64-unknown-linux-gnu` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_linuxarm64_minimal.tar.bz2` | `85602db8e813ad824b45ae54218476aa3d91d561` |
| `arm-unknown-linux-gnueabi` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_linuxarm_minimal.tar.bz2` | `1b62aa629b0a64ee72d2f65bd706e63694c2054b` |
| `x86_64-apple-darwin` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_macosx64_minimal.tar.bz2` | `13756e3029307720fe00898d54aef8a64c9b4043` |
| `aarch64-apple-darwin` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_macosarm64_minimal.tar.bz2` | `bcdd1877edb300b0fb91d8f71c0b14aae256cd0f` |
| `x86_64-pc-windows-msvc` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_windows64_minimal.tar.bz2` | `b690e035689ee20ba9d2500ac8953815bc7ed46a` |
| `aarch64-pc-windows-msvc` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_windowsarm64_minimal.tar.bz2` | `446515cfaed802200749083ea64f8718e885c91f` |
| `i686-pc-windows-msvc` | `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_windows32_minimal.tar.bz2` | `cb5aa9ec84777d3722ce0dacd9f07bf270592f93` |

The values above were read from `https://cef-builds.spotifycdn.com/index.json`
on 2026-11-04. The index lists 156.0.15 once per platform, as a stable build. This table is a reviewable mirror; `download-cef` fetches and
verifies against the official index directly. When updating CEF, refresh the
Cargo lock, this table, the CI cache key, and all three platform bundle
checks together.
