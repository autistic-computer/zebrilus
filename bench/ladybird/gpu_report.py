#!/usr/bin/env python3
import argparse
import gzip
import itertools
import json
from collections import Counter
from pathlib import Path

from baseline_report import load_sample, measurements, middle, paired, table

ARMS = {
    "cpu": "zz CPU",
    "gpu-readback": "zz GPU + bitmap",
    "dma-buf": "zz DMA-BUF",
    "qt-gpu": "Stock Qt GPU",
    "cef": "zz CEF",
}


def presented(run):
    p = run["presentation"]
    engine = p["engine"]
    samples = p["samples_pts_ns_index"]
    unique = {}
    for timestamp, idx in samples:
        if idx >= 0:
            unique.setdefault(idx, timestamp)
    if (
        len(unique) < 2
        or len(unique) != p["unique"]
        or engine["tainted"]
        or engine["frames"] != len(engine["intervals_ms"])
    ):
        raise ValueError("Incomplete display evidence")
    times = list(unique.values())
    gaps = sorted((b - a) / 1e6 for a, b in itertools.pairwise(times))
    return {
        "engine": engine["engine_fps"],
        "fps": 1000 * len(unique) / engine["wall_ms"],
        "coverage": 100 * len(unique) / engine["frames"],
        "p95": gaps[(95 * len(gaps) - 1) // 100],
        "maximum": max(gaps),
        "interior_grey": sum(
            idx == -1 and times[0] < t < times[-1] for t, idx in samples
        ),
    }


def groups(data):
    meta = data["environment"]
    modes = set(meta["modes"])
    if modes not in (ARMS.keys() - {"cef"}, {"dma-buf", "cef"}) or meta["blocks"] <= 0:
        raise ValueError("Unexpected comparison arms or block count")
    expected_sizes = {
        tuple(map(int, k[3:].split("x"))) for k in meta["outer"] if k.startswith("zz-")
    }
    result = {s: {} for s in sorted(expected_sizes)}
    warmups = Counter()
    for row in data["blocks"]:
        size = tuple(row["size"])
        idx = row["block"]
        if (
            size not in result
            or set(row["runs"]) != modes
            or sorted(row["order"]) != sorted(modes)
        ):
            raise ValueError("Incomplete comparison block")
        optical = "presentation_seconds" in meta
        if not optical:
            swap = max(
                sum(v[1] for v in snap.values())
                for run in row["runs"].values()
                for phase in run["phases"].values()
                for snap in phase["memory_kib"]
            )
            if row["max_swap_pss_kib"] != swap or row["accepted"] != (
                idx > 0 and swap <= 1024
            ):
                raise ValueError("Swap admission mismatch")
        elif row["accepted"] != (idx > 0):
            raise ValueError("Invalid presentation block")
        for arm, run in row["runs"].items():
            if run["mode"] != arm or run["ready"]["geometry"] != [*size, 1, "visible"]:
                raise ValueError("Viewport mismatch")
            if optical:
                presented(run)
                continue
            for phase in ("canvas", "scroll"):
                r = run["phases"][phase]
                if (
                    r["callbacks"] != meta["frames"]
                    or len(r["callback_intervals_ms"]) != meta["frames"]
                ):
                    raise ValueError("Incomplete fixed-work phase")
            if idx == 0 and (
                run["canvas_pixel"]["rgb"] != [38, 57, 71]
                or run["scroll_pixel"]["rgb"] != [216, 79, 131]
            ):
                raise ValueError("Missing compositor pixel evidence")
        if row["warmup"] != (idx == 0):
            raise ValueError("Warm-up mismatch")
        if idx == 0:
            warmups[size] += 1
        if row["accepted"]:
            if idx in result[size]:
                raise ValueError("Duplicate block")
            result[size][idx] = row["runs"]
    if any(
        warmups[s] != 1 or set(r) != set(range(1, meta["blocks"] + 1))
        for s, r in result.items()
    ):
        raise ValueError("Missing warm-up or accepted block")
    return result


def report(data):
    meta = data["environment"]
    optical = "presentation_seconds" in meta
    blocks = groups(data)
    name = "Composited scrolling" if optical else "Resource use"
    excluded = sum(not r["accepted"] and not r["warmup"] for r in data["blocks"])
    subject = (
        "in the same zz desktop with CEF and Ladybird"
        if "cef" in meta["modes"]
        else "with four Ladybird configurations"
    )
    text = [
        f"# {name} {subject}",
        f"{meta['utc']} · {meta['cpu']} · {', '.join(meta['gpu'])} · {meta['kwin_version']}.",
        f"{meta['blocks']} measured blocks and one discarded warm-up per viewport. {excluded} blocks rejected. Table entries are medians. Paired differences include the range across blocks.",
    ]
    for size, items in blocks.items():
        text.append(f"## {size[0]} × {size[1]} CSS pixels")
        runs = {
            arm: [block[arm] for _, block in sorted(items.items())]
            for arm in meta["modes"]
        }
        if optical:
            measures = {a: [presented(r) for r in v] for a, v in runs.items()}
            cols = ("engine", "fps", "coverage", "p95", "maximum", "interior_grey")
            text.append(
                table(
                    [
                        "Arm",
                        "Page callbacks/s",
                        "Distinct composited frames/s",
                        "Callbacks seen (%)",
                        "P95 gap (ms)",
                        "Longest gap (ms)",
                        "Interior grey samples",
                    ],
                    [
                        [ARMS[a], *(middle(v, k) for k in cols)]
                        for a, v in measures.items()
                    ],
                )
            )
            text.append(
                table(
                    [
                        "Paired difference",
                        "Distinct composited frames/s",
                        "P95 gap (ms)",
                    ],
                    [
                        [
                            f"DMA-BUF minus {ARMS[a]}",
                            *(
                                paired(measures["dma-buf"], measures[a], k, 2)
                                for k in ("fps", "p95")
                            ),
                        ]
                        for a in runs
                        if a != "dma-buf"
                    ],
                )
            )
            continue
        warm = next(
            r["runs"]
            for r in data["blocks"]
            if r["warmup"] and tuple(r["size"]) == size
        )
        text.append(
            table(
                [
                    "Arm",
                    "Initial canvas at 1.5 s",
                    "Canvas after animation",
                    "Scrolled page",
                ],
                [
                    [
                        ARMS[a],
                        "Visible"
                        if warm[a]["initial_canvas_visible"]
                        else "Marker missing",
                        "Verified",
                        "Verified",
                    ]
                    for a in runs
                ],
            )
        )
        for phase in ("idle", "canvas", "scroll"):
            text.append(f"### {phase.capitalize()}")
            measures = {}
            for a, values in runs.items():
                measures[a] = []
                for r in values:
                    p = r["phases"][phase]
                    m = measurements(p)
                    vram = p.get("gpu_memory_mib")
                    if vram is not None:
                        m["gpu_memory"] = sum(vram.values())
                    if "terminal" in r["phases"]:
                        m["pane_pss"] = (
                            m["pss_total"]
                            - measurements(r["phases"]["terminal"])["pss_total"]
                        )
                    measures[a].append(m)
            if phase == "idle":
                text.append(
                    table(
                        [
                            "Arm",
                            "App PSS MiB",
                            "Pane added PSS MiB",
                            "Host PSS MiB",
                            "Engine PSS MiB",
                            "Driver GPU MiB",
                            "CPU % of a core",
                        ],
                        [
                            [
                                ARMS[a],
                                middle(v, "pss_total", 1),
                                middle(v, "pane_pss", 1) if a != "qt-gpu" else "—",
                                middle(v, "pss_host", 1),
                                middle(v, "pss_engine", 1),
                                middle(v, "gpu_memory", 1)
                                if "gpu_memory" in v[0]
                                else "—",
                                middle(v, "idle_total"),
                            ]
                            for a, v in measures.items()
                        ],
                    )
                )
                text.append(
                    table(
                        [
                            "Paired difference",
                            "App PSS MiB",
                            "Added pane PSS MiB",
                            "Driver GPU MiB",
                        ],
                        [
                            [
                                f"DMA-BUF minus {ARMS[a]}",
                                paired(measures["dma-buf"], v, "pss_total", 1),
                                paired(measures["dma-buf"], v, "pane_pss", 1)
                                if a != "qt-gpu"
                                else "—",
                                paired(measures["dma-buf"], v, "gpu_memory", 1)
                                if "gpu_memory" in v[0]
                                else "—",
                            ]
                            for a, v in measures.items()
                            if a != "dma-buf"
                        ],
                    )
                )
                continue
            text.append(
                table(
                    [
                        "Arm",
                        "App CPU s",
                        "Host CPU s",
                        "Engine CPU s",
                        "KWin CPU s",
                        "Host reads MiB",
                        "Callbacks/s",
                        "Driver GPU MiB",
                    ],
                    [
                        [
                            ARMS[a],
                            *(
                                middle(v, k)
                                for k in (
                                    "cpu_total",
                                    "cpu_host",
                                    "cpu_engine",
                                    "kwin_cpu_s",
                                    "host_read_mib",
                                    "callback_hz",
                                )
                            ),
                            middle(v, "gpu_memory", 1) if "gpu_memory" in v[0] else "—",
                        ]
                        for a, v in measures.items()
                    ],
                )
            )
            text.append(
                table(
                    ["Paired difference", "App CPU s", "Host CPU s", "Engine CPU s"],
                    [
                        [
                            f"DMA-BUF minus {ARMS[a]}",
                            *(
                                paired(measures["dma-buf"], measures[a], k, 2)
                                for k in ("cpu_total", "cpu_host", "cpu_engine")
                            ),
                        ]
                        for a in runs
                        if a != "dma-buf"
                    ],
                )
            )
    return "\n\n".join(text) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("sources", nargs="+", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    campaigns = []
    for source in args.sources:
        data = load_sample(source)
        campaigns.extend(data.get("campaigns", [data]))
    summary = "\n---\n\n".join(report(campaign) for campaign in campaigns)
    data = campaigns[0] if len(campaigns) == 1 else {"campaigns": campaigns}
    if args.archive:
        args.archive.write_bytes(
            gzip.compress(json.dumps(data, separators=(",", ":")).encode(), mtime=0)
        )
    if args.output:
        args.output.write_text(summary)
    else:
        print(summary, end="")


if __name__ == "__main__":
    main()
