import os
import time
from collections import defaultdict
from pathlib import Path

import psutil


def tree(root_pid, daemon_pid, headless):
    root = psutil.Process(root_pid)
    descendants = [root, *root.children(recursive=True)]
    if headless:
        hubs = [root]
    else:
        hubs = [p for p in descendants if p.name() == "Hub"]
    engine = {p.pid for hub in hubs for p in [hub, *hub.children(recursive=True)]}
    support = {}
    if daemon_pid:
        daemon = psutil.Process(daemon_pid)
        support = {p.pid: p for p in [daemon, *daemon.children(recursive=True)]}
    members = {p.pid: p for p in descendants} | support
    result = {}
    for pid, proc in members.items():
        if pid in engine:
            role = "engine/" + proc.name()
        elif pid in support:
            role = "support/daemon" if pid == daemon_pid else "support/pty"
        elif pid == root_pid:
            role = "host"
        else:
            kind = next(
                (arg[7:] for arg in proc.cmdline() if arg.startswith("--type=")), None
            )
            role = "engine/" + kind if kind else "host/" + proc.name()
        result[(pid, proc.create_time())] = (role, proc)
    return result


def counters(members):
    result = {}
    for key, (role, p) in members.items():
        cpu = p.cpu_times()
        result[key] = (role, cpu.user + cpu.system, p.io_counters().read_chars)
    return result


def delta(before, after):
    if before.keys() != after.keys():
        raise RuntimeError(
            f"Process membership changed during measurement: {before.keys() ^ after.keys()}"
        )
    cpu = defaultdict(float)
    reads = defaultdict(int)
    for key, (role, end_cpu, end_reads) in after.items():
        _, start_cpu, start_reads = before[key]
        cpu[role] += end_cpu - start_cpu
        reads[role] += end_reads - start_reads
    return {
        "cpu_s": {k: round(v, 4) for k, v in cpu.items()},
        "read_bytes": dict(reads),
    }


def memory(members):
    roles = defaultdict(lambda: [0, 0])
    for (pid, _), (role, _) in members.items():
        lines = Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines()
        fields = {
            line.split(":")[0]: int(line.split()[1])
            for line in lines
            if line.startswith(("Pss:", "SwapPss:"))
        }
        roles[role][0] += fields["Pss"]
        roles[role][1] += fields["SwapPss"]
    return dict(roles)


def pressure():
    totals = {}
    for resource in ("cpu", "memory"):
        for line in Path(f"/proc/pressure/{resource}").read_text().splitlines():
            kind, *fields = line.split()
            totals[resource + "_" + kind + "_us"] = int(fields[-1].split("=")[1])
    for line in Path("/proc/vmstat").read_text().splitlines():
        name, value = line.split()
        if name in ("pswpin", "pswpout"):
            totals[name] = int(value)
    return totals


def quiet():
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        busy = psutil.cpu_percent(interval=1)
        psi = float(
            Path("/proc/pressure/memory")
            .read_text()
            .splitlines()[0]
            .split()[1]
            .split("=")[1]
        )
        if busy < 40 and psi < 5:
            return {
                "cpu_busy_pct": busy,
                "memory_psi_avg10": psi,
                "load_1m": os.getloadavg()[0],
            }
    raise RuntimeError(
        "Host did not meet the CPU and memory pressure limits in 120 seconds"
    )
