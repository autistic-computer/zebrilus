import collections
import statistics
import sys


def load(path):
    cell = collections.defaultdict(list)
    for l in open(path):
        if l.strip():
            r = l.rstrip("\n").split("\t")
            cell[(r[1], r[0])].append(r)
    return cell


batches = [load(p) for p in sys.argv[1:]]
keys = sorted(batches[-1]) if len(batches) > 2 else sorted(set().union(*batches))


def stats(rs):
    if not rs:
        return "-", "-", "-"
    med = lambda i: f"{statistics.median(float(r[i]) for r in rs):g}"
    return f"{sum(int(r[3]) for r in rs)}/{len(rs)}", med(6), med(7)


n = len(batches)
print("| scenario | host | pass | median calls | median failed |")
print("|---|---|---|---|---|")
for k in keys:
    cols = [stats(b.get(k, [])) for b in batches]
    print(f"| {k[0]} | {k[1]} | " + " | ".join(" → ".join(c[i] for c in cols) for i in range(3)) + " |")
