#!/usr/bin/env python3
import argparse
import gzip
import json
from pathlib import Path
from statistics import median

MODES = {"headless": "Headless Hub", "qt": "Stock Qt", "zz": "zz"}
GROUPS = ("total", "host", "engine", "support")


def load_sample(path):
    if path.is_dir():
        restarts = path / "resumptions.jsonl"
        return {
            "resumptions": [json.loads(s) for s in restarts.read_text().splitlines()]
            if restarts.exists()
            else [],
            "environment": json.loads((path / "environment.json").read_text()),
            "blocks": [
                json.loads(s) for s in (path / "blocks.jsonl").read_text().splitlines()
            ],
        }
    with gzip.open(path, "rt") as source:
        return json.load(source)


def accepted_blocks(data):
    meta = data["environment"]
    sizes = {
        tuple(map(int, name.removeprefix("zz-").split("x")))
        for name in meta["outer"]
        if name.startswith("zz-")
    }
    groups = {s: {} for s in sorted(sizes)}
    warmups = set()
    for row in data["blocks"]:
        size = tuple(row["size"])
        if (
            size not in groups
            or sorted(row["order"]) != sorted(MODES)
            or set(row["runs"]) != MODES.keys()
        ):
            raise ValueError(f"Incomplete arms for {size}, block {row['block']}")
        if row["warmup"]:
            if row["block"] != 0 or size in warmups:
                raise ValueError(f"Unexpected warm-up for {size}")
            warmups.add(size)
        for mode, run in row["runs"].items():
            expected = {"canvas", "scroll", "idle", "integer", "layout"} | (
                {"terminal"} if mode == "zz" else set()
            )
            if (
                run["mode"] != mode
                or run["ready"]["geometry"] != [*size, 1, "visible"]
                or run["phases"].keys() != expected
            ):
                raise ValueError(f"Incomplete {mode} trial for {size}")
            if any(
                run["phases"][phase]["callbacks"] != meta["frames"]
                or len(run["phases"][phase]["callback_intervals_ms"]) != meta["frames"]
                for phase in ("canvas", "scroll")
            ):
                raise ValueError("Incomplete animation workload")
            if (
                row["warmup"]
                and mode != "headless"
                and (
                    run["canvas_pixel"]["rgb"] != [38, 57, 71]
                    or run["scroll_pixel"]["rgb"] != [216, 79, 131]
                    or run["scroll_pixel"]["point"] != run["canvas_pixel"]["point"]
                )
            ):
                raise ValueError(f"Missing compositor proof for {mode} at {size}")
        swap = max(
            sum(v[1] for v in sample.values())
            for run in row["runs"].values()
            for phase in run["phases"].values()
            for sample in phase["memory_kib"]
        )
        if row["max_swap_pss_kib"] != swap or row["accepted"] != (
            row["block"] > 0 and swap <= 1024
        ):
            raise ValueError(
                f"Swap or acceptance mismatch for {size}, block {row['block']}"
            )
        if not row["accepted"]:
            continue
        if row["warmup"] or row["block"] in groups[size]:
            raise ValueError(f"Invalid accepted block {row['block']} for {size}")
        groups[size][row["block"]] = row["runs"]
    if (
        not groups
        or warmups != sizes
        or not meta["blocks"]
        or any(
            set(group) != set(range(1, meta["blocks"] + 1)) for group in groups.values()
        )
    ):
        raise ValueError(
            f"Expected {meta['blocks']} complete measured blocks per viewport"
        )
    return groups


def summed(values, group):
    return sum(
        value
        for role, value in values.items()
        if group == "total" or role == group or role.startswith(group + "/")
    )


def measurements(phase):
    values = {}
    for group in GROUPS:
        samples = [
            summed({k: v[0] for k, v in snap.items()}, group) / 1024
            for snap in phase["memory_kib"]
        ]
        cpu = summed(phase["cpu_s"], group)
        values["pss_" + group] = median(samples)
        values["peak_" + group] = max(samples)
        values["cpu_" + group] = cpu
        values["idle_" + group] = 100 * cpu / phase["wall_s"]
    values.update(
        {
            k: v
            for k, v in phase.items()
            if k
            in (
                "kwin_cpu_s",
                "callback_hz",
                "p95_ms",
                "p99_ms",
                "late_callbacks",
                "work_ms",
            )
        }
    )
    values.update(
        {
            role: value
            for role, value in phase["cpu_s"].items()
            if role.startswith("engine/")
        }
    )
    values["host_read_mib"] = summed(phase["read_bytes"], "host") / 2**20
    return values


def middle(cases, key, digits=2):
    return f"{median(case[key] for case in cases):.{digits}f}"


def paired(left, right, key, digits):
    diffs = [a[key] - b[key] for a, b in zip(left, right, strict=True)]
    return f"{median(diffs):+.{digits}f} ({min(diffs):+.{digits}f} to {max(diffs):+.{digits}f})"


def table(header, rows):
    return "\n".join(
        "| " + " | ".join(map(str, row)) + " |"
        for row in (header, ["---"] * len(header), *rows)
    )


def sensitivity(blocks, page_bytes):
    ordered = sorted(
        (
            (
                max(
                    (r["host_counters"]["pswpin"] + r["host_counters"]["pswpout"])
                    * page_bytes
                    / 2**20
                    for r in block.values()
                ),
                index,
                block,
            )
            for index, block in blocks.items()
        ),
        key=lambda row: (row[0], row[1]),
    )
    rows = []
    for label, subset in (
        ("All accepted blocks", ordered),
        ("Lower-swap half", ordered[: (len(ordered) + 1) // 2]),
        (
            "At most 16 MiB system swap traffic per arm",
            [r for r in ordered if r[0] <= 16],
        ),
    ):
        selected = [block for _, _, block in subset]
        if not selected:
            rows.append([label, 0, "—", "—", "—", "—"])
            continue
        metrics = []
        for phase, other_mode, other_phase, key, digits in (
            ("idle", "zz", "terminal", "pss_total", 1),
            ("scroll", "qt", "scroll", "cpu_host", 2),
            ("scroll", "qt", "scroll", "cpu_engine", 2),
        ):
            left = [measurements(b["zz"]["phases"][phase]) for b in selected]
            right = [
                measurements(b[other_mode]["phases"][other_phase]) for b in selected
            ]
            metrics.append(paired(left, right, key, digits))
        rows.append([label, len(selected), f"{subset[-1][0]:.1f}", *metrics])
    return table(
        [
            "Subset",
            "Blocks",
            "Max system swap MiB in one arm",
            "Opening pane Δ MiB",
            "Scroll host zz − Qt CPU s",
            "Scroll engine zz − Qt CPU s",
        ],
        rows,
    )


def report(data, groups):
    meta = data["environment"]
    resumptions = len(data.get("resumptions", []))
    restarts = "resumption" if resumptions == 1 else "resumptions"
    measured = [row for row in data["blocks"] if row["accepted"]]
    trials = [run for row in measured for run in row["runs"].values()]
    busy = [run["preflight"]["cpu_busy_pct"] for run in trials]
    max_swap = max(row["max_swap_pss_kib"] for row in measured)
    swap_io = sum(
        run["host_counters"]["pswpin"] + run["host_counters"]["pswpout"]
        for run in trials
    )
    sections = [
        "# Browser resource baseline results",
        f"{meta['utc']}, {meta['cpu']}, {meta['logical_cpus']} logical CPUs, {meta['ram_bytes'] / 2**30:.1f} GiB RAM, Linux {meta['kernel']}, {meta['kwin_version']}.",
        f"{meta['blocks']} accepted blocks per size, {sum(b['warmup'] for b in data['blocks'])} warm-up blocks and {sum(not b['warmup'] and not b['accepted'] for b in data['blocks'])} rejected blocks, with {resumptions} campaign {restarts}. Each measured animation completes {meta['frames']} steps. Values are medians across blocks. Paired differences show the median and the full range of individual differences. CPU is user plus system time across all threads. KWin runs separately and is excluded from app totals.",
        f"Preflight system CPU ranged from {min(busy):.1f}% to {max(busy):.1f}%. The highest sampled app-tree SwapPss was {max_swap} KiB. The system swapped {swap_io} pages in or out during measured trials.",
    ]
    for size, blocks in groups.items():
        sections.append(f"## {size[0]} × {size[1]} CSS pixels")
        cases = {
            (mode, phase): [
                measurements(block[mode]["phases"][phase]) for block in blocks.values()
            ]
            for mode in MODES
            for phase in (("terminal",) if mode == "zz" else ())
            + ("idle", "canvas", "scroll", "integer", "layout")
        }
        content_rows = []
        for task in ("integer", "layout"):
            for mode, label in MODES.items():
                trials = cases[mode, task]
                elapsed = [r["work_ms"] for r in trials]
                content_rows.append(
                    [
                        task,
                        label,
                        f"{median(elapsed):.1f}",
                        f"{min(elapsed):.1f}–{max(elapsed):.1f}",
                        middle(trials, "cpu_host"),
                        middle(trials, "cpu_engine"),
                        middle(trials, "engine/WebContent"),
                    ]
                )
        terminal = cases["zz", "terminal"]
        idle_rows = []
        for mode, phase in (
            ("zz", "terminal"),
            ("headless", "idle"),
            ("qt", "idle"),
            ("zz", "idle"),
        ):
            arm = "zz terminal only" if phase == "terminal" else MODES[mode]
            trials = cases[mode, phase]
            idle_rows.append(
                [
                    arm,
                    *[middle(trials, "pss_" + role, 1) for role in GROUPS],
                    middle(trials, "peak_total", 1),
                    middle(trials, "idle_total", 2),
                ]
            )
        sections.extend(
            [
                "### Idle memory and CPU",
                table(
                    [
                        "Arm",
                        "Total MiB",
                        "Host MiB",
                        "Engine MiB",
                        "Daemon + PTY MiB",
                        "Sampled peak MiB",
                        "CPU % of one core",
                    ],
                    idle_rows,
                ),
            ]
        )
        zz_idle = cases["zz", "idle"]
        memory_rows = [
            [label, *[paired(zz_idle, ref, "pss_" + group, 1) for group in GROUPS]]
            for label, ref in (
                ("Open pane in zz", terminal),
                ("zz minus stock Qt", cases["qt", "idle"]),
            )
        ]
        sections.append(
            table(
                [
                    "Within-block difference",
                    "Total MiB",
                    "Host MiB",
                    "Engine MiB",
                    "Daemon + PTY MiB",
                ],
                memory_rows,
            )
        )
        engine_rows = []
        startup_rows = []
        for mode, label in MODES.items():
            trials = [block[mode] for block in blocks.values()]
            opening = [trial["open_to_ready_ms"] for trial in trials]
            page = [trial["ready"]["navigationMs"] for trial in trials]
            startup_rows.append(
                [
                    label,
                    "Pane request" if mode == "zz" else "Profile setup and launch",
                    f"{median(opening):.0f}",
                    f"{min(opening):.0f}–{max(opening):.0f}",
                    f"{median(page):.0f}",
                ]
            )
        sections.extend(
            [
                "### Opening the first page",
                table(
                    [
                        "Arm",
                        "Start of clock",
                        "Median to ready ms",
                        "Range ms",
                        "Navigation to fixture ready ms",
                    ],
                    startup_rows,
                ),
            ]
        )
        work_rows, timing_rows, paired_rows = [], [], []
        for phase in ("canvas", "scroll"):
            for mode, label in MODES.items():
                runs = cases[mode, phase]
                work_rows.append(
                    [
                        phase,
                        label,
                        *[middle(runs, "cpu_" + group) for group in GROUPS],
                        middle(runs, "kwin_cpu_s"),
                        middle(runs, "host_read_mib", 1),
                        middle(runs, "peak_total", 1),
                    ]
                )
                helpers = ["engine/" + h for h in ("WebContent", "Compositor", "Hub")]
                other = [r["cpu_engine"] - sum(r[h] for h in helpers) for r in runs]
                engine_rows.append(
                    [
                        phase,
                        label,
                        *[middle(runs, h) for h in helpers],
                        f"{max(0, median(other)):.2f}",
                    ]
                )
                timing_rows.append(
                    [
                        phase,
                        label,
                        *[
                            middle(runs, k, 1)
                            for k in (
                                "callback_hz",
                                "p95_ms",
                                "p99_ms",
                                "late_callbacks",
                            )
                        ],
                        max(r["late_callbacks"] for r in runs),
                    ]
                )
            zz, qt = cases["zz", phase], cases["qt", phase]
            paired_rows.append(
                [
                    phase,
                    *[
                        paired(zz, qt, "cpu_" + group, 2)
                        for group in ("total", "host", "engine")
                    ],
                ]
            )
        sections.extend(
            [
                "### CPU and reads per completed workload",
                table(
                    [
                        "Work",
                        "Arm",
                        "Total CPU s",
                        "Host CPU s",
                        "Engine CPU s",
                        "Daemon + PTY CPU s",
                        "KWin CPU s",
                        "Host reads MiB",
                        "Sampled peak MiB",
                    ],
                    work_rows,
                ),
                table(
                    ["zz minus stock Qt", "Total CPU s", "Host CPU s", "Engine CPU s"],
                    paired_rows,
                ),
                "### JavaScript and synchronous layout",
                table(
                    [
                        "Work",
                        "Arm",
                        "Page time ms",
                        "Page time range ms",
                        "Host CPU s",
                        "Engine CPU s",
                        "WebContent CPU s",
                    ],
                    content_rows,
                ),
                "### Engine CPU by helper",
                table(
                    [
                        "Work",
                        "Arm",
                        "WebContent CPU s",
                        "Compositor CPU s",
                        "Hub CPU s",
                        "Other helpers CPU s",
                    ],
                    engine_rows,
                ),
                "### Page animation callbacks",
                table(
                    [
                        "Work",
                        "Arm",
                        "Hz",
                        "p95 ms",
                        "p99 ms",
                        "Count >25 ms",
                        "Worst trial count >25 ms",
                    ],
                    timing_rows,
                ),
                "### Sensitivity to background swapping",
                sensitivity(blocks, meta["page_bytes"]),
            ]
        )
    return "\n\n".join(sections) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path, help="Run directory or compressed samples")
    parser.add_argument("--output", type=Path, help="Write the Markdown report")
    parser.add_argument(
        "--archive", type=Path, help="Pack the environment and every trial"
    )
    args = parser.parse_args()
    data = load_sample(args.source)
    summary = report(data, accepted_blocks(data))
    if args.archive:
        encoded = json.dumps(data, separators=(",", ":")).encode()
        args.archive.write_bytes(gzip.compress(encoded, mtime=0))
    if args.output:
        args.output.write_text(summary)
    else:
        print(summary, end="")


if __name__ == "__main__":
    main()
