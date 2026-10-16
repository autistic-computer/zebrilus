import json
import os
import subprocess
import time

import fixtures
import probe
from groups.chatty import FLIP
from groups.throughput import ceiling_seconds, read_seconds
from ptyclient import PtyClient

MIB = 1 << 20


class Tm:
    def __init__(self, ctx, mux):
        self.ctx = ctx
        self.mux = mux
        self.name = mux.name

    def session(self, name, cols, rows, command=None):
        self.ctx.session(self.mux, name, cols, rows, command)
        return self.mux.out("display-message", "-p", "-t", f"{name}:", "#{pane_id}")

    def window(self, session, command=None, focus=False):
        args = ["new-window", *([] if focus else ["-d"]), "-P", "-F", "#{window_id} #{pane_id}", "-t", f"{session}:"]
        window, pane = self.mux.out(*args, *([command] if command else [])).split()
        return window, pane

    def split(self, pane, command):
        pane = self.mux.out("split-window", "-d", "-P", "-F", "#{pane_id}", "-t", pane, command)
        self.mux.run("select-layout", "-t", pane, "tiled")
        return pane

    def close(self, window):
        self.mux.run("kill-window", "-t", window)

    def attach(self, session, cols, rows):
        return self.ctx.attach(self.mux, session, cols, rows)

    def screen(self, pane):
        return self.mux.run("capture-pane", "-p", "-t", pane).stdout

    def spawn_args(self, session):
        return ["new-window", "-d", "-P", "-F", "#{window_id}", "-t", f"{session}:"]

    def spawned(self, out):
        return out.strip()

    def verbs(self, session, pane):
        return {
            "list_sessions": ["list-sessions"],
            "list_panes": ["list-panes", "-s", "-t", session],
            "send": ["send-keys", "-t", pane, "-l", "x"],
            "capture": ["capture-pane", "-p", "-t", pane],
        }


class Rx:
    def __init__(self, ctx, rex):
        self.ctx = ctx
        self.mux = rex
        self.name = rex.name
        self.size = (180, 50)
        self.gates = 0
        self.held = None

    def _command(self, command):
        if not command:
            return None, []
        self.gates += 1
        gate = self.ctx.env.path(f"rex-go-{self.gates}")
        script = f"while [ ! -e {gate} ]; do sleep 0.01; done; {command}"
        return gate, ["--shell", "none", "--", "/bin/sh", "-c", script]

    def _created(self, out, gate, resize=True, hold=False):
        data = json.loads(out)
        first = (data.get("initial_windows") or [data])[0]
        window = first.get("window_id")
        block = (first.get("block_ids") or [data.get("block_id")])[0]
        if resize:
            cols, rows = self.size
            self.mux.run("block", "call", "-b", block, "resize", json.dumps({"columns": cols, "rows": rows}), check=True)
        if gate and hold:
            self.held = gate
        elif gate:
            open(gate, "w").close()
        return window, block

    def release(self):
        open(self.held, "w").close()
        self.held = None

    def session(self, name, cols, rows, command=None, hold=False):
        self.mux.start()
        self.size = (cols, rows)
        gate, args = self._command(command)
        return self._created(self.mux.out("new", name, "--json", *args), gate, hold=hold)[1]

    def window(self, session, command=None, focus=False):
        gate, args = self._command(command)
        out = self.mux.out("window", "new", "-s", session, f"--focus={'true' if focus else 'false'}", "--json", *args)
        return self._created(out, gate)

    def split(self, block, command):
        gate, args = self._command(command)
        out = self.mux.out("split", "-b", block, "--focus=false", "--json", *args)
        return self._created(out, gate, resize=False)[1]

    def close(self, window):
        self.mux.run("window", "close", window)

    def attach(self, session, cols, rows):
        return PtyClient(self.mux.attach_argv(session, cols, rows), self.mux.env, self.ctx.env, cols=cols, rows=rows)

    def screen(self, block):
        return self.mux.run("capture", "-b", block).stdout

    def spawn_args(self, session):
        return ["window", "new", "-s", session, "--focus=false", "--json"]

    def spawned(self, out):
        return json.loads(out)["window_id"]

    def verbs(self, session, block):
        return {
            "list_sessions": ["ls"],
            "list_panes": ["block", "ls", "-s", session],
            "send": ["send", "-b", block, "x"],
            "capture": ["capture", "-b", block],
        }


def rotate(items, i):
    k = i % len(items)
    return items[k:] + items[:k]


def reset(ctx):
    ctx.reset()
    ctx.rex.kill()


def add(ctx, metric_id, unit, kind, values, notes=None, better=None):
    ctx.add(f"rex.{metric_id}", unit, kind, values.get("zz"), values.get("tmux"), notes, better, rex=values.get("rex"))


def samples(ps):
    return {p.name: p.mux.sample() for p in ps}


def timed(argv, env):
    start = time.perf_counter_ns()
    proc = subprocess.run(argv, env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60)
    return (time.perf_counter_ns() - start) / 1e6, proc


def cli(ctx, ps):
    runs = ctx.pick(40, 15)
    warm = 3
    targets = {p.name: p.session("c", 180, 50) for p in ps}
    time.sleep(1.0)

    def measure(verb, size):
        walls = {p.name: [] for p in ps}
        bad = {p.name: 0 for p in ps}
        argv = {p.name: p.mux.argv(*p.verbs("c", targets[p.name])[verb]) for p in ps}
        before = None
        for i in range(warm + runs):
            if i == warm:
                before = samples(ps)
            for p in rotate(ps, i):
                ms, proc = timed(argv[p.name], p.mux.env)
                if i >= warm:
                    walls[p.name].append(ms)
                    bad[p.name] += proc.returncode != 0
        after = samples(ps)
        notes = [f"{n} exited nonzero {k}/{runs}" for n, k in bad.items() if k]
        add(ctx, f"cli.wall.{verb}.{size}", "ms", "wall", walls, notes)
        add(ctx, f"cli.cpu.{verb}.{size}", "ms", "cpu", {n: (after[n].cpu_ns - before[n].cpu_ns) / runs / 1e6 for n in after})

    for verb in ("list_sessions", "list_panes", "send", "capture"):
        measure(verb, "p1")
    for p in ps:
        for _ in range(19):
            p.window("c")
    time.sleep(1.5)
    for verb in ("list_sessions", "list_panes"):
        measure(verb, "p20")


def record_mem(ctx, phase, values, notes=None):
    add(ctx, f"mem.footprint.{phase}", "MiB", "mem", {n: s.footprint / MIB for n, s in values.items()}, notes)
    add(ctx, f"mem.rss.{phase}", "MiB", "rss", {n: s.rss / MIB for n, s in values.items()})
    add(ctx, f"mem.threads.{phase}", "count", "threads", {n: s.threads for n, s in values.items()})


def mem(ctx, ps):
    for p in ps:
        p.session("m", 180, 50)
    time.sleep(2.0)
    record_mem(ctx, "p1", samples(ps))
    for p in ps:
        for _ in range(19):
            p.window("m")
    time.sleep(3.0)
    record_mem(ctx, "p20", samples(ps))
    if not ctx.quick:
        clients = [p.attach("m", 180, 50) for p in ps]
        ctx.drain(clients, 3.0)
        record_mem(ctx, "tui20", samples(ps))
        for client in clients:
            client.kill()
    reset(ctx)
    last = {}
    for p in ps:
        p.session("m", 180, 50, "exec sleep 1000000")
        for _ in range(20):
            last[p.name] = p.window("m", "seq 1 12000; exec sleep 1000000")[1]
    for p in ps:
        ctx.wait(lambda p=p: "12000" in p.screen(last[p.name]), 60, 0.5)
    time.sleep(5.0)
    kept = {p.name: sum(1 for line in p.screen(last[p.name]).splitlines() if line.strip().isdigit()) for p in ps}
    kept["zz"] = ctx.zz.out("display-message", "-p", "-t", last["zz"], "#{history_size}")
    kept["tmux"] = ctx.tmux.out("display-message", "-p", "-t", last["tmux"], "#{history_size}")
    note = f"history kept per pane after seq 1 12000: zz {kept['zz']}, tmux {kept['tmux']} (history_size), rex {kept['rex']} (lines in rex capture)"
    record_mem(ctx, "scroll180", samples(ps), note)


def idle(ctx, ps):
    seconds = ctx.pick(10.0, 5.0)
    for p in ps:
        p.session("i", 180, 50)
        for _ in range(19):
            p.window("i")
    time.sleep(3.0)
    s0 = samples(ps)
    t0 = time.perf_counter()
    time.sleep(seconds)
    elapsed = time.perf_counter() - t0
    s1 = samples(ps)
    add(ctx, "idle.cpu_pct.p20", "%", "cpu", {n: (s1[n].cpu_ns - s0[n].cpu_ns) / 1e9 / elapsed * 100 for n in s1})
    add(ctx, "idle.wakeups_per_s.p20", "1/s", "count", {n: (s1[n].wakeups - s0[n].wakeups) / elapsed for n in s1})


def spawn(ctx, ps):
    runs = ctx.pick(10, 4)
    for p in ps:
        p.session("s", 180, 50)
    time.sleep(0.5)
    wall = {p.name: [] for p in ps}
    cpu = {p.name: [] for p in ps}
    close = {p.name: [] for p in ps}
    for i in range(runs):
        for p in rotate(ps, i):
            s0 = p.mux.sample()
            ms, proc = timed(p.mux.argv(*p.spawn_args("s")), p.mux.env)
            time.sleep(0.1)
            s1 = p.mux.sample()
            wall[p.name].append(ms)
            cpu[p.name].append((s1.cpu_ns - s0.cpu_ns) / 1e6)
            if proc.returncode == 0:
                window = p.spawned(proc.stdout)
                kms, _ = timed(p.mux.argv(*(["window", "close", window] if p.name == "rex" else ["kill-window", "-t", window])), p.mux.env)
                close[p.name].append(kms)
            time.sleep(0.1)
    add(ctx, "spawn.wall.new_window", "ms", "wall", wall)
    add(ctx, "spawn.cpu.new_window", "ms", "cpu", cpu)
    add(ctx, "spawn.wall.close_window", "ms", "wall", close)


def detached(ctx, ps, kind, path, runs):
    size = os.path.getsize(path)
    rate = {p.name: [] for p in ps}
    ceiling = []
    for p in ps:
        p.session("t", 180, 50)
    time.sleep(0.5)
    for i in range(runs):
        ceiling.append(size / ceiling_seconds(path) / 1e6)
        for p in rotate(ps, i):
            out = ctx.env.path(f"rx-tp-{kind}-{p.name}-{i}.txt")
            window, _ = p.window("t", ctx.pane("timer.py", path, out))
            seconds = read_seconds(out, 300)
            if seconds:
                rate[p.name].append(size / seconds / 1e6)
            p.close(window)
            time.sleep(0.3)
    add(ctx, f"throughput.detached.{kind}", "MB/s", "throughput", rate, f"pty ceiling {sum(ceiling) / len(ceiling):.0f} MB/s", better="higher")
    reset(ctx)


def attached_client(ctx, p, name, path, out):
    if p.name == "rex":
        p.session(name, 180, 51, ctx.pane("timer.py", path, out), hold=True)
        client = p.attach(name, 180, 51)
        client.drain(1.0)
        return client, p.release
    p.session(name, 180, 50)
    time.sleep(0.5)
    client = p.attach(name, 180, 51)
    client.drain(1.0)
    return client, lambda: p.window(name, ctx.pane("timer.py", path, out), focus=True)


def attached(ctx, ps, path, runs):
    ms = {p.name: [] for p in ps}
    tty = {p.name: [] for p in ps}
    client_cpu = {p.name: [] for p in ps}
    for i in range(runs):
        for p in rotate(ps, i):
            reset(ctx)
            out = ctx.env.path(f"rx-tpa-{p.name}-{i}.txt")
            client, start = attached_client(ctx, p, f"ta{i}", path, out)
            before = client.total
            c0 = probe.sample(client.pid).cpu_ns
            start()
            seconds = read_seconds(out, 300, pump=lambda client=client: client.drain(0.02))
            if seconds:
                ms[p.name].append(seconds * 1000)
                client.drain_quiet(0.3, 10.0)
                tty[p.name].append(client.total - before)
                client_cpu[p.name].append((probe.sample(client.pid).cpu_ns - c0) / 1e6)
            client.kill()
    note = "rex attach passes one block through, so Rex runs the flood in a fresh session's first block and attaches before it starts"
    add(ctx, "throughput.attached.ascii_ms", "ms", "throughput", ms, note)
    add(ctx, "throughput.attached.tty_bytes", "B", "bytes_info", tty)
    add(ctx, "throughput.attached.client_cpu_ms", "ms", "cpu", client_cpu)


def chatty(ctx, ps):
    seconds = ctx.pick(10.0, 4.0)
    for p in ps:
        p.session("po", 120, 30)
        for _ in range(10):
            p.window("po", ctx.pane("printer.py", 100))
    time.sleep(1.5)
    s0 = samples(ps)
    t0 = time.perf_counter()
    time.sleep(seconds)
    elapsed = time.perf_counter() - t0
    s1 = samples(ps)
    add(ctx, "chatty.cpu_pct.steady", "%", "cpu", {n: (s1[n].cpu_ns - s0[n].cpu_ns) / 1e9 / elapsed * 100 for n in s1})
    add(ctx, "chatty.rss.steady", "MiB", "rss", {n: s1[n].rss / MIB for n in s1})
    if ctx.quick:
        return
    reset(ctx)
    for p in ps:
        if p.name == "rex":
            continue
        first = p.session("po", 180, 50, FLIP)
        for _ in range(3):
            p.split(first, FLIP)
    time.sleep(1.0)
    ps = [p for p in ps if p.name != "rex"]
    clients = {p.name: p.attach("po", 180, 51) for p in ps}
    ctx.drain(list(clients.values()), 2.0)
    s0 = samples(ps)
    c0 = {n: probe.sample(c.pid).cpu_ns for n, c in clients.items()}
    t0 = time.perf_counter()
    got = ctx.drain(list(clients.values()), seconds)
    elapsed = time.perf_counter() - t0
    s1 = samples(ps)
    add(ctx, "chatty.cpu_pct.visible", "%", "cpu", {n: (s1[n].cpu_ns - s0[n].cpu_ns) / 1e9 / elapsed * 100 for n in s1}, "no rex: rex attach shows one block, not a window of four")
    add(ctx, "chatty.client_cpu_pct.visible", "%", "cpu", {n: (probe.sample(c.pid).cpu_ns - c0[n]) / 1e9 / elapsed * 100 for n, c in clients.items()})
    add(ctx, "chatty.tty_kibps.visible", "KiB/s", "bytes", {n: got[id(c)] / 1024 / elapsed for n, c in clients.items()})
    for client in clients.values():
        client.kill()


def backpressure(ctx, ps, path):
    limit = 90.0
    ms = {}
    peak = {}
    client_peak = {}
    notes = []
    for p in ps:
        reset(ctx)
        out = ctx.env.path(f"rx-bp-{p.name}.txt")
        client, start = attached_client(ctx, p, "bp", path, out)
        start()
        top = 0
        ctop = 0
        begin = time.monotonic()
        while not os.path.exists(out) and time.monotonic() - begin < limit:
            top = max(top, p.mux.sample().rss)
            if probe.alive(client.pid):
                ctop = max(ctop, probe.sample(client.pid).rss)
            time.sleep(0.1)
        seconds = read_seconds(out, 0.5)
        stalled_alive = probe.alive(client.pid)
        recovered = client.drain_quiet(1.0, 20.0) if stalled_alive else False
        peak[p.name] = top / MIB
        client_peak[p.name] = ctop / MIB
        if seconds:
            ms[p.name] = seconds * 1000
        notes.append(f"{p.name}: {'finished' if seconds else f'not finished in {limit:.0f} s'}, client {'alive' if stalled_alive else 'gone'} after the flood{', drained' if recovered else ''}")
        client.kill()
    add(ctx, "backpressure.ascii_ms", "ms", "throughput", ms, notes)
    add(ctx, "backpressure.server_peak_rss", "MiB", "rss", peak)
    add(ctx, "backpressure.client_peak_rss", "MiB", "rss", client_peak)


def run(ctx):
    ps = [Tm(ctx, ctx.zz), Tm(ctx, ctx.tmux), Rx(ctx, ctx.rex)]
    ascii_path = fixtures.fixture("ascii", 150)
    steps = [
        ("cli", lambda: cli(ctx, ps)),
        ("mem", lambda: mem(ctx, ps)),
        ("idle", lambda: idle(ctx, ps)),
        ("spawn", lambda: spawn(ctx, ps)),
        ("detached", lambda: detached(ctx, ps, "ascii", ascii_path, ctx.pick(3, 1))),
        ("chatty", lambda: chatty(ctx, ps)),
    ]
    if not ctx.quick:
        steps += [
            ("unicode", lambda: detached(ctx, ps, "unicode", fixtures.fixture("unicode", 150), 3)),
            ("attached", lambda: attached(ctx, ps, ascii_path, 3)),
            ("backpressure", lambda: backpressure(ctx, ps, ascii_path)),
        ]
    for name, step in steps:
        print(f"[rex] {name}", flush=True)
        reset(ctx)
        step()
    reset(ctx)
