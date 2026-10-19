import argparse
import os
import re
import statistics
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from ptyclient import PtyClient

ESCAPES = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)?|\x1b[P^_].*?\x1b\\|\x1b[()*+#%][ -~]|\x1b[ -~]|[\x00-\x1f\x7f]", re.DOTALL)
LETTERS = b"abcdefghijklmnopqrstuvwxyz"
CARRY = 256


def token(key):
    return bytes(LETTERS[(key - 97 + 5 * i) % 26] for i in range(6))


class Clients:
    clients = set()


def wait_for(client, wanted, timeout):
    deadline = time.perf_counter() + timeout
    carry = b""
    received = 0
    while True:
        remaining = deadline - time.perf_counter()
        if remaining <= 0 or client.closed:
            return None, received
        data = client._read_once(min(remaining, 0.05))
        if not data:
            continue
        received += len(data)
        chunk = carry + data
        if wanted in ESCAPES.sub(b"", chunk):
            return time.perf_counter(), received
        carry = chunk[-CARRY:]


def drain(client, seconds):
    deadline = time.perf_counter() + seconds
    received = 0
    while not client.closed:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            break
        data = client._read_once(min(remaining, 0.05))
        if data:
            received += len(data)
    return received


def main():
    parser = argparse.ArgumentParser(description="keystroke echo while a flood.py pane redraws")
    parser.add_argument("--zz", required=True)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--session", default="flood")
    parser.add_argument("--flow", choices=("on", "off"), required=True)
    parser.add_argument("--keys", type=int, default=60)
    parser.add_argument("--gap", type=float, default=0.3)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--warmup", type=float, default=5.0)
    parser.add_argument("--label", default="")
    parser.add_argument("--cols", type=int, default=120)
    parser.add_argument("--rows", type=int, default=40)
    args = parser.parse_args()
    env = dict(os.environ, ZZ_SOCKET=args.socket, ZZ_FLOW_WINDOW=args.flow, TERM="xterm-256color")
    client = PtyClient([args.zz, "attach-session", "-t", args.session], env, Clients(), cols=args.cols, rows=args.rows)
    received = drain(client, args.warmup)
    started = time.perf_counter()
    samples = []
    missed = 0
    for index in range(args.keys):
        key = 98 + index % 25
        t0 = time.perf_counter()
        os.write(client.fd, bytes([key]))
        seen, got = wait_for(client, token(key), args.timeout)
        received += got
        if seen is None:
            missed += 1
        else:
            samples.append((seen - t0) * 1000)
        received += drain(client, args.gap)
    elapsed = time.perf_counter() - started
    client.kill()
    samples.sort()
    quantile = lambda q: samples[min(len(samples) - 1, int(q * len(samples)))] if samples else float("nan")
    print(
        f"{args.label} flow={args.flow} keys={args.keys} missed={missed} "
        f"p50_ms={quantile(0.5):.1f} p90_ms={quantile(0.9):.1f} max_ms={(samples[-1] if samples else float('nan')):.1f} "
        f"mean_ms={(statistics.fmean(samples) if samples else float('nan')):.1f} "
        f"tty_kib_s={received / 1024 / max(elapsed, 1e-9):.0f}"
    )


if __name__ == "__main__":
    main()
