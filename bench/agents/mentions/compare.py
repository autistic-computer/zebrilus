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


a, b = load(sys.argv[1]), load(sys.argv[2])


def stats(rs):
    if not rs:
        return "-", "-", "-"
    med = lambda i: f"{statistics.median(float(r[i]) for r in rs):g}"
    return f"{sum(int(r[3]) for r in rs)}/{len(rs)}", med(6), med(7)


print("| scenario | host | pass before | pass after | calls before | calls after | failed before | failed after |")
print("|---|---|---|---|---|---|---|---|")
for k in sorted(set(a) | set(b)):
    pa, ca, fa = stats(a.get(k, []))
    pb, cb, fb = stats(b.get(k, []))
    print(f"| {k[0]} | {k[1]} | {pa} | {pb} | {ca} | {cb} | {fa} | {fb} |")
