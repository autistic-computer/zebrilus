# Browser client fonts

The browser has no access to host fonts, so the client bundles every face it renders.

| File | Face | License |
| --- | --- | --- |
| `lilex/Lilex-Regular.ttf` | Lilex regular | [`lilex/LICENSE.txt`](lilex/LICENSE.txt) |
| `lilex/Lilex-Bold.ttf` | Lilex bold | [`lilex/LICENSE.txt`](lilex/LICENSE.txt) |
| `lilex/Lilex-Italic.ttf` | Lilex italic | [`lilex/LICENSE.txt`](lilex/LICENSE.txt) |
| `lilex/Lilex-BoldItalic.ttf` | Lilex bold italic | [`lilex/LICENSE.txt`](lilex/LICENSE.txt) |

Lilex comes from [mishamyrt/Lilex](https://github.com/mishamyrt/Lilex), under the SIL Open Font
License 1.1. Chrome text uses Plumula Sans and chat prose Plumula Serif, both loaded from
`crates/zz-ui/assets/fonts/plumula` by `zz_ui::fonts::plumula`. Terminals, code, and command
output use Lilex regardless of the font families the daemon's terminal appearance names, because
those are host fonts. The client ships no CJK or emoji fallback; unsupported glyphs render as
missing glyphs.
