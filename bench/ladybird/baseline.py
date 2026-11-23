#!/usr/bin/env python3
import argparse
import contextlib
import fcntl
import hashlib
import http.server
import itertools
import json
import math
import os
import platform
import queue
import random
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

import psutil
from baseline_accounting import counters, delta, memory, pressure, quiet, tree
from PIL import Image

MODES = ("headless", "qt", "zz")
PHASES = ("idle", "canvas", "scroll", "integer", "layout")
CHECKSUMS = {
    "integer": 1005651397,
    "layout": sum((1199 // (6 + i % 7)) * 10 for i in range(48)),
}
SIZES = ((786, 608), (1152, 768))
PAGE = Path(__file__).with_name("baseline-page.html").read_bytes()


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/next":
            command = self.server.commands.get(timeout=60)
            data = json.dumps(command).encode()
            mime = "application/json"
        elif self.path == "/":
            data, mime = PAGE, "text/html; charset=utf-8"
        else:
            data, mime = b"", "image/x-icon"
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        with contextlib.suppress(BrokenPipeError, ConnectionResetError):
            self.wfile.write(data)

    def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.server.events.put((self.path, payload, time.monotonic()))
        self.send_response(204)
        self.end_headers()

    def log_message(self, *_):
        pass


@contextlib.contextmanager
def fixture():
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture) as server:
        server.commands = queue.Queue()
        server.events = queue.Queue()
        worker = threading.Thread(
            target=server.serve_forever, args=(0.05,), daemon=True
        )
        worker.start()
        try:
            yield server
        finally:
            server.commands.put({})
            server.shutdown()
            worker.join()


def event(server, expected, timeout):
    kind, value, when = server.events.get(timeout=timeout)
    if kind != expected:
        raise RuntimeError(f"Expected {expected}, received {kind}: {value}")
    return value, when


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def isolated_env(folder, parent):
    env = dict(parent)
    for key in (
        "ZZ_SESSION",
        "ZZ_PANE",
        "ZZ_SOCKET",
        "ZZ_BROWSER_GPU",
        "ZZ_BROWSER_SHARED_TEXTURE",
        "TMUX",
        "TMUX_PANE",
        "DISPLAY",
    ):
        env.pop(key, None)
    for key, name in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("ZZ_LOG_DIR", "logs"),
    ):
        sub = folder / name
        sub.mkdir()
        env[key] = str(sub)
    return env


def launch_zz(bundle, env, sock, log, outer):
    config = Path(env["XDG_CONFIG_HOME"]) / "zz-dev"
    data = Path(env["XDG_DATA_HOME"]) / "zz-dev"
    config.mkdir()
    data.mkdir()
    if outer:
        state = {
            "version": 1,
            "mode": "windowed",
            "bounds": {"x": 0, "y": 0, "width": outer[0], "height": outer[1]},
        }
        (data / "window-state.json").write_text(json.dumps(state))
    (config / "config").touch()
    (data / "import-prompted-v2").touch()
    env["ZZ_LADYBIRD_HUB"] = str(bundle / "runtime/libexec/Hub")
    return subprocess.Popen(
        [bundle / "zz", "--socket", sock, "app"], env=env, stdout=log, stderr=log
    )


def cli(bundle, env, sock, *args, check=True):
    return subprocess.run(
        [bundle / "cli", "--socket", sock, *args],
        env=env,
        capture_output=True,
        text=True,
        timeout=10,
        check=check,
    )


def wait_for_desktop(app, bundle, env, sock):
    end = time.monotonic() + 30
    identity = Path(str(sock) + ".identity")
    while time.monotonic() < end:
        if app.poll() is not None:
            raise RuntimeError(f"Desktop exited with status {app.returncode}")
        if (
            identity.exists()
            and cli(bundle, env, sock, "list-clients", check=False).stdout.strip()
        ):
            ids = cli(
                bundle,
                env,
                sock,
                "list-panes",
                "-a",
                "-F",
                "#{session_id}",
                check=False,
            ).stdout.splitlines()
            if ids:
                daemon = next(
                    row
                    for row in identity.read_text().splitlines()
                    if row.startswith("pid=")
                )
                return int(daemon.split("=")[1]), ids[0]
        time.sleep(0.1)
    raise TimeoutError("Desktop failed to attach")


def phase(root, daemon, mode, kwin, name, server, frames, milliseconds, viewport):
    members = tree(root, daemon, mode == "headless")
    checkpoints = [memory(members)]
    before = counters(members)
    compositor = psutil.Process(kwin)

    def compositor_cpu():
        c = compositor.cpu_times()
        return c.user + c.system

    kwin_start = compositor_cpu()
    start = time.monotonic()
    if server:
        server.commands.put({"phase": name, "frames": frames, "idleMs": milliseconds})
    deadline = start + milliseconds / 1000
    tick = start + 1
    payload = None
    while True:
        now = time.monotonic()
        if now - start > 30:
            raise TimeoutError(f"{mode} {name} did not finish")
        if server:
            try:
                payload, end = event(server, "/done", max(0, tick - now))
                break
            except queue.Empty:
                pass
        elif now >= deadline:
            end = now
            break
        else:
            time.sleep(max(0, min(tick, deadline) - now))
            if time.monotonic() >= deadline:
                end = time.monotonic()
                break
        checkpoints.append(memory(members))
        tick += 1
    kwin_cpu = compositor_cpu() - kwin_start
    after_members = tree(root, daemon, mode == "headless")
    result = delta(before, counters(after_members))
    checkpoints.append(memory(after_members))
    result.update(
        wall_s=round(end - start, 4),
        kwin_cpu_s=round(kwin_cpu, 4),
        memory_kib=checkpoints,
    )
    if payload:
        times = payload["times"]
        if (
            payload["phase"] != name
            or payload["geometry"] != [*viewport, 1, "visible"]
            or payload["y"] != (frames * 16 if name in ("scroll", *CHECKSUMS) else 0)
            or len(times) != (frames if name in ("canvas", "scroll") else 0)
            or payload["checksum"] != CHECKSUMS.get(name)
        ):
            raise ValueError(f"Unmatched work or geometry: {name}, {payload}")
        if name in CHECKSUMS:
            result.update(work_ms=payload["elapsedMs"], checksum=payload["checksum"])
        if times:
            ordered = sorted(times)
            result.update(
                callback_intervals_ms=[round(t, 4) for t in times],
                callbacks=len(times),
                callback_hz=round(1000 / statistics.mean(times), 2),
                p95_ms=ordered[math.ceil(0.95 * len(times)) - 1],
                p99_ms=ordered[math.ceil(0.99 * len(times)) - 1],
                late_callbacks=sum(t > 25 for t in times),
            )
    return result


def screen_probe(env, path, point=None):
    subprocess.run(
        ["spectacle", "-b", "-n", "-f", "-o", str(path)],
        env=env,
        check=True,
        timeout=15,
        capture_output=True,
    )
    with Image.open(path) as src:
        img = src.convert("RGB")
        if point is None:
            marker = bytes((38, 57, 71))
            offset = img.tobytes().find(marker * 60)
            if offset < 0 or offset % 3:
                raise ValueError("Canvas marker missing from compositor capture")
            y, x = divmod(offset // 3, img.width)
            point = (x + 15, y + 10)
            expected = (38, 57, 71)
        else:
            expected = (216, 79, 131)
        actual = img.getpixel(point)
        if actual != expected:
            raise ValueError(
                f"Unexpected compositor pixel at {point}: {actual}, expected {expected}"
            )
        return {"point": point, "rgb": actual}


def trial(args, parent, kwin, mode, size, outer, label, calibrate=False):
    start_health = quiet()
    with (
        tempfile.TemporaryDirectory(prefix="zz-lb-", dir="/tmp") as tmp,
        fixture() as server,
        (args.output / f"{label}.log").open("w") as log,
    ):
        folder = Path(tmp)
        env = isolated_env(folder, parent)
        home = folder / "home"
        home.mkdir()
        env.update(
            HOME=str(home),
            SHELL="/bin/sh",
            ENV="/dev/null",
            BASH_ENV="/dev/null",
            HISTFILE="/dev/null",
            ZDOTDIR=str(home),
        )
        sock = folder / "s"
        url = f"http://127.0.0.1:{server.server_port}/"
        daemon = None
        result = {
            "mode": mode,
            "viewport": size,
            "preflight": start_health,
            "phases": {},
        }
        pressure_start = pressure()
        opened = time.monotonic()
        if mode == "zz":
            app = launch_zz(args.bundle, env, sock, log, outer)
        else:
            profile = folder / "profile"
            (profile / "config").mkdir(parents=True)
            settings = {"firstRunComplete": True}
            if outer:
                settings["window"] = {
                    "size": {"width": outer[0], "height": outer[1]},
                    "maximized": False,
                }
            (profile / "config/Settings.json").write_text(json.dumps(settings))
            headless = mode == "headless"
            binary = args.stock / ("libexec/Hub" if headless else "bin/Ladybird")
            options = (
                [
                    "--headless=manual",
                    "--window-width",
                    str(size[0]),
                    "--window-height",
                    str(size[1]),
                ]
                if headless
                else []
            )
            app = subprocess.Popen(
                [
                    binary,
                    *options,
                    "--force-cpu-painting",
                    "--profile-path",
                    profile,
                    url,
                ],
                env=env,
                stdout=log,
                stderr=log,
            )
        try:
            if mode == "zz":
                daemon, session = wait_for_desktop(app, args.bundle, env, sock)
                result["launch_to_attached_ms"] = round(
                    (time.monotonic() - opened) * 1000, 2
                )
                if not calibrate:
                    time.sleep(1.5)
                    result["phases"]["terminal"] = phase(
                        app.pid, daemon, mode, kwin, "terminal", None, 0, 3000, size
                    )
                opened = time.monotonic()
                cli(
                    args.bundle,
                    env,
                    sock,
                    "new-window",
                    "-t",
                    session,
                    "--kind",
                    "browser",
                    url,
                )
            try:
                ready, ready_time = event(server, "/ready", 30)
            except queue.Empty as error:
                raise TimeoutError(
                    f"{label} did not load the fixture in 30 seconds. See {log.name}"
                ) from error
            result["open_to_ready_ms"] = round((ready_time - opened) * 1000, 2)
            result["ready"] = ready
            if calibrate:
                return ready["geometry"]
            if (
                ready["geometry"] != [*size, 1, "visible"]
                or "Ladybird/2026.47.7" not in ready["ua"]
            ):
                raise ValueError(f"Unexpected initial page: {ready}")
            prove_pixels = label.startswith("0-") and mode != "headless"
            if prove_pixels:
                result["canvas_pixel"] = screen_probe(
                    env, args.output / f"{label}-canvas.png"
                )
            time.sleep(1.5)
            members = tree(app.pid, daemon, mode == "headless")
            if sum(role == "engine/Hub" for role, p in members.values()) != 1:
                raise RuntimeError("Expected exactly one Hub")
            if mode == "zz":
                shells = [p for role, p in members.values() if role == "support/pty"]
                if len(shells) != 1 or not Path(shells[0].exe()).samefile("/bin/sh"):
                    raise ValueError(
                        f"Expected one quiescent shell: {[p.name() for p in shells]}"
                    )
            result["inventory"] = [
                {"role": role, "name": p.name(), "pid": p.pid}
                for role, p in members.values()
            ]
            for name in PHASES:
                if name in CHECKSUMS:
                    time.sleep(0.3)
                result["phases"][name] = phase(
                    app.pid,
                    daemon,
                    mode,
                    kwin,
                    name,
                    server,
                    args.frames,
                    args.idle_ms,
                    size,
                )
            if prove_pixels:
                result["scroll_pixel"] = screen_probe(
                    env,
                    args.output / f"{label}-scroll.png",
                    result["canvas_pixel"]["point"],
                )
            result["host_counters"] = {
                k: v - pressure_start[k] for k, v in pressure().items()
            }
            return result
        finally:
            stop(app)
            if mode == "zz" and sock.exists():
                cli(args.bundle, env, sock, "kill-server", check=False)


def calibrate_size(args, env, kwin, mode, size):
    outer = (
        (size[0] + 294, size[1] + 112) if mode == "zz" else (size[0] + 6, size[1] + 115)
    )
    for i in range(4):
        w, h, dpr, visible = trial(
            args,
            env,
            kwin,
            mode,
            size,
            outer,
            f"calibrate-{mode}-{size[0]}x{size[1]}-{i}",
            True,
        )
        if (w, h, dpr, visible) == (*size, 1, "visible"):
            return outer
        outer = (outer[0] + size[0] - w, outer[1] + size[1] - h)
    raise RuntimeError(f"Could not calibrate {mode} to {size}")


def swap_peak(run):
    return max(
        sum(v[1] for v in checkpoint.values())
        for phase in run["phases"].values()
        for checkpoint in phase["memory_kib"]
    )


def suite(args, env, kwin):
    library = args.bundle / "runtime"
    helpers = {p.name: sha(p) for p in (library / "libexec").iterdir() if p.is_file()}
    for name, digest in helpers.items():
        if sha(args.stock / "libexec" / name) != digest:
            raise ValueError(f"Different Ladybird helper: {name}")
    libraries = {
        p.name: sha(p)
        for p in (library / "lib").iterdir()
        if p.is_file() and not p.is_symlink()
    }
    for name, digest in libraries.items():
        if sha(args.stock / "lib" / name) != digest:
            raise ValueError(f"Different Ladybird library: {name}")
    sizes = args.sizes or SIZES
    manifest = args.output / "environment.json"
    ledger = args.output / "blocks.jsonl"
    previous = json.loads(manifest.read_text()) if args.resume else None
    outer = {
        (m, s): previous["outer"][f"{m}-{s[0]}x{s[1]}"]
        if previous
        else calibrate_size(args, env, kwin, m, s)
        for s in sizes
        for m in ("zz", "qt")
    }
    meta = {
        "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "kernel": platform.release(),
        "cpu": next(
            line.split(":")[1].strip()
            for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")
        ),
        "logical_cpus": os.cpu_count(),
        "ram_bytes": psutil.virtual_memory().total,
        "page_bytes": os.sysconf("SC_PAGE_SIZE"),
        "clock_ticks": os.sysconf("SC_CLK_TCK"),
        "hub_and_helpers_sha256": helpers,
        "engine_libraries_sha256": libraries,
        "shell": str(Path("/bin/sh").resolve()),
        "cli_sha256": sha(args.bundle / "cli"),
        "cpu_governor": Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .read_text()
        .strip()
        if Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").exists()
        else None,
        "zz_sha256": sha(args.bundle / "zz"),
        "qt_sha256": sha(args.stock / "bin/Ladybird"),
        "fixture_sha256": hashlib.sha256(PAGE).hexdigest(),
        "kwin_version": subprocess.check_output(
            ["kwin_wayland", "--version"], text=True
        ).strip(),
        "blocks": args.blocks,
        "frames": args.frames,
        "idle_ms": args.idle_ms,
        "seed": args.seed,
        "outer": {f"{m}-{s[0]}x{s[1]}": v for (m, s), v in outer.items()},
    }
    completed = set()
    if previous is not None:
        meta["utc"] = previous["utc"]
        if meta != previous or list(meta["outer"]) != list(previous["outer"]):
            changed = sorted(
                k
                for k in meta.keys() | previous.keys()
                if meta.get(k) != previous.get(k)
            )
            raise ValueError(
                f"Cannot resume with different settings or binaries: {changed}"
            )
        for line in ledger.read_text().splitlines():
            row = json.loads(line)
            if row["accepted"] or row["warmup"]:
                completed.add((row["block"], tuple(row["size"])))
        pending_sizes = [
            s
            for s in sizes
            if any((i, s) not in completed for i in range(args.blocks + 1))
        ]
        if pending_sizes:
            resumed = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
            for size in pending_sizes:
                for mode in ("qt", "zz"):
                    geometry = trial(
                        args,
                        env,
                        kwin,
                        mode,
                        size,
                        outer[mode, size],
                        f"resume-{resumed}-{mode}-{size[0]}x{size[1]}",
                        True,
                    )
                    if geometry != [*size, 1, "visible"]:
                        raise ValueError(
                            f"Viewport changed after resuming: {mode}, {geometry}"
                        )
            with (args.output / "resumptions.jsonl").open("a") as sessions:
                print(
                    json.dumps(
                        {
                            "utc": resumed,
                            "completed_blocks_including_warmups": len(completed),
                            "rechecked_viewports": pending_sizes,
                        }
                    ),
                    file=sessions,
                )
    else:
        manifest.write_text(json.dumps(meta, indent=2) + "\n")
    rng = random.Random(args.seed)
    orders = list(itertools.permutations(MODES))
    rng.shuffle(orders)
    with ledger.open("a" if args.resume else "x") as out:
        for block in range(args.blocks + 1):
            for size in sizes if block % 2 == 0 else sizes[::-1]:
                if (block, size) in completed:
                    continue
                order = (
                    MODES
                    if block == 0
                    else orders[(block - 1 + sizes.index(size)) % len(orders)]
                )
                prefix = f"{block}-{size[0]}x{size[1]}-"
                prior = [
                    int(p.name.removeprefix(prefix).split("-", 1)[0])
                    for p in args.output.glob(prefix + "*-*.log")
                ]
                first = max(prior, default=0) + 1
                for attempt in range(first, first + 4):
                    runs = {}
                    for mode in order:
                        label = f"{prefix}{attempt}-{mode}"
                        print(label, flush=True)
                        runs[mode] = trial(
                            args, env, kwin, mode, size, outer.get((mode, size)), label
                        )
                    peak = max(swap_peak(run) for run in runs.values())
                    row = {
                        "block": block,
                        "warmup": block == 0,
                        "attempt": attempt,
                        "order": order,
                        "size": size,
                        "max_swap_pss_kib": peak,
                        "accepted": peak <= 1024 and block > 0,
                        "runs": runs,
                    }
                    print(json.dumps(row, separators=(",", ":")), file=out, flush=True)
                    if peak <= 1024 or block == 0:
                        break
                else:
                    raise RuntimeError(
                        "Four attempts exceeded 1 MiB of swap PSS. Resume after memory pressure subsides."
                    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", type=Path, default=Path("target/ladybird-app"))
    parser.add_argument("--stock", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--resume",
        action="store_true",
        help="Continue a saved run with the same settings and binaries",
    )
    parser.add_argument("--blocks", type=int, default=6)
    parser.add_argument("--frames", type=int, default=240)
    parser.add_argument("--idle-ms", type=int, default=4000)
    parser.add_argument("--seed", type=int, default=20264707)
    parser.add_argument(
        "--size",
        dest="sizes",
        action="append",
        type=lambda s: tuple(map(int, s.split("x"))),
    )
    parser.add_argument("--private-bus", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.blocks < 0 or not 16 <= args.frames <= 480 or not 0 < args.idle_ms <= 20000:
        parser.error("blocks must be nonnegative, frames 16..480 and idle-ms 1..20000")
    if args.sizes and (
        any(len(s) != 2 or min(s) <= 0 for s in args.sizes)
        or len(set(args.sizes)) != len(args.sizes)
    ):
        parser.error("supply distinct viewports as positive WIDTHxHEIGHT")
    if not args.private_bus:
        os.execvp(
            "dbus-run-session",
            [
                "dbus-run-session",
                "--",
                sys.executable,
                __file__,
                *sys.argv[1:],
                "--private-bus",
            ],
        )
    args.bundle = args.bundle.resolve()
    args.stock = args.stock.resolve()
    args.output = args.output.resolve()
    if args.resume and not all(
        (args.output / f).is_file() for f in ("environment.json", "blocks.jsonl")
    ):
        parser.error("--resume requires an existing environment and block ledger")
    args.output.mkdir(parents=True, exist_ok=args.resume)
    with (
        tempfile.TemporaryDirectory(prefix="zz-lb-display-", dir="/tmp") as tmp,
        (args.output / ".lock").open("a") as lock,
        (args.output / "kwin.log").open("a") as log,
    ):
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        folder = Path(tmp)
        env = isolated_env(folder, os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.update(XDG_RUNTIME_DIR=tmp, QT_QPA_PLATFORM="wayland")
        kwin = subprocess.Popen(
            [
                "kwin_wayland",
                "--virtual",
                "--width",
                "1600",
                "--height",
                "1000",
                "--socket",
                "baseline",
                "--no-lockscreen",
                "--no-global-shortcuts",
            ],
            env=env,
            stdout=log,
            stderr=log,
        )
        try:
            end = time.monotonic() + 15
            while not (folder / "baseline").exists():
                if kwin.poll() is not None or time.monotonic() > end:
                    raise RuntimeError("Private compositor failed to start")
                time.sleep(0.1)
            env["WAYLAND_DISPLAY"] = "baseline"
            suite(args, env, kwin.pid)
        finally:
            stop(kwin)


if __name__ == "__main__":
    main()
