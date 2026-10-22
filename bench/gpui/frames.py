#!/usr/bin/env python3
import argparse
from collections import defaultdict
import json
import os
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

STREAM_PL = r"""
use Time::HiRes qw(time sleep);
$| = 1;
my $rate = $ARGV[0] || 240;
my $start = time;
my $n = 0;
my @words = qw(alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau);
while (1) {
    $n++;
    my $w = join(' ', map { $words[($n * 7 + $_ * 3) % @words] } 0..9);
    printf("\e[3%dm%08d\e[0m %s %s\n", $n % 7 + 1, $n, $w, 'x' x ($n % 23));
    my $due = $start + $n / $rate;
    my $now = time;
    sleep($due - $now) if $due > $now;
}
"""

ANIM_HTML = """<!doctype html><html><body style="margin:0;background:#202830">
<canvas id=c style="width:100vw;height:100vh"></canvas><script>
const c=document.getElementById('c'),x=c.getContext('2d');
function f(t){c.width=innerWidth;c.height=innerHeight;x.fillStyle='#e0a040';
for(let i=0;i<40;i++){x.beginPath();x.arc((Math.sin(t/700+i)*0.45+0.5)*c.width,(i+0.5)*c.height/40,12,0,7);x.fill()}
requestAnimationFrame(f)}requestAnimationFrame(f)</script></body></html>"""

AGENT_AWK = r"""
function request_id(line) {
  if (match(line, /"id":"[^"]*"/)) {
    return substr(line, RSTART + 5, RLENGTH - 5)
  }
  if (match(line, /"id":[0-9]+/)) {
    return substr(line, RSTART + 5, RLENGTH - 5)
  }
  return "null"
}
/"method":"initialize"/ {
  printf "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"protocolVersion\":1,\"agentInfo\":{\"name\":\"zz-frames\",\"version\":\"1\"}}}\n", request_id($0)
  fflush()
  next
}
/"method":"session\/new"/ {
  printf "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"sessionId\":\"zz-frames\"}}\n", request_id($0)
  fflush()
  next
}
/"method":"session\/prompt"/ {
  id = request_id($0)
  for (i = 1; i <= items; i++) {
    if (i % 5 == 0) {
      printf "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"zz-frames\",\"update\":{\"sessionUpdate\":\"tool_call\",\"toolCallId\":\"call-%d\",\"title\":\"read crates/zz/src/chunk_%d.rs\",\"kind\":\"read\",\"status\":\"completed\"}}}\n", i, i
    } else {
      printf "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"zz-frames\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"Step %d reads the **stream** output, checks `frames.py` and keeps going while the terminal next to it prints line after line of colored text.\\n\\n\"}}}}\n", i
    }
  }
  printf "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"stopReason\":\"end_turn\"}}\n", id
  fflush()
  next
}
"""

SCENARIOS = ["idle", "stream", "grid", "scroll", "sidebar", "chrome", "agent", "browser"]
DEFAULT_SCENARIOS = ["idle", "stream", "grid", "scroll", "sidebar", "chrome"]
FOREIGN = tuple(p for p in os.environ.get("ZZ_FRAMES_FOREIGN", "").split(":") if p)
QUIET = ("bench/perf/run.py", "bench/perf/gate.py", "compat/run.sh", "bench/run.sh")
HOLD = os.environ.get("ZZ_FRAMES_HOLD")
LINUX = sys.platform.startswith("linux")


class HeldBuilds:
    def __enter__(self):
        if HOLD:
            Path(HOLD).write_text(str(os.getpid()))
            time.sleep(4)
        return self

    def __exit__(self, *exc):
        if HOLD:
            try:
                os.unlink(HOLD)
            except FileNotFoundError:
                pass


def awk_command():
    version = subprocess.run(["awk", "-W", "version"], capture_output=True, text=True, stdin=subprocess.DEVNULL)
    if (version.stdout + version.stderr).startswith("mawk"):
        return "awk -W interactive"
    return "awk"


def noise(own=()):
    out = subprocess.run(["ps", "-Ao", "pid=,stat=,command="], capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        pid, stat, command = (line.split(None, 2) + ["", ""])[:3]
        argv = command.split()
        if not argv or stat.startswith("T"):
            continue
        name = os.path.basename(argv[0])
        if name in ("rustc", "cargo", "clippy-driver", "ld", "ld64", "zig"):
            found.append(name)
        elif name == "gamescope" and int(pid) not in own:
            found.append(name)
        elif any(root in argv[0] for root in FOREIGN):
            found.append(os.path.basename(argv[0]))
        elif len(argv) > 1 and name in ("python3", "Python", "bash", "sh") and any(q in argv[1] for q in QUIET):
            found.append(argv[1])
    return found


def gate_running():
    out = subprocess.run(["ps", "-Ao", "stat=,command="], capture_output=True, text=True).stdout
    for line in out.splitlines():
        stat, _, command = line.strip().partition(" ")
        argv = command.split()
        if not argv or stat.startswith("T"):
            continue
        name = os.path.basename(argv[0])
        if any(root in argv[0] for root in FOREIGN) and "/deps/" in argv[0]:
            return True
        if len(argv) > 1 and name in ("python3", "Python", "bash", "sh") and any(q in argv[1] for q in QUIET):
            return True
    return False


def tree(roots):
    out = subprocess.run(["ps", "-Ao", "pid=,ppid="], capture_output=True, text=True).stdout
    children = {}
    for line in out.splitlines():
        pid, ppid = (int(v) for v in line.split())
        children.setdefault(ppid, []).append(pid)
    found, stack = [], [r for r in roots if r]
    while stack:
        pid = stack.pop()
        found.append(pid)
        stack.extend(children.get(pid, []))
    return found


def wait_quiet(limit, own=()):
    deadline = time.time() + limit
    while time.time() < deadline:
        if not noise(own):
            return True
        time.sleep(5)
    return False


def proc_cpu(path):
    fields = Path(path).read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def thread_times(pid):
    if LINUX:
        threads = len(os.listdir(f"/proc/{pid}/task"))
        return proc_cpu(f"/proc/{pid}/task/{pid}/stat"), proc_cpu(f"/proc/{pid}/stat"), threads
    out = subprocess.run(["ps", "-M", "-p", str(pid)], capture_output=True, text=True).stdout
    rows = out.splitlines()[1:]
    total = 0.0
    main = None
    for i, row in enumerate(rows):
        parts = row.split()
        if i == 0:
            stime, utime = parts[6], parts[7]
        else:
            stime, utime = parts[4], parts[5]
        t = clock(stime) + clock(utime)
        if main is None:
            main = t
    out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout
    total = clock(out.strip()) if out.strip() else 0.0
    return main or 0.0, total, len(rows)


def clock(value):
    seconds = 0.0
    for part in value.split(":"):
        seconds = seconds * 60 + float(part)
    return seconds


RAPL = ("intel-rapl:0", "intel-rapl:0:0", "intel-rapl:0:1")


class Power:
    def __init__(self, duration):
        self.duration = duration
        self.perf = None
        self.energy0 = None

    @staticmethod
    def energy():
        paths = [f"/sys/class/powercap/{d}/energy_uj" for d in RAPL]
        out = subprocess.run(["sudo", "-n", "cat", *paths], capture_output=True, text=True)
        if out.returncode != 0:
            return None
        return [int(v) for v in out.stdout.split()]

    def start(self):
        self.energy0 = self.energy()
        self.perf = subprocess.Popen(
            ["sudo", "-n", "perf", "stat", "-x,", "-a", "-e", "i915/rcs0-busy/", "--", "sleep", str(self.duration)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )

    def stop(self, elapsed):
        out = {}
        energy1 = self.energy()
        if self.energy0 and energy1:
            for name, d, e0, e1 in zip(("package", "core", "uncore"), RAPL, self.energy0, energy1):
                if e1 < e0:
                    e1 += int(Path(f"/sys/class/powercap/{d}/max_energy_range_uj").read_text())
                out[f"{name}_w"] = (e1 - e0) / 1e6 / elapsed
        if self.perf:
            _, err = self.perf.communicate()
            self.perf = None
            for line in err.splitlines():
                fields = line.split(",")
                if len(fields) > 2 and "rcs0-busy" in fields[2] and fields[0].replace(".", "").isdigit():
                    out["gpu_busy_pct"] = 100 * float(fields[0]) / 1e9 / self.duration
        return out

    def discard(self):
        if self.perf is None:
            return
        if self.perf.poll() is None:
            subprocess.run(["sudo", "-n", "kill", "-TERM", str(self.perf.pid)], capture_output=True)
        self.perf.communicate()
        self.perf = None


def thread_switches(pid):
    found = {}
    if not LINUX:
        return found
    try:
        tids = os.listdir(f"/proc/{pid}/task")
    except OSError:
        return found
    for tid in tids:
        try:
            status = Path(f"/proc/{pid}/task/{tid}/status").read_text()
            name = Path(f"/proc/{pid}/task/{tid}/comm").read_text().strip()
        except OSError:
            continue
        fields = dict(line.split(":", 1) for line in status.splitlines() if ":" in line)
        if tid == str(pid):
            name = "main"
        found[tid] = (name.rstrip("0123456789").rstrip("-"), int(fields["voluntary_ctxt_switches"]))
    return found


def wakeups(before, after, elapsed):
    by_name = defaultdict(int)
    for tid, (name, count) in after.items():
        by_name[name] += count - before.get(tid, (name, 0))[1]
    total = sum(by_name.values())
    return {
        "gui_wakeups_per_s": total / elapsed,
        "gui_wakeups_by_thread": {k: round(v / elapsed, 2) for k, v in sorted(by_name.items(), key=lambda kv: -kv[1]) if v},
    }


def gpu_usage(pid):
    clients = {}
    try:
        fds = os.listdir(f"/proc/{pid}/fdinfo")
    except OSError:
        return None
    for fd in fds:
        try:
            text = Path(f"/proc/{pid}/fdinfo/{fd}").read_text()
        except OSError:
            continue
        if "drm-engine-render:" not in text:
            continue
        fields = dict(line.split(":", 1) for line in text.splitlines() if ":" in line)
        client = fields.get("drm-client-id", fd).strip()
        render = int(fields["drm-engine-render"].split()[0])
        resident = fields.get("drm-resident-system0", "0 KiB").split()
        scale = {"KiB": 1024, "MiB": 1024 * 1024, "GiB": 1024**3}.get(resident[1] if len(resident) > 1 else "", 1)
        clients[client] = (render, int(resident[0]) * scale)
    if not clients:
        return None
    return sum(r for r, _ in clients.values()), sum(m for _, m in clients.values())


def footprint(pid):
    if LINUX:
        for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
            if line.startswith("Pss:"):
                return int(line.split()[1]) * 1024
        return None
    out = subprocess.run(
        ["footprint", "-p", str(pid), "--format", "bytes"], capture_output=True, text=True
    ).stdout
    for line in out.splitlines():
        if "phys_footprint:" in line:
            return int(line.split()[-2]) if line.split()[-1] == "B" else int(line.split()[1])
    return None


class Run:
    def __init__(self, app, out_dir, label, warmup, duration, rate, extra_env=None, config=None, display=None, refresh=120, power=False):
        self.refresh = refresh
        self.power = power
        self.extra_env = extra_env or {}
        self.config = config
        self.display = display
        self.app = Path(app)
        self.binary = self.app / ("zz" if LINUX else "Contents/MacOS/zz")
        self.out_dir = Path(out_dir)
        self.label = label
        self.warmup = warmup
        self.duration = duration
        self.rate = rate
        self.quiet_wait = 900
        self.root = Path(tempfile.mkdtemp(prefix="zzf.", dir="/tmp"))
        self.home = self.root / "h"
        self.socket = self.root / "s"
        self.gui = None
        self.gui_pid = None
        self.size = None
        self.daemon_pid = None

    def env(self):
        env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith(("ZZ_", "TMUX", "GPUI_"))
            and k not in ("HOME", "ZDOTDIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME")
        }
        env["HOME"] = str(self.home)
        env["ZZ_LOG_DIR"] = str(self.root / "logs")
        return env

    def zz(self, *args, check=True):
        result = subprocess.run(
            [str(self.binary), "--socket", str(self.socket), *args],
            env=self.env(),
            capture_output=True,
            text=True,
            timeout=30,
        )
        if check and result.returncode != 0:
            raise RuntimeError(f"zz {' '.join(args)}: {result.stderr.strip()}")
        return result.stdout.strip()

    def prepare(self, x, y, width, height):
        data = self.home / (".local/share/zz" if LINUX else "Library/Application Support/zz")
        data.mkdir(parents=True)
        (data / "import-prompted-v2").write_text("")
        self.size = (int(width), int(height))
        (self.root / "logs").mkdir()
        (self.home / ".zshrc").write_text("PROMPT='%~ %# '\n")
        (self.home / ".bashrc").write_text("PS1='\\w $ '\n")
        (data / "window-state.json").write_text(
            json.dumps(
                {
                    "version": 1,
                    "display_uuid": self.display,
                    "mode": "windowed",
                    "bounds": {"x": x, "y": y, "width": width, "height": height},
                }
            )
        )
        (self.root / "stream.pl").write_text(STREAM_PL)
        if self.config:
            (self.home / ".config/zz").mkdir(parents=True)
            shutil.copy(self.config, self.home / ".config/zz/config")

    def launch(self, frames_path):
        env = self.env()
        env.update(self.extra_env)
        if frames_path:
            env["GPUI_FRAME_STATS"] = str(frames_path)
        command = [str(self.binary), "--socket", str(self.socket), "app"]
        if LINUX:
            w, h = (str(v) for v in self.size)
            command = ["gamescope", "--backend", "headless", "-W", w, "-H", h, "-w", w, "-h", h, "-r", str(self.refresh), "--expose-wayland", "--", *command]
        self.gui = subprocess.Popen(
            command,
            env=env,
            stdout=open(self.root / "app.stdout", "w"),
            stderr=open(self.root / "app.stderr", "w"),
        )
        identity = Path(str(self.socket) + ".identity")
        deadline = time.time() + 20
        while time.time() < deadline:
            if self.gui.poll() is not None:
                raise RuntimeError("GUI exited during startup")
            if identity.exists() and identity.stat().st_size > 0:
                for line in identity.read_text().splitlines():
                    if line.startswith("pid="):
                        self.daemon_pid = int(line[4:])
                if self.daemon_pid:
                    break
            time.sleep(0.05)
        else:
            raise RuntimeError("daemon not ready")
        self.gui_pid = self.gui.pid
        if LINUX:
            binary = self.binary.resolve()
            for pid in tree([self.gui.pid]):
                try:
                    argv = Path(f"/proc/{pid}/cmdline").read_bytes().decode().split("\0")
                    exe = Path(f"/proc/{pid}/exe").resolve()
                except OSError:
                    continue
                if exe == binary and "app" in argv and not any(a.startswith("--type=") for a in argv):
                    self.gui_pid = pid
                    break
            else:
                raise RuntimeError("GUI process not found under gamescope")
        deadline = time.time() + 20
        while time.time() < deadline:
            if self.zz("list-panes", "-a", "-F", "#{pane_id}", check=False):
                break
            time.sleep(0.1)
        time.sleep(3)
        line = self.zz("list-clients", "-F", "#{client_name}\t#{client_session}").splitlines()[0]
        self.client, self.session = line.split("\t")

    def stream_cmd(self):
        return f"perl {self.root / 'stream.pl'} {self.rate}"

    def current_window(self):
        return self.zz("display-message", "-t", f"{self.session}:", "-p", "#{window_id}")

    def setup(self, scenario):
        previous = self.current_window()
        own = self.zz("display-message", "-t", f"{self.session}:", "-p", "#{session_id}")
        for session in self.zz("list-sessions", "-F", "#{session_id}").splitlines():
            if session != own:
                self.zz("kill-session", "-t", session)
        if scenario == "idle":
            return None
        if scenario == "stream":
            self.zz("new-window", "-t", f"{self.session}:", "-n", "stream", self.stream_cmd())
        elif scenario == "grid":
            self.zz("new-window", "-t", f"{self.session}:", "-n", "grid")
            for _ in range(8):
                self.zz("split-window", "-t", f"{self.session}:grid")
                self.zz("select-layout", "-t", f"{self.session}:grid", "tiled")
            self.zz("select-layout", "-t", f"{self.session}:grid", "tiled")
            first = self.zz("list-panes", "-t", f"{self.session}:grid", "-F", "#{pane_id}").splitlines()[0]
            self.zz("respawn-pane", "-k", "-t", first, self.stream_cmd())
            self.zz("select-pane", "-t", first)
        elif scenario == "scroll":
            self.zz("new-window", "-t", f"{self.session}:", "-n", "scroll", "seq -f 'scrollback line %06g' 1 20000; exec cat")
            time.sleep(1.5)
            self.zz("copy-mode", "-t", f"{self.session}:scroll")
        elif scenario == "browser":
            page = self.root / "anim.html"
            page.write_text(ANIM_HTML)
            self.zz("new-window", "-t", f"{self.session}:", "--kind", "browser", "-n", "browser", f"file://{page}")
            time.sleep(8)
        elif scenario == "sidebar":
            for i in range(24):
                self.zz("new-session", "-d", "-s", f"s{i:02d}-a-session-name-long-enough-to-fade", "-n", f"window-{i:02d}-with-a-long-descriptive-name")
            self.zz("new-window", "-t", f"{self.session}:", "-n", "streaming-window-with-a-long-name-that-overflows", self.stream_cmd())
        elif scenario == "chrome":
            for i in range(24):
                self.zz("new-session", "-d", "-s", f"s{i:02d}")
            self.zz("new-window", "-t", f"{self.session}:", "-n", "chrome", self.stream_cmd())
        elif scenario == "agent":
            adapter = self.root / "agent.awk"
            adapter.write_text(AGENT_AWK)
            command = f"{awk_command()} -v items=60 -f {adapter}"
            self.zz("set-option", "-g", "experimental-agent-pane", "on")
            self.zz("set-option", "-g", "--", "agent-command", command)
            self.zz("set-option", "-g", "--", "agent-claude-code-command", command)
            self.zz("new-window", "-t", f"{self.session}:", "-n", "agent", self.stream_cmd())
            stream = self.zz("display-message", "-t", f"{self.session}:agent", "-p", "#{pane_id}")
            before = set(self.zz("list-panes", "-t", f"{self.session}:agent", "-F", "#{pane_id}").splitlines())
            self.zz("split-window", "-h", "--kind", "picker", "-t", stream)
            after = set(self.zz("list-panes", "-t", f"{self.session}:agent", "-F", "#{pane_id}").splitlines())
            agent = (after - before).pop()
            self.zz("select-pane-kind", "-t", agent, "agent")
            reply = self.zz("agent-send", "--wait", "--timeout", "25", "-t", agent, "walk through the stream")
            if "Step 59" not in reply:
                raise RuntimeError(f"the agent turn did not finish: {reply[-200:]!r}")
            self.zz("select-pane", "-t", stream)
        self.zz("kill-window", "-t", previous)
        if scenario == "chrome":
            self.zz("command-prompt", "-b", "-C", "-t", self.client)
        return None

    def drive(self, scenario, until):
        if scenario != "scroll":
            time.sleep(max(0.0, until - time.time()))
            return 0
        sent = 0
        direction = "scroll-up"
        next_at = time.time()
        while next_at < until:
            time.sleep(max(0.0, next_at - time.time()))
            self.zz("send-keys", "-t", f"{self.session}:scroll", "-X", "-N", "3", direction, check=False)
            sent += 1
            if sent % 200 == 0:
                direction = "scroll-down" if direction == "scroll-up" else "scroll-up"
            next_at += 0.05
        return sent

    def measure(self, scenario):
        with HeldBuilds():
            return self.measure_held(scenario)

    def freeze_for_gate(self):
        if not gate_running():
            return False
        pids = tree([self.gui.pid, self.daemon_pid])
        for pid in pids:
            try:
                os.kill(pid, signal.SIGSTOP)
            except ProcessLookupError:
                pass
        print("  frozen for a foreign gate", flush=True)
        while gate_running():
            time.sleep(5)
        for pid in reversed(pids):
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        time.sleep(2)
        return True

    def measure_held(self, scenario):
        self.freeze_for_gate()
        quiet_before = wait_quiet(self.quiet_wait, (self.gui.pid,))
        self.setup(scenario)
        noisy_attempts = 0
        for attempt in range(200):
            result = self.window(scenario)
            if result is None:
                continue
            if result["noise"] and noisy_attempts < 3:
                noisy_attempts += 1
                print(f"  noisy window ({','.join(result['noise'])}), retrying", flush=True)
                quiet_before = wait_quiet(self.quiet_wait, (self.gui.pid,))
                continue
            result["quiet_before"] = quiet_before
            result["attempts"] = attempt + 1
            return result
        raise RuntimeError("could not finish a window without a foreign gate")

    def window(self, scenario):
        warm_until = time.time() + self.warmup
        commands = 0
        self.drive(scenario, warm_until)
        main0, total0, threads0 = thread_times(self.gui_pid)
        gpu0 = gpu_usage(self.gui_pid) if LINUX else None
        switches0 = thread_switches(self.gui_pid)
        daemon0 = thread_times(self.daemon_pid)[1]
        power = Power(self.duration) if self.power else None
        t0 = time.time_ns()
        if power:
            power.start()
        noisy = []
        end = time.time() + self.duration
        finished = False
        try:
            while time.time() < end:
                commands += self.drive(scenario, min(end, time.time() + 2))
                noisy += noise((self.gui.pid,))
                if self.freeze_for_gate():
                    return None
            finished = True
        finally:
            if power and not finished:
                power.discard()
        t1 = time.time_ns()
        main1, total1, threads1 = thread_times(self.gui_pid)
        gpu1 = gpu_usage(self.gui_pid) if LINUX else None
        switches1 = thread_switches(self.gui_pid)
        daemon1 = thread_times(self.daemon_pid)[1]
        elapsed = (t1 - t0) / 1e9
        power = power.stop(elapsed) if power else {}
        if gpu0 and gpu1:
            power["gpu_ms_per_s"] = (gpu1[0] - gpu0[0]) / 1e6 / elapsed
            power["gpu_resident"] = gpu1[1]
        return {
            **power,
            **(wakeups(switches0, switches1, elapsed) if switches1 else {}),
            "scenario": scenario,
            "t0_ns": t0,
            "t1_ns": t1,
            "elapsed_s": elapsed,
            "gui_main_cpu_pct": 100 * (main1 - main0) / elapsed,
            "gui_total_cpu_pct": 100 * (total1 - total0) / elapsed,
            "daemon_cpu_pct": 100 * (daemon1 - daemon0) / elapsed,
            "gui_threads": threads1,
            "gui_footprint": footprint(self.gui_pid),
            "scroll_commands": commands,
            "noise": sorted(set(noisy)),
            "load1": os.getloadavg()[0],
        }

    def stop(self):
        if self.gui and self.gui.poll() is None:
            self.gui.send_signal(signal.SIGTERM)
            try:
                self.gui.wait(10)
            except subprocess.TimeoutExpired:
                self.gui.kill()
        if LINUX and self.gui_pid:
            try:
                os.kill(self.gui_pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        try:
            self.zz("kill-server", check=False)
        except Exception:
            pass
        if self.daemon_pid:
            for _ in range(60):
                try:
                    os.kill(self.daemon_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.05)
            else:
                os.kill(self.daemon_pid, signal.SIGTERM)
        shutil.rmtree(self.root, ignore_errors=True)


def summarize_frames(frames_path, window):
    rows = []
    if frames_path and frames_path.exists():
        with open(frames_path) as handle:
            for line in handle:
                try:
                    frame = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if window["t0_ns"] <= frame["timestamp_unix_ns"] < window["t1_ns"]:
                    rows.append(frame)
    out = {"frames": len(rows), "fps": len(rows) / window["elapsed_s"]}
    if not rows:
        return out

    def cpu(timing):
        value = timing.get("thread_cpu_ns")
        return value if value is not None else timing["wall_ns"]

    def med(values):
        return statistics.median(values) / 1000 if values else None

    out["draw_cpu_us_med"] = med([cpu(f["draw"]) for f in rows])
    out["draw_cpu_ms_per_s"] = sum(cpu(f["draw"]) for f in rows) / 1e6 / window["elapsed_s"]
    for phase in ("request_layout", "prepaint", "paint", "scene_finish"):
        out[f"{phase}_us_med"] = med([cpu(f["phases"][phase]) for f in rows])
    submits = [cpu(f["phases"]["present_submit"]) for f in rows if f["phases"].get("present_submit")]
    out["present_submit_us_med"] = med(submits)
    flags = [f["accessibility_active"] for f in rows if "accessibility_active" in f]
    out["accessibility_active_share"] = sum(flags) / len(flags) if flags else None
    for key in ("views_rendered", "layout_nodes", "sprites", "quads", "glyphs_shaped", "shape_calls"):
        out[f"{key}_med"] = statistics.median([f["counts"][key] for f in rows])
    return out


METRICS = [
    ("gui_main_cpu_pct", "main%"),
    ("gui_total_cpu_pct", "gui%"),
    ("gui_wakeups_per_s", "wakeups/s"),
    ("frames", "frames"),
    ("draw_cpu_us_med", "draw_us"),
    ("views_rendered_med", "views"),
    ("layout_nodes_med", "layout_nodes"),
    ("request_layout_us_med", "layout_us"),
    ("prepaint_us_med", "prepaint_us"),
    ("paint_us_med", "paint_us"),
    ("scene_finish_us_med", "finish_us"),
    ("present_submit_us_med", "submit_us"),
    ("gpu_ms_per_s", "gpu_ms/s"),
    ("package_w", "package_W"),
    ("uncore_w", "uncore_W"),
    ("gpu_busy_pct", "gpu_busy%"),
]


def summarize(argv):
    path = argv[0]
    base = argv[1] if len(argv) > 1 else None
    rows = [json.loads(line) for line in open(path)]
    groups = defaultdict(list)
    labels = []
    for row in rows:
        groups[(row["label"], row["scenario"])].append(row)
        if row["label"] not in labels:
            labels.append(row["label"])
    base = base or labels[0]
    noisy = [r for r in rows if r.get("noise")]
    print(f"windows {len(rows)}, with noise {len(noisy)}")
    a11y = {r.get("accessibility_active_share") for r in rows}
    print(f"accessibility_active_share values: {sorted(v for v in a11y if v is not None)}")
    for key, name in METRICS:
        print(f"\n{name}")
        print("scenario  " + "  ".join(f"{label:>32}" for label in labels))
        for scenario in SCENARIOS:
            cells = []
            base_med = None
            for label in labels:
                values = [r.get(key) for r in groups[(label, scenario)] if r.get(key) is not None]
                if not values:
                    cells.append(f"{'-':>32}")
                    continue
                med = statistics.median(values)
                if label == base:
                    base_med = med
                delta = ""
                if label != base and base_med:
                    delta = f"{100 * (med - base_med) / base_med:+.1f}%"
                cells.append(f"{med:.2f} {delta} [{min(values):.2f}-{max(values):.2f}]".rjust(32))
            print(f"{scenario:<9} " + "  ".join(cells))
    print("\nwakeups/s by thread (median)")
    for scenario in SCENARIOS:
        for label in labels:
            runs = [r["gui_wakeups_by_thread"] for r in groups[(label, scenario)] if "gui_wakeups_by_thread" in r]
            if not runs:
                continue
            names = sorted({n for run in runs for n in run})
            meds = {n: statistics.median([run.get(n, 0) for run in runs]) for n in names}
            top = sorted(meds.items(), key=lambda kv: -kv[1])[:8]
            print(f"{scenario:<9} {label:>12}  " + "  ".join(f"{n}={v:.1f}" for n, v in top))



def main():
    if sys.argv[1:2] == ["summarize"]:
        return summarize(sys.argv[2:])
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", action="append", required=True, help="label=path/to/zz.app")
    parser.add_argument("--out", required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=5)
    parser.add_argument("--duration", type=float, default=20)
    parser.add_argument("--rate", type=int, default=240)
    parser.add_argument("--no-recorder", action="store_true")
    parser.add_argument("--scenarios", default=",".join(DEFAULT_SCENARIOS))
    parser.add_argument("--bounds", default="80,80,1280,900")
    parser.add_argument("--quiet-wait", type=float, default=900)
    parser.add_argument("--repeat-offset", type=int, default=0)
    parser.add_argument("--config", help="zz config file every run starts with")
    parser.add_argument("--display-uuid", help="display the window opens on (lowercase CGDisplay UUID)")
    parser.add_argument("--refresh", type=int, default=120, help="Linux gamescope refresh rate")
    parser.add_argument("--power", action="store_true", help="Linux: RAPL package watts and i915 render busy, needs passwordless sudo")
    args = parser.parse_args()
    apps = []
    envs = {}
    for entry in args.app:
        label, rest = entry.split("=", 1)
        path, *pairs = rest.split(",")
        apps.append((label, path))
        envs[label] = dict(pair.split("=", 1) for pair in pairs)
    scenarios = args.scenarios.split(",")
    x, y, width, height = (float(v) for v in args.bounds.split(","))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    results = out / "results.jsonl"
    for repeat in range(args.repeat_offset, args.repeat_offset + args.repeats):
        order = apps if repeat % 2 == 0 else list(reversed(apps))
        for label, app in order:
            run = Run(
                app, out, label, args.warmup, args.duration, args.rate, envs[label], args.config, args.display_uuid, args.refresh, args.power
            )
            run.quiet_wait = args.quiet_wait
            frames = None if args.no_recorder else out / f"frames-{label}-{repeat}.jsonl"
            try:
                run.prepare(x, y, width, height)
                run.launch(frames)
                for scenario in scenarios:
                    window = run.measure(scenario)
                    record = {"label": label, "repeat": repeat, **window, **summarize_frames(frames, window)}
                    with open(results, "a") as handle:
                        handle.write(json.dumps(record) + "\n")
                    print(
                        f"{label} r{repeat} {scenario}: main {record['gui_main_cpu_pct']:.2f}% "
                        f"total {record['gui_total_cpu_pct']:.2f}% frames {record['frames']}",
                        flush=True,
                    )
            finally:
                run.stop()
            time.sleep(3)


if __name__ == "__main__":
    sys.exit(main())
