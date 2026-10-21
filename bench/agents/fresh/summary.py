import collections, statistics, sys

rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1]) if l.strip()]
rows = [r for r in rows if r[2] != "n0"]
cell = collections.defaultdict(list)
task = collections.defaultdict(list)
for r in rows:
    cell[(r[0], r[1])].append(r)
    task[(r[2], r[0][:6] + "/" + r[1])].append(int(r[4]))

print("model              arm      pass   med_s  med_calls  mean_fail  blocked  med_ktok  cost")
for k in sorted(cell):
    rs = cell[k]
    p = sum(int(r[4]) for r in rs)
    med = lambda i: statistics.median(float(r[i]) for r in rs)
    fails = sum(float(r[8]) for r in rs) / len(rs)
    blocked = sum(int(r[9]) for r in rs)
    cost = sum(float(r[12]) for r in rs)
    print(f"{k[0]:18s} {k[1]:8s} {p:2d}/{len(rs):<3d} {med(5):5.0f}  {med(7):9.0f}  {fails:9.1f}  {blocked:7d}  {med(10)/1000:8.1f}  ${cost:.2f}")
print()
cols = sorted({c for _, c in task})
print("task  " + "  ".join(f"{c:14s}" for c in cols))
for t in sorted({t for t, _ in task}):
    print(f"{t:5s} " + "  ".join(f"{sum(task[(t, c)])}/{len(task[(t, c)])}".ljust(14) for c in cols))
