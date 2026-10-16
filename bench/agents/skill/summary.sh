#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
python3 - "${1:-$S/out/results.tsv}" <<'EOF'
import collections, statistics, sys

rows = [line.rstrip("\n").split("\t") for line in open(sys.argv[1]) if line.strip()]
cells = collections.defaultdict(list)
tasks = collections.defaultdict(list)
for r in rows:
    cells[(r[0], r[1], r[2])].append(r)
    tasks[(r[3], f"{r[0]}/{r[2]}")].append(int(r[4]))

def med(rs, i):
    return statistics.median(float(r[i]) for r in rs)

print("## backend x model x arm: pass/n, median wall s, median CLI calls, mean failed calls, median CLI stdout KiB, median input ktok, total cost")
for key in sorted(cells):
    rs = cells[key]
    passed = sum(int(r[4]) for r in rs)
    fails = sum(float(r[8]) for r in rs) / len(rs)
    cost = sum(float(r[12]) for r in rs)
    print(f"{key[0]:4s} {key[1]:22s} {key[2]:9s} {passed:3d}/{len(rs):<3d} {med(rs, 5):6.0f}s  calls={med(rs, 7):4.0f}  fail={fails:4.1f}  out={med(rs, 9) / 1024:7.1f}K  in={med(rs, 10) / 1000:6.1f}k  ${cost:.3f}")

print()
print("## per task: passes by backend/arm")
for task in sorted({t for t, _ in tasks}, key=lambda t: int(t[1:])):
    cols = sorted(arm for t, arm in tasks if t == task)
    print(f"{task:4s}" + "".join(f"  {arm:12s} {sum(tasks[(task, arm)])}/{len(tasks[(task, arm)])}" for arm in cols))
EOF
