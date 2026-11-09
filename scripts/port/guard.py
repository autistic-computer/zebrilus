#!/usr/bin/env python3
import fcntl
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time


LIMIT = 3 * 1024 ** 3
RESERVE = 4 * 1024 ** 3


def available_memory():
    if sys.platform == "linux":
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) * 1024
    return None


def limited():
    if sys.platform != "linux":
        return os.environ.get("ZZ_PORT_GUARDED") == "1"
    for line in Path("/proc/self/cgroup").read_text().splitlines():
        if line.startswith("0::"):
            group = Path(line[3:].lstrip("/"))
            value = (Path("/sys/fs/cgroup") / group / "memory.max").read_text().strip()
            return value != "max" and (int(value) <= LIMIT or group.name.startswith("zz-port-"))
    return False


def ensure():
    if not limited():
        os.execv(sys.executable, [sys.executable, str(Path(__file__).resolve()), sys.executable, str(Path(sys.argv[0]).resolve()), *sys.argv[1:]])


def resident_tree(pid):
    result = subprocess.run(["ps", "-axo", "pid=,ppid=,rss="], capture_output=True, text=True, check=True)
    rows = [tuple(map(int, line.split())) for line in result.stdout.splitlines() if line.strip()]
    owned = {pid}
    while True:
        expanded = owned | {child for child, parent, _ in rows if parent in owned}
        if expanded == owned:
            break
        owned = expanded
    return sum(rss * 1024 for child, _, rss in rows if child in owned)


def main():
    def cancelled(number, _frame):
        raise SystemExit(128 + number)

    signal.signal(signal.SIGTERM, cancelled)
    signal.signal(signal.SIGHUP, cancelled)
    command = sys.argv[1:]
    if not command:
        raise SystemExit("usage: guard.py COMMAND [ARG ...]")
    environment = os.environ.copy()
    environment.update({"ZZ_PORT_GUARDED": "1", "CARGO_BUILD_JOBS": "1", "CMAKE_BUILD_PARALLEL_LEVEL": "1", "MAKEFLAGS": "-j1", "RAYON_NUM_THREADS": "1", "LLVM_NUM_THREADS": "1", "OMP_NUM_THREADS": "1"})
    if limited():
        os.execvpe(command[0], command, environment)
    cache = Path.home() / ".cache/zz-port"
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "build.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("Waiting for the shared build slot", file=sys.stderr, flush=True)
            fcntl.flock(lock, fcntl.LOCK_EX)
        if sys.platform == "linux":
            if not shutil.which("systemd-run"):
                raise SystemExit("A user systemd memory scope is required before compiling on this host")
            unit = f"zz-port-{os.getpid()}-{time.time_ns()}.scope"
            available = available_memory()
            budget = min(LIMIT, available - RESERVE) if available is not None else LIMIT
            if budget < 512 * 1024 ** 2:
                raise SystemExit("Build not started: preserving 4 GiB of available device memory")
            high = budget * 7 // 8
            invocation = ["systemd-run", "--user", "--scope", "--quiet", "--nice=19", "--unit=" + unit, "-p", "CPUWeight=10", "-p", "IOWeight=10", "-p", "MemoryHigh=" + str(high), "-p", "MemoryMax=" + str(budget), "-p", "MemorySwapMax=256M", "-p", "TasksMax=128", *command]
        else:
            unit = None
            budget = LIMIT
            invocation = command
        print(f"Build slot acquired: one job, {budget / 1024 ** 3:.2f} GiB memory budget", file=sys.stderr, flush=True)
        environment["ZZ_PORT_BUILD_LOCK_FD"] = str(lock.fileno())
        process = None
        peak = 0
        scope_path = None
        try:
            process = subprocess.Popen(invocation, env=environment, start_new_session=True, pass_fds=(lock.fileno(),))
            if unit:
                while process.poll() is None:
                    if scope_path is None:
                        result = subprocess.run(["systemctl", "--user", "show", unit, "-p", "ControlGroup", "--value"], capture_output=True, text=True, timeout=5)
                        group = result.stdout.strip()
                        if group:
                            scope_path = Path("/sys/fs/cgroup") / group.lstrip("/")
                    if scope_path is not None:
                        try:
                            peak = max(peak, int((scope_path / "memory.peak").read_text()))
                        except (OSError, ValueError):
                            pass
                    available = available_memory()
                    if available is not None and available < RESERVE // 2:
                        raise RuntimeError("Build stopped to preserve available device memory")
                    time.sleep(0.2)
                status = process.returncode
            else:
                while process.poll() is None:
                    peak = max(peak, resident_tree(process.pid))
                    if peak > LIMIT:
                        os.killpg(process.pid, signal.SIGTERM)
                        raise RuntimeError("Build exceeded its 3 GiB memory budget")
                    time.sleep(0.2)
                status = process.returncode
        except BaseException:
            if unit:
                subprocess.run(["systemctl", "--user", "stop", unit], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise
        (cache / "last-build.json").write_text(json.dumps({"program": Path(command[0]).name, "memory_limit_bytes": budget, "observed_memory_peak_bytes": peak, "exit_code": status}, indent=2) + "\n")
        print(f"Build finished: observed peak {peak / 1024 ** 2:.0f} MiB; exit {status}", file=sys.stderr, flush=True)
        if unit and status:
            subprocess.run(["systemctl", "--user", "reset-failed", unit], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        raise SystemExit(status if status >= 0 else 128 - status)


if __name__ == "__main__":
    main()
