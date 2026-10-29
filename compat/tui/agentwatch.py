#!/usr/bin/env python3

import json, os, glob, sys, time, re, subprocess, hashlib, collections

ROOT = os.environ.get('ZZ_WATCH_ROOT') or ''
STATE = os.environ.get('ZZ_WATCH_STATE') or '/tmp/zz-agentwatch-state.json'
if not ROOT:
    cands = sorted(glob.glob(os.path.expanduser('~/.claude/projects/*/*/subagents/workflows')),
                   key=os.path.getmtime)
    if not cands:
        sys.exit(0)
    ROOT = cands[-1]
DEV = os.environ.get('ZZ_WATCH_DEV') or os.path.expanduser('~/dev')
NOW = time.time()
TAIL = 60
REPEAT = 4
FAILRUN = 4

def jload(p, d=None):
    try: return json.load(open(p))
    except Exception: return d if d is not None else {}

allruns = sorted(glob.glob(ROOT + '/wf_*'), key=os.path.getmtime)
if not allruns: sys.exit(0)
runs = [r for r in allruns if NOW - os.path.getmtime(r) < 6 * 3600] or allruns[-1:]

st = jload(STATE)
flags, pulse = [], []
for run in runs:
    finished = set()
    shortfall = []
    try:
        for line in open(run + '/journal.jsonl'):
            e = json.loads(line)
            if e.get('type') != 'result': continue
            finished.add(e.get('agentId'))
            r = e.get('result')
            if not isinstance(r, dict): continue
            for ob in r.get('obligations') or []:
                if ob.get('status_after') in ('active', 'different', 'blocked'):
                    shortfall.append((e.get('agentId'), ob.get('id'), ob.get('status_after'),
                                      len(ob.get('clauses_open') or [])))
    except Exception: pass

    meta = {}
    for m in glob.glob(run + '/*.meta.json'):
        i = os.path.basename(m).replace('agent-', '').replace('.meta.json', '')
        meta[i] = jload(m, {}).get('description', i)

    FAIL = re.compile(r'exit code[: ]*(1[0-9]{2}|[1-9][0-9]?)\b|exit status (1[0-9]{2}|[1-9][0-9]?)\b'
                      r'|error\[E\d+\]|panicked at|Killed|Out of memory|No such file|command not found'
                      r'|timed out after|assertion .*failed|FAILED|Traceback', re.I)
    OK = re.compile(r'\bexit(?: code)?[: =]*0\b|PASS\b|all \d+ .*identical|test result: ok', re.I)

    def scan(path):
        calls, results = [], []
        try: lines = open(path, errors='replace').read().splitlines()
        except Exception: return calls, results, 0
        for line in lines:
            try: e = json.loads(line)
            except Exception: continue
            msg = e.get('message', e)
            content = msg.get('content') if isinstance(msg, dict) else None
            if not isinstance(content, list): continue
            for p in content:
                if not isinstance(p, dict): continue
                if p.get('type') == 'tool_use':
                    inp = p.get('input') or {}
                    cmd = inp.get('command') or inp.get('file_path') or json.dumps(inp, sort_keys=True)
                    calls.append(str(cmd))
                elif p.get('type') == 'tool_result':
                    c = p.get('content')
                    if isinstance(c, list):
                        c = ' '.join(x.get('text', '') for x in c if isinstance(x, dict))
                    results.append(str(c)[:4000])
        return calls, results, len(lines)

    prev = st.get(run, {})
    cur = {}

    done_labels = {label for aid, label in meta.items() if aid in finished}

    live, superseded = {}, []
    for aid, label in meta.items():
        if aid in finished or label in done_labels: continue
        path = f'{run}/agent-{aid}.jsonl'
        if not os.path.exists(path): continue
        prior = live.get(label)
        if prior is None or os.path.getmtime(path) > os.path.getmtime(f'{run}/agent-{prior}.jsonl'):
            if prior is not None: superseded.append((label, prior))
            live[label] = aid
        else:
            superseded.append((label, aid))
    for label, aid in superseded:
        if not prev.get('seen_superseded', {}).get(aid):
            flags.append(f'{label}: an earlier attempt ({aid[:8]}) died and the harness restarted it; '
                         f'watching the live one only')
    cur['seen_superseded'] = {aid: True for _, aid in superseded}

    seen_short = dict(prev.get('seen_short', {}))
    for aid, oid, status, nopen in shortfall:
        k = f'{aid}:{oid}'
        if seen_short.get(k): continue
        seen_short[k] = True
        lane = meta.get(aid, aid[:8])
        flags.append(f'SECOND HALF: {lane} finished with {oid} at {status}, {nopen} clause(s) open. '
                     f'{len(live)} agent(s) still live. Read its report: if the remaining items name '
                     f'their files, launch a second-half lane into the idle gate slots instead of '
                     f'carrying {oid} to the next cycle.')
    cur['seen_short'] = seen_short
    st[run] = cur

    for label, aid in sorted((l, a) for l, a in live.items()):
        path = f'{run}/agent-{aid}.jsonl'
        calls, results, nrec = scan(path)
        idle = (NOW - os.path.getmtime(path)) / 60.0
        d = nrec - prev.get(aid, {}).get('rec', nrec)
        cur[aid] = {'rec': nrec}

        if idle > 20:
            flags.append(f'{label}: SILENT {idle:.0f}m, no transcript write ({nrec} records)')

        win = calls[-TAIL:]
        h = collections.Counter(hashlib.sha1(re.sub(r'\s+', ' ', c).strip().encode()).hexdigest() for c in win)
        if h:
            top, k = h.most_common(1)[0]
            if k >= REPEAT:
                sample = next(c for c in reversed(win)
                              if hashlib.sha1(re.sub(r'\s+', ' ', c).strip().encode()).hexdigest() == top)
                flags.append(f'{label}: SPINNING, ran an identical command {k}x in its last {len(win)} calls -> '
                             + re.sub(r'\s+', ' ', sample)[:180])

        rw = results[-TAIL:]
        sigs = []
        for r in rw:
            m = FAIL.search(r)
            if not m or OK.search(r[:400]): continue
            start = max(0, m.start() - 40)
            sigs.append(re.sub(r'\s+', ' ', r[start:m.end() + 90]).strip())
        if sigs:
            sc = collections.Counter(sigs)
            sig, k = sc.most_common(1)[0]
            if k >= FAILRUN:
                flags.append(f'{label}: STUCK, the identical failure {k}x in its last {len(rw)} calls -> {sig[:170]}')
        kills = sum(1 for r in rw if re.search(r'exit code[: ]*137|Killed|Out of memory', r, re.I))
        touts = sum(1 for r in rw if re.search(r'exit code[: ]*124|timed out after', r, re.I))
        if kills >= 2: flags.append(f'{label}: {kills} memory kills in recent results (cap too low or --jobs too high)')
        if touts >= 4: flags.append(f'{label}: {touts} command timeouts in recent results (work does not fit the 600s cap)')

        key = label.split(':')[-1]
        cands = [c for c in glob.glob(DEV + '/zz-tui-*') + glob.glob(DEV + '/zz-gate-*')
                 if not os.path.basename(c).endswith('-review')
                 and re.fullmatch(r'zz-(?:tui|gate)-' + re.escape(key) + r'(?:-\d+)?', os.path.basename(c))]
        cands.sort(key=lambda c: (len(os.path.basename(c)), os.path.getmtime(c)), reverse=True)
        wd = cands[0] if cands else None
        age = None
        if wd:
            try:
                ts = subprocess.run(['git', '-C', wd, 'log', '-1', '--format=%ct'],
                                    capture_output=True, text=True, timeout=20).stdout.strip()
                if ts: age = (NOW - int(ts)) / 60.0
            except Exception: pass
        if age is not None and age > 180 and d > 40 and label.startswith(('worker', 'fix')):
            flags.append(f'{label}: {d} new calls since last check, no commit in {age:.0f}m ({os.path.basename(wd)})')

        pulse.append(f'{label} {nrec}rec(+{d}) idle{idle:.0f}m' + (f' commit{age:.0f}m' if age is not None else ''))

st['ticks'] = st.get('ticks', 0) + 1
try: json.dump(st, open(STATE, 'w'))
except Exception: pass

for f in flags: print('AGENT WATCH: ' + f)
if st['ticks'] % 4 == 0 and pulse: print('agent pulse: ' + ' | '.join(pulse))
