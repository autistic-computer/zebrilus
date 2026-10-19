#!/usr/bin/env python3
import argparse
import json
import os
import signal
import statistics
import subprocess
import sys
import time
from pathlib import Path

ANIM_HTML = """<!doctype html><html><body style="margin:0;background:#202830">
<canvas id=c style="width:100vw;height:100vh"></canvas><script>
const c=document.getElementById('c'),x=c.getContext('2d');
function f(t){c.width=innerWidth;c.height=innerHeight;x.fillStyle='#e0a040';
for(let i=0;i<40;i++){x.beginPath();x.arc((Math.sin(t/700+i)*0.45+0.5)*c.width,(i+0.5)*c.height/40,12,0,7);x.fill()}
requestAnimationFrame(f)}requestAnimationFrame(f)</script></body></html>"""

HERE = Path(__file__).resolve().parent
CDP_PORT = 9341


def rustc_busy():
    return subprocess.run(["pgrep", "-x", "rustc"], capture_output=True).returncode == 0


def wait_quiet(limit):
    deadline = time.time() + limit
    while time.time() < deadline:
        if not rustc_busy():
            return True
        time.sleep(5)
    return False


def proc_cpu(path):
    fields = Path(path).read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def children(pid):
    out = subprocess.run(["ps", "-Ao", "pid=,ppid="], capture_output=True, text=True).stdout
    kids = {}
    for line in out.splitlines():
        p, pp = (int(v) for v in line.split())
        kids.setdefault(pp, []).append(p)
    found, stack = [], [pid]
    while stack:
        p = stack.pop()
        found.append(p)
        stack.extend(kids.get(p, []))
    return found


def cdp_eval(expr):
    out = subprocess.run(
        ["node", str(HERE / "cdp-eval.mjs"), str(CDP_PORT), expr],
        capture_output=True, text=True, timeout=20,
    )
    try:
        return json.loads(out.stdout.strip())
    except json.JSONDecodeError:
        return None


class Run:
    def __init__(self, app, root, shape, app_env=None):
        self.app_env = app_env or {}
        self.binary = Path(app) / "zz"
        self.root = root
        self.shape = shape
        self.socket = f"/tmp/zzm{os.getpid() % 1000}.sock"
        self.gui = None
        self.gamescope = None
        self.dbus = None

    def env(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith(("ZZ_", "TMUX", "GPUI_"))}
        for name in ("config", "data", "state", "cache"):
            env[f"XDG_{name.upper()}_HOME"] = str(self.root / name)
        env["ZZ_SOCKET"] = self.socket
        env["ZZ_LOG_DIR"] = str(self.root / "logs")
        return env

    def zz(self, *args, check=True):
        result = subprocess.run([str(self.binary), *args], env=self.env(), capture_output=True, text=True, timeout=30)
        if check and result.returncode != 0:
            raise RuntimeError(f"zz {' '.join(args)}: {result.stderr.strip()}")
        return result.stdout.strip()

    def launch(self, frames):
        for name in ("config/zz", "data/zz", "state", "cache", "logs"):
            (self.root / name).mkdir(parents=True, exist_ok=True)
        (self.root / "data/zz/import-prompted-v2").write_text("")
        config = "check-for-updates = false\n"
        if self.shape == "glass":
            config += "interface-theme = liquid-glass\n"
        (self.root / "config/zz/config").write_text(config)
        (self.root / "anim.html").write_text(ANIM_HTML)
        env = self.env()
        env["GPUI_FRAME_STATS"] = str(frames)
        env["ZZ_BROWSER_REMOTE_DEBUGGING_PORT"] = str(CDP_PORT)
        env.update(EXTRA_ENV)
        env.update(self.app_env)
        if COMPOSITOR == "kwin":
            out = subprocess.run(["dbus-daemon", "--session", "--fork", "--print-address=1", "--print-pid=1"],
                                 capture_output=True, text=True, check=True).stdout.split()
            address, self.dbus = out[0], int(out[1])
            env.pop("DISPLAY", None)
            env.pop("WAYLAND_DISPLAY", None)
            env["XDG_SESSION_TYPE"] = "wayland"
            env["DBUS_SESSION_BUS_ADDRESS"] = address
            session = self.root / "session.sh"
            (self.root / "kwincfg").mkdir(exist_ok=True)
            session.write_text("#!/usr/bin/env bash\nexport XDG_CONFIG_HOME=%s\nexec %s app\n" % (self.root / "config", self.binary))
            session.chmod(0o755)
            kenv = dict(env)
            kenv["XDG_CONFIG_HOME"] = str(self.root / "kwincfg")
            kenv["QT_QPA_PLATFORM"] = "offscreen"
            self.gamescope = subprocess.Popen(
                ["kwin_wayland", "--virtual", "--no-lockscreen", "--width", "1280", "--height", "900",
                 "--socket", f"zzm-kwin-{os.getpid()}", "--exit-with-session", str(session)],
                env=kenv, stdout=open(self.root / "kwin.log", "w"), stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        else:
            expose = [] if COMPOSITOR == "gamescope-x11" else ["--expose-wayland"]
            self.gamescope = subprocess.Popen(
                ["gamescope", "--backend", "headless", "-W", "1280", "-H", "900", "-w", "1280", "-h", "900",
                 "-r", "120", *expose, "--", str(self.binary), "app"],
                env=env, stdout=open(self.root / "gamescope.log", "w"), stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        deadline = time.time() + 30
        binary = self.binary.resolve()
        while time.time() < deadline and self.gui is None:
            time.sleep(0.2)
            for pid in children(self.gamescope.pid):
                try:
                    argv = Path(f"/proc/{pid}/cmdline").read_bytes().decode().split("\0")
                    exe = Path(f"/proc/{pid}/exe").resolve()
                except OSError:
                    continue
                if exe == binary and "app" in argv and not any(a.startswith("--type=") for a in argv):
                    self.gui = pid
        if self.gui is None:
            raise RuntimeError("GUI process not found")
        deadline = time.time() + 30
        while time.time() < deadline:
            if self.zz("list-clients", "-F", "#{client_name}", check=False):
                break
            time.sleep(0.2)
        time.sleep(2)
        line = self.zz("list-clients", "-F", "#{client_name}\t#{client_session}").splitlines()[0]
        self.client, self.session = line.split("\t")
        previous = self.zz("display-message", "-t", f"{self.session}:", "-p", "#{window_id}")
        self.zz("new-window", "-t", f"{self.session}:", "--kind", "browser", "-n", "browser",
                f"file://{self.root / 'anim.html'}")
        self.zz("kill-window", "-t", previous)
        time.sleep(6)
        if self.shape == "palette":
            self.zz("command-prompt", "-b", "-C", "-t", self.client)
        time.sleep(1)

    def window(self, warmup, duration):
        time.sleep(warmup)
        if cdp_eval("window.__zzn===undefined&&(window.__zzn=0,(function c(){window.__zzn++;requestAnimationFrame(c)})()),window.__zzn") is None:
            raise RuntimeError("CDP not reachable")
        n0 = cdp_eval("window.__zzn")
        main0 = proc_cpu(f"/proc/{self.gui}/task/{self.gui}/stat")
        total0 = proc_cpu(f"/proc/{self.gui}/stat")
        t0 = time.time_ns()
        time.sleep(duration)
        t1 = time.time_ns()
        main1 = proc_cpu(f"/proc/{self.gui}/task/{self.gui}/stat")
        total1 = proc_cpu(f"/proc/{self.gui}/stat")
        n1 = cdp_eval("window.__zzn")
        elapsed = (t1 - t0) / 1e9
        return {
            "t0_ns": t0, "t1_ns": t1, "elapsed_s": elapsed,
            "main_cpu_pct": 100 * (main1 - main0) / elapsed,
            "gui_cpu_pct": 100 * (total1 - total0) / elapsed,
            "browser_frames": (n1 - n0) if isinstance(n0, int) and isinstance(n1, int) else None,
            "main_cpu_ms": 1000 * (main1 - main0),
        }

    def stop(self):
        try:
            self.zz("kill-server", check=False)
        except Exception:
            pass
        if self.gui is not None:
            try:
                os.kill(self.gui, signal.SIGTERM)
            except ProcessLookupError:
                pass
        if self.gamescope is not None:
            if COMPOSITOR == "kwin":
                self.gamescope.terminate()
            try:
                self.gamescope.wait(10)
            except subprocess.TimeoutExpired:
                self.gamescope.kill()
                self.gamescope.wait(5)
        if self.dbus:
            for pid in children(self.dbus)[1:]:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            try:
                os.kill(self.dbus, signal.SIGTERM)
            except ProcessLookupError:
                pass


def gpui_frames(path, w):
    rows = []
    if path.exists():
        for line in open(path):
            try:
                f = json.loads(line)
            except json.JSONDecodeError:
                continue
            if w["t0_ns"] <= f["timestamp_unix_ns"] < w["t1_ns"]:
                rows.append(f)
    out = {"gpui_frames": len(rows)}
    if rows:
        cpu = lambda t: t.get("thread_cpu_ns") if t.get("thread_cpu_ns") is not None else t["wall_ns"]
        out["draw_cpu_ms_per_s"] = sum(cpu(f["draw"]) for f in rows) / 1e6 / w["elapsed_s"]
        out["draw_us_med"] = statistics.median(cpu(f["draw"]) for f in rows) / 1000
    return out


EXTRA_ENV = {}
COMPOSITOR = os.environ.get("MEASURE_COMPOSITOR", "gamescope")
QUIET_STATE = {"ok": True}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", action="append", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--warmup", type=float, default=5)
    ap.add_argument("--duration", type=float, default=20)
    ap.add_argument("--shapes", default="page,palette,glass")
    ap.add_argument("--quiet-wait", type=float, default=900)
    ap.add_argument("--repeat-start", type=int, default=0)
    args = ap.parse_args()
    apps = []
    for entry in args.app:
        label, rest = entry.split("=", 1)
        path, *pairs = rest.split(",")
        apps.append((label, path, dict(pair.split("=", 1) for pair in pairs)))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    results = out / "results.jsonl"
    for repeat in range(args.repeat_start, args.repeat_start + args.repeats):
        order = apps if repeat % 2 == 0 else list(reversed(apps))
        for shape in args.shapes.split(","):
            for label, app, app_env in order:
                root = out / f"run-{label}-{shape}-{repeat}"
                frames = root / "frames.jsonl"
                run = Run(app, root, shape, app_env)
                quiet = wait_quiet(args.quiet_wait if QUIET_STATE["ok"] else 60)
                QUIET_STATE["ok"] = quiet
                try:
                    run.launch(frames)
                    w = run.window(args.warmup, args.duration)
                    busy_after = rustc_busy()
                    record = {"label": label, "shape": shape, "repeat": repeat, "quiet_before": quiet,
                              "rustc_after": busy_after, "load1": os.getloadavg()[0], **w, **gpui_frames(frames, w)}
                    if record["browser_frames"]:
                        record["main_ms_per_browser_frame"] = record["main_cpu_ms"] / record["browser_frames"]
                    with open(results, "a") as handle:
                        handle.write(json.dumps(record) + "\n")
                    print(f"{label} {shape} r{repeat}: main {record['main_cpu_pct']:.1f}% gui {record['gui_cpu_pct']:.1f}% "
                          f"browser_frames {record['browser_frames']} gpui_frames {record['gpui_frames']} "
                          f"ms/bf {record.get('main_ms_per_browser_frame', 0):.3f} quiet {quiet}", flush=True)
                finally:
                    run.stop()
                time.sleep(2)


def summarize(path):
    rows = [json.loads(line) for line in open(path)]
    labels = list(dict.fromkeys(r["label"] for r in rows))
    shapes = list(dict.fromkeys(r["shape"] for r in rows))
    for key in ("main_cpu_pct", "main_ms_per_browser_frame", "browser_frames", "gpui_frames", "gui_cpu_pct", "draw_cpu_ms_per_s"):
        print(f"\n{key}")
        for shape in shapes:
            cells = []
            for label in labels:
                vals = [r.get(key) for r in rows if r["label"] == label and r["shape"] == shape and r.get(key) is not None]
                if vals:
                    cells.append(f"{label}: {statistics.median(vals):.3f} [{min(vals):.3f}-{max(vals):.3f}]")
            print(f"  {shape:8} " + "   ".join(cells))
    noisy = [r for r in rows if not r["quiet_before"] or r["rustc_after"]]
    print(f"\nwindows {len(rows)}, not quiet {len(noisy)}")


if __name__ == "__main__":
    if sys.argv[1:2] == ["summarize"]:
        summarize(sys.argv[2])
    else:
        for pair in os.environ.get("MEASURE_ENV", "").split(","):
            if "=" in pair:
                k, v = pair.split("=", 1)
                EXTRA_ENV[k] = v
        main()
