import contextlib
import itertools
import json
import os
import select
import signal
import subprocess
import sys
import time
from pathlib import Path

from PIL import Image

BASE = Path(__file__).resolve().parent
PAGE = (
    (BASE / "baseline-page.html")
    .read_bytes()
    .replace(
        b"main().catch",
        (BASE / "present-scroll.js").read_bytes() + b"\npresentScroll().catch",
    )
)


def build_capture(destination):
    xml = Path(
        os.getenv(
            "ZZ_SCREENCAST_XML",
            "/usr/share/plasma-wayland-protocols/zkde-screencast-unstable-v1.xml",
        )
    )
    if not xml.is_file():
        raise FileNotFoundError(
            f"Install plasma-wayland-protocols or set ZZ_SCREENCAST_XML: {xml}"
        )
    for option, name in (
        ("client-header", "screencast-client.h"),
        ("private-code", "screencast-code.c"),
    ):
        subprocess.run(["wayland-scanner", option, xml, destination / name], check=True)
    program = destination / "capture"
    subprocess.run(
        [
            "cc",
            "-I",
            destination,
            BASE / "screencast.c",
            destination / "screencast-code.c",
            "-lwayland-client",
            "-o",
            program,
        ],
        check=True,
    )
    return program


def region(env, path, width):
    subprocess.run(
        ["spectacle", "-b", "-n", "-f", "-o", path],
        env=env,
        capture_output=True,
        timeout=12,
        check=True,
    )
    with Image.open(path) as img:
        img = img.convert("RGB")
        raw = img.tobytes()
        marker = bytes([128]) * (width - 16) * 3
        for y in range(img.height - 8):
            row = y * img.width * 3
            x = raw.find(marker, row, row + img.width * 3)
            if x < 0 or (x - row) % 3:
                continue
            middle = x + 7 * img.width * 3
            if raw.startswith(marker, middle):
                return (x - row) // 3, y
    raise ValueError("Could not locate the grey test viewport")


def record(node, width, filename):
    import gi

    gi.require_version("Gst", "1.0")
    from gi.repository import Gst

    Gst.init(None)
    pipeline = Gst.parse_launch(
        f"pipewiresrc path={int(node)} ! videoconvert ! video/x-raw,format=GRAY8 ! appsink name=frames sync=false"
    )
    frames = pipeline.get_by_name("frames")
    active = True

    def finish(*_):
        nonlocal active
        active = False

    signal.signal(signal.SIGTERM, finish)
    samples = []
    pipeline.set_state(Gst.State.PLAYING)
    try:
        while active:
            sample = frames.emit("try-pull-sample", 200_000_000)
            if sample is None:
                msg = pipeline.get_bus().pop_filtered(Gst.MessageType.ERROR)
                if msg:
                    raise RuntimeError(msg.parse_error())
                continue
            caps = sample.get_caps().get_structure(0)
            if caps.get_value("width") != width or caps.get_value("height") != 8:
                raise ValueError(f"Unexpected capture dimensions: {caps.to_string()}")
            buf = sample.get_buffer()
            data = buf.extract_dup(0, buf.get_size())
            stripe = [data[(2 * i + 1) * width // 26] for i in range(13)]
            if all(80 < v < 175 for v in stripe):
                index = -1
            elif stripe[0] > 195 and all(v < 60 or v > 195 for v in stripe[1:]):
                index = sum((v > 195) << i for i, v in enumerate(stripe[1:]))
            else:
                index = -2
            samples.append([buf.pts, index])
            if len(samples) == 1:
                print(sample.get_caps().to_string(), flush=True)
    finally:
        pipeline.set_state(Gst.State.NULL)
        Path(filename).write_text(json.dumps(samples))


def line(proc):
    if not select.select([proc.stdout], [], [], 12)[0]:
        raise TimeoutError("Private compositor capture did not become ready")
    value = proc.stdout.readline().strip()
    if not value:
        raise RuntimeError(f"Capture process exited: {proc.poll()}")
    return value


def measure(env, folder, capture, server, size, seconds, screenshot):
    from baseline import event, stop

    w = size[0]
    x, y = region(env, screenshot, w)
    with contextlib.ExitStack() as stack:
        node_process = subprocess.Popen(
            [capture, str(x), str(y), str(w), "8"],
            env=env,
            stdout=subprocess.PIPE,
            text=True,
        )
        stack.callback(stop, node_process)
        node = int(line(node_process))
        output = folder / "samples.json"
        decoder = subprocess.Popen(
            [sys.executable, __file__, str(node), str(w), str(output)],
            env=env,
            stdout=subprocess.PIPE,
            text=True,
        )
        stack.callback(stop, decoder)
        caps = line(decoder)
        server.commands.put({"seconds": seconds})
        engine, _ = event(server, "/report", seconds + 8)
        time.sleep(0.3)
    samples = json.loads(output.read_text())
    ids = [(pts, v) for pts, v in samples if v >= 0]
    unique = list(dict.fromkeys(v for _, v in ids))
    if (
        engine["geometry"] != [*size, 1, "visible"]
        or engine["y"] != 16 * ((engine["frames"] - 1) % 500 + 1)
        or engine["tainted"]
        or not unique
        or unique != sorted(unique)
        or unique[-1] >= engine["frames"]
        or any(b < a for (_, a), (_, b) in itertools.pairwise(ids))
        or any(v == -2 for _, v in samples)
    ):
        raise ValueError(
            f"Invalid composited patterns: {engine}, {samples[:12]}, {samples[-6:]}"
        )
    if (
        samples[0][1] != -1
        or samples[-1][1] != -1
        or any(b <= a for (a, _), (b, _) in itertools.pairwise(samples))
    ):
        raise ValueError("Missing idle boundaries or non-monotonic capture timestamps")
    return {
        "engine": engine,
        "caps": caps,
        "region": [x, y, w, 8],
        "samples_pts_ns_index": samples,
        "unique": len(unique),
        "composited_hz": round(1000 * len(unique) / engine["wall_ms"], 2),
    }


if __name__ == "__main__":
    record(sys.argv[1], int(sys.argv[2]), sys.argv[3])
