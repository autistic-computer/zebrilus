#!/usr/bin/env python3
import argparse
import http.server
import json
import os
import queue
import statistics
import subprocess
import tempfile
import threading
import time
from pathlib import Path

import psutil
from PIL import Image

PAGE = b"""<!doctype html><meta charset=utf-8><title>Engine comparison</title>
<style>
body{background:#194b70;color:white;font:20px sans-serif;margin:0;padding:32px;height:180vh}
input,button{font:inherit}canvas{display:block;margin-top:24px;max-width:100%}
</style><h1>Browser pane</h1><p>Rendering, input and animation</p>
<input autofocus placeholder="Type here"><button id=next>Continue</button>
<canvas width=640 height=280></canvas><script>
const note=(name,value)=>fetch('/event/'+name,{method:'POST',body:JSON.stringify(value)});
const stored=localStorage.getItem('pane-proof');
localStorage.setItem('pane-proof','seen');
if (location.pathname === '/separate') note('profile', stored);
const box=document.querySelector('input'), ctx=document.querySelector('canvas').getContext('2d');
box.oninput=()=>{document.title=box.value;note('input',box.value)};
next.onclick=()=>{document.title='Clicked';note('click',true)};
onresize=()=>note('resize',[innerWidth,innerHeight]);
const paint=n=>{ctx.fillStyle='#eeaf52';ctx.fillRect(0,0,640,280);
ctx.fillStyle='#263947';ctx.fillRect(n%550,75,90,100)};
paint(0);
function ready(){if(innerWidth<100||innerHeight<100){requestAnimationFrame(ready);return;}
 requestAnimationFrame(()=>note('ready',[innerWidth,innerHeight,navigator.userAgent,stored]))}
requestAnimationFrame(ready);
if(location.pathname==='/motion'){
 let first,prev,steps=[];
 function tick(t){first??=t;if(prev!==undefined)steps.push(t-prev);prev=t;paint((t-first)*0.18);
 if(t-first<4000)requestAnimationFrame(tick);else note('motion',[steps,innerWidth,innerHeight])}
 requestAnimationFrame(tick)
}
if (location.pathname === '/scroll') {
  const list = document.createElement('section');
  list.innerHTML = Array.from({length: 180}, (_, i) =>
    '<article style="background:linear-gradient(100deg,#194b70,#273e5f);padding:12px">' +
    '<strong>Pane ' + (i+1) + '</strong> Navigation, history, and profile storage.</article>'
  ).join('');
  document.body.append(list);
  const frames = [];
  let start, previous;
  function scrollFrame(t) {
    start ??= t;
    if (previous !== undefined) frames.push(t - previous);
    previous = t;
    scrollTo(0, Math.floor((t - start) * 1.25));
    if (t - start < 4000) requestAnimationFrame(scrollFrame);
    else note('scroll', [frames, innerWidth, innerHeight, scrollY]);
  }
  requestAnimationFrame(scrollFrame);
}
if (location.pathname === '/workload') {
  requestAnimationFrame(() => {
    let hash = 1;
    const jsStart = performance.now();
    for (let i = 0; i < 2000000; i++) {
      hash = (Math.imul(hash ^ i, 1664525) + 1013904223) | 0;
    }
    const jsMs = performance.now() - jsStart;
    const list = document.createElement('section');
    list.style.width = '300px';
    document.body.append(list);
    const domStart = performance.now();
    for (let i = 0; i < 800; i++) {
      const child = document.createElement('div');
      child.textContent = 'row ' + i;
      list.append(child);
    }
    let height = 0;
    for (let pass = 0; pass < 10; pass++) {
      list.style.lineHeight = (20 + pass) + 'px';
      height += list.getBoundingClientRect().height;
    }
    note('workload', [jsMs, performance.now() - domStart, hash, height]);
  });
}
</script>"""


class Page(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"" if self.path == "/favicon.ico" else PAGE
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        payload = self.rfile.read(int(self.headers["Content-Length"]))
        self.server.events.put(
            (self.path.rsplit("/", 1)[-1], json.loads(payload), time.monotonic())
        )
        self.send_response(204)
        self.end_headers()

    def log_message(self, *_):
        pass


def event(server, name, timeout=20):
    end = time.monotonic() + timeout
    while True:
        kind, value, when = server.events.get(
            timeout=max(0.001, end - time.monotonic())
        )
        if kind == name:
            return value, when


def process_tree(pid, daemon):
    app = psutil.Process(pid)
    processes = [app]
    for child in app.children():
        if child.pid != daemon:
            processes += [child, *child.children(recursive=True)]
    return processes


def sample(pid, daemon):
    processes = process_tree(pid, daemon)
    kb, swap_kb, cpu = 0, 0, 0
    names = []
    for p in processes:
        try:
            mem = Path(f"/proc/{p.pid}/smaps_rollup").read_text().splitlines()
            kb += int(next(row for row in mem if row.startswith("Pss:")).split()[1])
            swap_kb += int(
                next(row for row in mem if row.startswith("SwapPss:")).split()[1]
            )
            times = p.cpu_times()
            cpu += times.user + times.system
            names.append(p.name())
        except (FileNotFoundError, psutil.NoSuchProcess):
            continue
    return {
        "pss_mib": round(kb / 1024, 2),
        "swap_pss_mib": round(swap_kb / 1024, 2),
        "cpu_s": round(cpu, 2),
        "processes": names,
    }


def swap_counters():
    rows = (row.split() for row in Path("/proc/vmstat").read_text().splitlines())
    return {key: int(value) for key, value in rows if key in {"pswpin", "pswpout"}}


def run(binary, cli, hub, directory, readback):
    directory.mkdir()
    for folder in ["data/zz-dev", "config/zz-dev", "cache", "logs"]:
        (directory / folder).mkdir(parents=True)
    (directory / "data/zz-dev/import-prompted-v2").touch()
    (directory / "config/zz-dev/config").touch()
    env = os.environ.copy()
    for key in [
        "ZZ_PANE",
        "ZZ_SESSION",
        "ZZ_SOCKET",
        "TMUX",
        "TMUX_PANE",
        "ZZ_BROWSER_GPU",
        "ZZ_BROWSER_SHARED_TEXTURE",
    ]:
        env.pop(key, None)
    env.update(
        XDG_DATA_HOME=str(directory / "data"),
        XDG_CONFIG_HOME=str(directory / "config"),
        XDG_CACHE_HOME=str(directory / "cache"),
        ZZ_LOG_DIR=str(directory / "logs"),
        ZZ_LADYBIRD_HUB=str(hub),
    )
    if readback:
        env["ZZ_BROWSER_SHARED_TEXTURE"] = "0"
    with (
        tempfile.TemporaryDirectory(prefix="zz-lb-", dir="/tmp") as tmp,
        http.server.ThreadingHTTPServer(("127.0.0.1", 0), Page) as server,
    ):
        server.events = queue.Queue()
        threading.Thread(target=server.serve_forever, daemon=True).start()
        base = f"http://127.0.0.1:{server.server_port}"
        sock = str(Path(tmp) / "s")

        def command(*args, check=True):
            return subprocess.run(
                [cli, "--socket", sock, *args],
                env=env,
                capture_output=True,
                text=True,
                check=check,
                timeout=15,
            )

        with (directory / "stderr.log").open("w") as log:
            app = subprocess.Popen(
                [binary, "--socket", sock, "app"], env=env, stdout=log, stderr=log
            )
        try:
            until = time.monotonic() + 30
            while time.monotonic() < until:
                if app.poll() is not None:
                    raise RuntimeError(f"GUI exited with {app.returncode}")
                if (
                    Path(sock + ".identity").exists()
                    and command("list-clients", check=False).stdout.strip()
                ):
                    panes = (
                        command("list-panes", "-a", "-F", "#{session_id}", check=False)
                        .stdout.strip()
                        .splitlines()
                    )
                    if panes:
                        break
                time.sleep(0.1)
            else:
                raise TimeoutError("GUI attach")
            daemon = int(
                next(
                    s
                    for s in Path(sock + ".identity").read_text().splitlines()
                    if s.startswith("pid=")
                )[4:]
            )
            time.sleep(2)
            start_swap = swap_counters()
            result = {
                "terminal": sample(app.pid, daemon),
                "host_load_1m": os.getloadavg()[0],
            }
            launch = time.monotonic()
            first = command(
                "new-window",
                "-t",
                panes[0],
                "-P",
                "-F",
                "#{pane_id}",
                "--kind",
                "browser",
                base + "/static",
            ).stdout.strip()
            info, ready = event(server, "ready")
            assert info[3] is None, "Expected a fresh profile"
            result.update(first_page_ms=round((ready - launch) * 1000), page=info)
            time.sleep(2)
            result["one_pane"] = sample(app.pid, daemon)
            mapped = Path(f"/proc/{app.pid}/maps").read_text()
            result["libcef_mapped"] = "/libcef.so" in mapped
            assert result["libcef_mapped"] == ("Ladybird" not in info[2])
            if readback or "Ladybird" in info[2]:
                path = directory / "pane.png"
                command("capture-browser", "-t", first, "-o", str(path))
                with Image.open(path) as img:
                    assert img.convert("RGB").getpixel((10, 10)) == (25, 75, 112), (
                        "Unexpected browser pixels"
                    )
                    result["capture_px"] = img.size
            command("send-keys", "-t", first, "-l", "spike")
            text, _ = event(server, "input")
            while text != "spike":
                text, _ = event(server, "input")
            command("send-keys", "-t", first, "Tab", "Enter")
            event(server, "click")
            start = time.monotonic()
            second = command(
                "split-window",
                "-t",
                first,
                "-h",
                "-P",
                "-F",
                "#{pane_id}",
                "--kind",
                "browser",
                base + "/static",
            ).stdout.strip()
            command("select-layout", "-t", first, "even-horizontal")
            split, ready = event(server, "ready")
            assert 100 < split[0] < info[0]
            assert split[3] == "seen", "The second pane lost the profile storage"
            result.update(second_page_ms=round((ready - start) * 1000), split=split[:2])
            time.sleep(2)
            result["two_panes"] = sample(app.pid, daemon)
            if "Ladybird" in info[2]:
                assert result["two_panes"]["processes"].count("Hub") == 1
            start_cpu = sample(app.pid, daemon)["cpu_s"]
            command("set-browser-url", "-t", second, base + "/motion")
            motion, _ = event(server, "motion")
            intervals, *result["animation_viewport"] = motion
            result["animation_fps"] = round(1000 / statistics.mean(intervals), 2)
            result["animation_p95_ms"] = round(
                sorted(intervals)[int(0.95 * (len(intervals) - 1))], 2
            )
            result["animation_cpu_s"] = round(
                sample(app.pid, daemon)["cpu_s"] - start_cpu, 2
            )
            command("set-browser-url", "-t", second, base + "/workload")
            workload, _ = event(server, "workload")
            assert workload[2:] == [-1973320191, 196000], workload
            result["js_ms"], result["dom_layout_ms"] = workload[:2]
            command("resize-pane", "-Z", "-t", second)
            time.sleep(0.4)
            start_cpu = sample(app.pid, daemon)["cpu_s"]
            command("set-browser-url", "-t", second, base + "/scroll")
            scroll, _ = event(server, "scroll")
            steps, width, height, position = scroll
            assert width >= info[0] - 5 and position > 4000, scroll[1:]
            result["scroll_viewport"] = [width, height]
            result["scroll_fps"] = round(1000 / statistics.mean(steps), 2)
            result["scroll_p95_ms"] = round(
                sorted(steps)[int(0.95 * (len(steps) - 1))], 2
            )
            result["scroll_cpu_s"] = round(
                sample(app.pid, daemon)["cpu_s"] - start_cpu, 2
            )
            command(
                "new-window",
                "-t",
                panes[0],
                "--kind",
                "browser",
                "--profile",
                "separate",
                base + "/separate",
            )
            stored, _ = event(server, "profile")
            assert stored is None, "Named profiles shared local storage"
            result["host_swap_pages"] = {
                key: end - start_swap[key] for key, end in swap_counters().items()
            }
            return result
        finally:
            app.terminate()
            try:
                app.wait(timeout=8)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait()
            if Path(sock).exists():
                command("kill-server", check=False)
            server.shutdown()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for flag in ["binary", "cli", "hub", "output"]:
        parser.add_argument(
            "--" + flag, type=lambda p: Path(p).resolve(), required=True
        )
    parser.add_argument("--readback", action="store_true")
    args = parser.parse_args()
    result = run(args.binary, args.cli, args.hub, args.output, args.readback)
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result), flush=True)
