---
name: design-board
description: Draw, change or show a zz design board (an HTML mockup under design/mockups painted with the real theme). Use whenever you write or edit a page under design/mockups, or the user asks to see, draw, board or mock up a screen before it is built. Covers serving the page, opening it in the Browser pane, the link to give, and the PNG captures.
---

# Design boards

A board is an HTML page under `design/mockups/<question>/` that paints with zz's tokens through penna's HTML kit (`design/README.md`). Its stylesheets are absolute paths (`/penna/penna.css`, `/zz.css`, `/tokens/`) that only the design server answers. Opened as a file, a board loads none of them and shows bare HTML. Never point the user at a board's file path.

## Every time you write or change a board

1. Make sure the server is up: `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8090/` prints 200. If not, start it with the Browser pane's `preview_start` and `name: "design"` (`.claude/launch.json` runs `just design serve` on 8090). Never start it with Bash.
2. Open the board in the Browser pane at its served URL: `preview_start` with `url: "http://127.0.0.1:8090/mockups/<question>/<board>.html"`, or `navigate` an open tab there. Writing an HTML file can open it in the pane as a file; that view is broken, so always replace it with the served URL.
3. Check it rendered: one screenshot at `scale: 0.5`. A white page with plain text means the kit did not load.
4. Capture a PNG per take with headless Chrome and send them with `SendUserFile`, since the user often follows from another device:

   ```bash
   "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --hide-scrollbars --force-device-scale-factor=1 --window-size=940,1130 --virtual-time-budget=15000 --screenshot="$OUT/<take>.png" "http://127.0.0.1:8090/mockups/<question>/<board>.html?only=<take>&bare&mode=dark"
   ```

   `$OUT` is the scratchpad. `?only=<take>` works when the board wraps each take in `<section data-take="<take>">` and ends its script with the `only` filter (copy it from `design/mockups/phone/machines-tiles.html`). `bare` hides the theme switcher, `mode=dark` or `mode=light` picks the mode. Keep `--virtual-time-budget` at 15000 or the capture can fire before the styles load. Look at every PNG before sending it.
5. In the reply, give the served URL as a markdown link, for example [Machines tiles](http://127.0.0.1:8090/mockups/phone/machines-tiles.html), and say it is open in the Browser pane. Name the file path only as where the source lives.

## Drawing one

- Start from the newest board in the same folder; reuse its phone frame, status bar and helpers (`design/mockups/phone/phone.css` for the phone).
- Draw three to five takes on one scenario, states drawn statically, each with a one-line caption. End with a side-by-side table.
- After the user picks, the final board carries a numbers table the build copies.
