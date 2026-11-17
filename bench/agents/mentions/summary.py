import collections
import re
import statistics
import sys

rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1]) if l.strip()]
cell = collections.defaultdict(list)
for r in rows:
    cell[(r[1], r[0])].append(r)


def med(rs, i):
    return statistics.median(float(r[i]) for r in rs)


print("| scenario | host | pass | token | median calls | median failed | median polling | median panes | median wall s | ends |")
print("|---|---|---|---|---|---|---|---|---|---|")
for k in sorted(cell):
    rs = cell[k]
    ends = collections.Counter(r[4] for r in rs)
    print(
        f"| {k[0]} | {k[1]} | {sum(int(r[3]) for r in rs)}/{len(rs)} | {'-' if k[0] in ('m4b', 'm4c', 'm5') else f'{sum(int(r[12]) for r in rs)}/{len(rs)}'} "
        f"| {med(rs, 6):g} | {med(rs, 7):g} | {med(rs, 8):g} | {med(rs, 9):g} | {med(rs, 5):.0f} "
        f"| {', '.join(f'{e} {n}' for e, n in ends.most_common())} |"
    )
print()
why = collections.Counter()
for r in rows:
    for w in r[14].split("; "):
        if w != "ok":
            why[(re.sub(r"\d+", "N", re.sub(r" \(.*", "", w)), r[0])] += 1
print("| failure | host | runs |")
print("|---|---|---|")
for (w, h), n in why.most_common():
    print(f"| {w} | {h} | {n} |")
print()
tok = [sum(int(r[i]) for r in rows) for i in (15, 16, 17, 18)]
print(f"runs: {len(rows)}; claude tokens in {tok[0]:,} out {tok[1]:,}; codex tokens in {tok[2]:,} out {tok[3]:,}")
