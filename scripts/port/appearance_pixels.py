#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from PIL import Image, ImageChops


ROOT = Path(__file__).resolve().parents[2]
THEMES = ("default-dark", "default-light", "macos-dark", "macos-light", "yaru-dark", "yaru-light")


def maximum_channel(image):
    red, green, blue = image.split()
    return ImageChops.lighter(ImageChops.lighter(red, green), blue)


def compare(reference, actual, tolerance):
    if reference.size != actual.size:
        raise ValueError(f"render dimensions disagree: {reference.size} != {actual.size}")
    delta = ImageChops.difference(reference, actual)
    histogram = maximum_channel(delta).histogram()
    different = sum(histogram[tolerance + 1:])
    ink_delta = ImageChops.difference(reference, Image.new("RGB", reference.size, reference.getpixel((0, 0))))
    ink = sum(maximum_channel(ink_delta).histogram()[tolerance + 1:])
    pixels = reference.width * reference.height
    absolute = sum(sum(index * count for index, count in enumerate(channel.histogram())) for channel in delta.split())
    return {"pixels": pixels, "different_pixels": different, "reference_ink_pixels": ink, "different_per_reference_ink": round(different / max(1, ink), 6), "matching_percent": round(100 * (1 - different / pixels), 6), "mean_channel_error": round(absolute / (pixels * 3), 6)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mjui", type=Path, default=ROOT.parent / "mjui")
    parser.add_argument("--chrome", default=shutil.which("google-chrome") or shutil.which("chromium"))
    parser.add_argument("--tolerance", type=int, default=3)
    parser.add_argument("--require-exact", action="store_true")
    args = parser.parse_args()
    if not args.chrome:
        raise SystemExit("Chrome or Chromium is required to render the penna reference")
    if not 0 <= args.tolerance <= 255:
        raise SystemExit("tolerance must be between zero and 255")
    directory = args.mjui / "build/penna-parity"
    results = []
    for theme in THEMES:
        reference_path = directory / (theme + "-penna.png")
        actual_path = directory / (theme + "-mjui.png")
        with tempfile.TemporaryDirectory(prefix="zz-parity-") as profile:
            subprocess.run([args.chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--no-default-browser-check", "--user-data-dir=" + profile, "--window-size=1004,588", "--force-device-scale-factor=1", "--virtual-time-budget=2000", "--screenshot=" + str(reference_path), (directory / (theme + ".html")).as_uri()], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=30, check=True)
        reference = Image.open(reference_path).convert("RGB")
        actual = Image.open(actual_path).convert("RGB")
        metrics = compare(reference, actual, args.tolerance)
        exact = ImageChops.difference(reference, actual).getbbox() is None
        results.append({"theme": theme, "exact": exact, **metrics})
        print(f'{theme}: {metrics["different_pixels"]} differing pixels; mean channel error {metrics["mean_channel_error"]}; exact={exact}', flush=True)
    report = {"reference": "penna DOM appearance fixtures", "renderer": "mjui CPU", "themes": results, "per_channel_tolerance": args.tolerance, "exact": all(row["exact"] for row in results), "scope": "eight direct appearance roles and seven states at scale1; native ala, other widgets, contextual grounds, GPU and other scales remain separate obligations"}
    target = ROOT / "target/port/appearance-pixels.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(report, indent=2) + "\n")
    (directory / "pixel-report.json").write_text(json.dumps(report, indent=2) + "\n")
    if args.require_exact and not report["exact"]:
        raise SystemExit("pixel identity is incomplete; see " + str(target))
    print("Rendering evidence recorded: " + str(target))


if __name__ == "__main__":
    main()
