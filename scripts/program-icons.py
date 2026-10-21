#!/usr/bin/env python3
"""Build zz's program icons from an icon bundle (SVGs plus a colors.json).

Usage: scripts/program-icons.py <svg-colors dir> <indexed-commands.json>

Writes crates/zz-ui/assets/programs/<id>.svgz (svgo-minified, square viewBox,
gzipped; usvg reads svgz on every platform) and the catalog the client matches
commands against, crates/zz-client/src/program/catalog.rs. Needs npx for svgo.
"""

from __future__ import annotations

import argparse
import gzip
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "crates/zz-ui/assets/programs"
CATALOG = ROOT / "crates/zz-client/src/program/catalog.rs"
SVGO = "svgo@4.0.0"
SIZE = 64
SKIP = {"bash", "zsh", "fish", "nushell", "fd", "rex"}
REDRAWN = {
    "terminal": ("Terminal", "terminal"),
    "log": ("log", "list"),
    "sleep": ("sleep", "moon"),
    "top": ("top", "cpu"),
    "caffeinate": ("caffeinate", "cup"),
    "gitchanges": ("Git changes", "git-branch"),
    "inspector": ("Inspector", "inspector"),
    "browser": ("Browser", "globe"),
}
VIEWBOXES = {"pytest": "500 171 474 542", "kind": "12.4 34.3 260.5 157.2"}
ART_BACKGROUND = "#F9F9F9"
ART_BACKGROUNDS: dict[str, str] = {}
DARK_INK = "#111111"
SVGO_CONFIG = """export default {
  multipass: true,
  floatPrecision: 2,
  plugins: [
    "preset-default",
    "removeDimensions",
    "removeTitle",
  ],
};
"""
P3_TO_SRGB = (
    (1.2249401, -0.2249404, 0.0),
    (-0.0420569, 1.0420571, 0.0),
    (-0.0196376, -0.0786361, 1.0982735),
)
PAINT = re.compile(r"(?:fill|stroke|stop-color)\s*[=:]\s*\"?\s*([^\";}\s]+)")


def linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def encode(c: float) -> float:
    c = min(1.0, max(0.0, c))
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


def hex_of(rgb: tuple[float, float, float]) -> str:
    return "#" + "".join(f"{round(c * 255):02X}" for c in rgb)


def rgb_of(hex_color: str) -> tuple[float, float, float]:
    value = hex_color.lstrip("#")
    return tuple(int(value[i : i + 2], 16) / 255 for i in (0, 2, 4))


def p3_to_srgb(rgb: tuple[float, float, float]) -> tuple[float, float, float]:
    lin = [linear(c) for c in rgb]
    return tuple(encode(sum(m * c for m, c in zip(row, lin))) for row in P3_TO_SRGB)


def srgb(hex_color: str, space: str | None) -> str:
    rgb = rgb_of(hex_color)
    return hex_of(p3_to_srgb(rgb) if space == "display-p3" else rgb)


def luminance(hex_color: str) -> float:
    r, g, b = (linear(c) for c in rgb_of(hex_color))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: str, b: str) -> float:
    hi, lo = sorted((luminance(a), luminance(b)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


def ink_on(background: str) -> str:
    return max(("#FFFFFF", DARK_INK), key=lambda ink: contrast(ink, background))


def p3_colors(svg: str) -> str:
    def replace(match: re.Match[str]) -> str:
        return hex_of(p3_to_srgb(tuple(float(c) for c in match.groups())))

    return re.sub(r"color\(display-p3 ([\d.]+) ([\d.]+) ([\d.]+)\)", replace, svg)


def prepare(entry_id: str, svg: str) -> str:
    svg = re.sub(r"<text\b.*?</text>", "", p3_colors(svg), flags=re.S)
    if entry_id in VIEWBOXES:
        svg = re.sub(r'viewBox="[^"]+"', f'viewBox="{VIEWBOXES[entry_id]}"', svg, count=1)
    return svg


def painted(svg: str) -> bool:
    return any(
        paint.lower() not in {"none", "currentcolor"} for paint in PAINT.findall(svg)
    )


def square(svg: str) -> str:
    match = re.search(r'viewBox="([^"]+)"', svg)
    if not match:
        raise SystemExit("svg without a viewBox")
    x, y, w, h = (float(v) for v in re.split(r"[\s,]+", match.group(1).strip()))
    side = max(w, h)
    x -= (side - w) / 2
    y -= (side - h) / 2
    box = " ".join(f"{v:g}" for v in (x, y, side, side))
    return svg.replace(
        match.group(0), f'width="{SIZE}" height="{SIZE}" viewBox="{box}"', 1
    )


def look(entry: dict, svg: str) -> tuple[str, str, str | None]:
    colors = entry["colors"]
    if entry["origin"] == "rex":
        background = srgb(colors["background"], colors.get("backgroundSpace"))
        foreground = srgb(colors["foreground"], colors.get("foregroundSpace"))
        return "Mark", background, foreground
    primary = colors.get("primary")
    if primary and not painted(svg):
        background = srgb(primary, colors.get("space"))
        return "Mark", background, ink_on(background)
    return "Art", ART_BACKGROUNDS.get(entry["id"], ART_BACKGROUND), None


def commands(entry: dict, indexed: dict[str, list[str]]) -> list[str]:
    if entry["origin"] == "rex":
        return indexed.get(entry["id"], [])
    return entry.get("commands", [])


def rust_color(hex_color: str) -> str:
    value = "00" + hex_color.lstrip("#").upper()
    return f"0x{value[:4]}_{value[4:]}"


def rust_entry(entry: dict, keys: list[str], kind: str, bg: str, fg: str | None) -> str:
    look = (
        f"Look::Mark {{ background: {rust_color(bg)}, foreground: {rust_color(fg)} }}"
        if kind == "Mark"
        else f"Look::Art {{ background: {rust_color(bg)} }}"
    )
    name, icon = REDRAWN.get(entry["id"], (entry["name"], None))
    icon = f"Some({json.dumps(icon)})" if icon else "None"
    return (
        f"    Program {{ id: {json.dumps(entry['id'])}, name: {json.dumps(name)}, "
        f"commands: &[{', '.join(json.dumps(k) for k in keys)}], icon: {icon}, look: {look} }},"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("bundle", type=Path)
    parser.add_argument("indexed", type=Path)
    args = parser.parse_args()

    entries = json.loads((args.bundle / "colors.json").read_text())
    entries = sorted(
        (e for e in entries if e["id"] not in SKIP),
        key=lambda e: (e["origin"] != "rex", e["id"]),
    )
    indexed: dict[str, list[str]] = {}
    for row in json.loads(args.indexed.read_text())["entries"]:
        indexed.setdefault(row["glyphID"], []).append(row["match"].lower())

    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        raw, out = scratch / "raw", scratch / "out"
        raw.mkdir()
        sources = {}
        for entry in entries:
            svg = prepare(entry["id"], (args.bundle / entry["svg"]).read_text())
            sources[entry["id"]] = svg
            (raw / f"{entry['id']}.svg").write_text(svg)
        config = scratch / "svgo.config.mjs"
        config.write_text(SVGO_CONFIG)
        subprocess.run(
            ["npx", "-y", SVGO, "--quiet", "--config", str(config), "-f", str(raw), "-o", str(out)],
            check=True,
        )

        if ASSETS.exists():
            shutil.rmtree(ASSETS)
        ASSETS.mkdir(parents=True)
        claimed: dict[str, str] = {}
        rows = []
        raw_bytes = packed_bytes = 0
        for entry in entries:
            keys = [k for k in commands(entry, indexed) if claimed.setdefault(k, entry["id"]) == entry["id"]]
            if entry["id"] not in REDRAWN:
                svg = square((out / f"{entry['id']}.svg").read_text().strip())
                data = gzip.compress(svg.encode(), compresslevel=9, mtime=0)
                (ASSETS / f"{entry['id']}.svgz").write_bytes(data)
                raw_bytes += len(sources[entry["id"]].encode())
                packed_bytes += len(data)
            kind, bg, fg = look(entry, sources[entry["id"]])
            rows.append(rust_entry(entry, keys, kind, bg, fg))

    licenses = args.bundle / "licenses/build"
    if licenses.is_dir():
        shutil.copytree(licenses, ASSETS / "licenses")

    CATALOG.parent.mkdir(parents=True, exist_ok=True)
    CATALOG.write_text(
        "use super::{Look, Program};\n\npub(super) const PROGRAMS: &[Program] = &[\n"
        + "\n".join(rows)
        + "\n];\n"
    )
    subprocess.run(["rustfmt", "--edition", "2024", str(CATALOG)], check=True)
    print(f"{len(rows)} icons: {raw_bytes / 1024:.0f} KiB of SVG -> {packed_bytes / 1024:.0f} KiB svgz")


if __name__ == "__main__":
    main()
