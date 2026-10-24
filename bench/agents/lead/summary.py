"""Pivot lead runs: summary.py RUNS_DIR... (each holds runs/*/metrics.json). Prints one row per model/arm/task."""

import collections
import json
import pathlib
import statistics
import sys

rows = collections.defaultdict(list)
for root in sys.argv[1:] or ["out"]:
    for f in sorted(pathlib.Path(root).glob("runs/*/metrics.json")):
        m = json.loads(f.read_text())
        if m.get("status") not in ("done", "timeout") or "killed=" in (f.parent / "meta").read_text():
            continue
        rows[(m.get("model", ""), m["arm"], m["task"])].append(m)


def mean(ms, key):
    values = [float(m[key]) for m in ms if str(m.get(key, "")) not in ("", "None")]
    return statistics.mean(values) if values else None


def fmt(value, digits=0):
    return "-" if value is None else f"{value:.{digits}f}"


print("| model | arm | task | runs | pass | wrong done | wall s | lead tools | worker starts | polls | keys to workers | sleep s | peak ctx k | out tok k | brief |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for (model, arm, task), ms in sorted(rows.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])):
    print("| " + " | ".join([
        model, arm, task, str(len(ms)),
        f"{sum(int(m.get('pass', 0)) for m in ms)}/{len(ms)}",
        fmt(mean(ms, "wrong_done"), 1), fmt(mean(ms, "wall")), fmt(mean(ms, "tools")), fmt(mean(ms, "workers"), 1),
        fmt(mean(ms, "polls"), 1), fmt(mean(ms, "keys"), 1), fmt(mean(ms, "sleep_s")),
        fmt((mean(ms, "peak_ctx") or 0) / 1000), fmt((mean(ms, "out_tok") or 0) / 1000, 1), fmt(mean(ms, "brief"), 2),
    ]) + " |")
