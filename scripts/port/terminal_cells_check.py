#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import runpy
import selectors
import struct
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
PACKED = struct.Struct("<II")


def packed(cells):
    return b"".join(PACKED.pack(*cell) for cell in cells)


def worker():
    return subprocess.Popen([str(TARGET / "zz-bend"), "serve"], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def command(paths, header, spans):
    values = ["cells", *map(str, paths), *map(str, header)]
    values.extend(str(value) for span in spans for value in span)
    return (" ".join(values) + "\n").encode()


def request(process, data, timeout=10):
    start = time.perf_counter()
    process.stdin.write(data)
    process.stdin.flush()
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        if not selector.select(timeout):
            process.kill()
            raise RuntimeError("terminal cell worker response timed out")
    response = process.stdout.readline()
    if not response:
        raise RuntimeError(process.stderr.read().decode())
    return int(response), time.perf_counter() - start


def close(process):
    process.stdin.close()
    status = process.wait(timeout=10)
    if status != 0:
        raise RuntimeError(f"terminal cell worker exited {status}: {process.stderr.read().decode()}")


def memory(pid):
    values = {}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith(("VmHWM:", "VmRSS:")):
            key, value, _ = line.split()
            values[key[:-1]] = int(value) * 1024
    return values


def profile(paths, template):
    cases = []
    for columns, rows in [(80, 24), (200, 80), (400, 120), (1000, 500), (2000, 1000), (4096, 2048)]:
        count = columns * rows
        header = list(template)
        header[5], header[6] = columns, rows
        header[19], header[20] = columns, rows
        header[26], header[27], header[28] = 1, 1, 0
        cell = PACKED.pack(65, 0xfeef0000)
        paths[0].write_bytes(cell * count)
        paths[1].write_bytes(b"")
        process = worker()
        status, seconds = request(process, command(paths, header, [[0, 0, 0, 1]]), timeout=120)
        assert status == 0, status
        expected = bytes(columns * 8) + cell * (count - columns)
        assert paths[2].read_bytes() == expected
        usage = memory(process.pid)
        close(process)
        cases.append({"columns": columns, "rows": rows, "cells": count,
                      "input_bytes": count * 8, "seconds": round(seconds, 6),
                      "worker_peak_bytes": usage["VmHWM"],
                      "peak_to_input_ratio": round(usage["VmHWM"] / (count * 8), 2)})
    return cases


def main():
    runpy.run_path(str(Path(__file__).with_name("guard.py")))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", action="store_true")
    args = parser.parse_args()
    oracle = json.loads((TARGET / "terminal-cells-oracle.json").read_text())
    assert oracle["reference"] == "zz_terminal::TerminalViewport::apply_patch.cells"
    compared = 0
    accepted = 0
    with tempfile.TemporaryDirectory(prefix="zz-bend-cells-") as directory:
        paths = [Path(directory) / name for name in ("current.bin", "changed.bin", "output.bin")]
        process = worker()
        for vector in oracle["vectors"]:
            paths[0].write_bytes(packed(vector["current"]))
            paths[1].write_bytes(packed(vector["changed"]))
            paths[2].write_bytes(b"unchanged")
            status, _ = request(process, command(paths, vector["header"], vector["spans"]))
            assert status == vector["status"], vector["name"]
            if status == 0:
                assert paths[2].read_bytes() == packed(vector["after"]), vector["name"]
                accepted += 1
                compared += len(vector["after"])
            else:
                assert paths[2].read_bytes() == b"unchanged", vector["name"]
        close(process)
        valid = next(vector for vector in oracle["vectors"] if vector["status"] == 0 and vector["current"])
        paths[0].write_bytes(packed(valid["current"])[:-1])
        paths[1].write_bytes(packed(valid["changed"]))
        paths[2].write_bytes(b"unchanged")
        malformed = subprocess.run([str(TARGET / "zz-bend"), "serve"],
                                   input=command(paths, valid["header"], valid["spans"]),
                                   capture_output=True, timeout=10)
        assert malformed.returncode == 65, malformed.stderr.decode()
        assert paths[2].read_bytes() == b"unchanged"
        changed_valid = next(vector for vector in oracle["vectors"] if vector["status"] == 0 and vector["changed"])
        paths[0].write_bytes(packed(changed_valid["current"]))
        paths[1].write_bytes(packed(changed_valid["changed"])[:-1])
        malformed_changed = subprocess.run([str(TARGET / "zz-bend"), "serve"],
                                           input=command(paths, changed_valid["header"], changed_valid["spans"]),
                                           capture_output=True, timeout=10)
        assert malformed_changed.returncode == 65, malformed_changed.stderr.decode()
        assert paths[2].read_bytes() == b"unchanged"
        oversized = list(valid["header"])
        oversized[5], oversized[6] = 65535, 129
        oversized[19], oversized[20] = 65535, 129
        oversized[26], oversized[27], oversized[28] = 0, 0, 0
        process = worker()
        status, _ = request(process, command(paths, oversized, []))
        assert status == 4, status
        assert paths[2].read_bytes() == b"unchanged"
        close(process)
        profiles = profile(paths, valid["header"]) if args.profile else []
    report = {"reference": oracle["reference"], "cases": len(oracle["vectors"]),
              "accepted": accepted, "rejected": len(oracle["vectors"]) - accepted,
              "accepted_cells_compared": compared, "malformed_payload_exit": malformed.returncode,
              "malformed_changed_exit": malformed_changed.returncode, "oversized_grid_rejected": True,
              "maximum_read_bytes": 65536, "maximum_input_bytes": 67108864,
              "profile": profiles, "result": "all native cell comparisons passed"}
    destination = TARGET / ("terminal-cells-native-profile.json" if args.profile else "terminal-cells-native.json")
    destination.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
