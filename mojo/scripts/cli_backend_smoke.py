import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
PROBE = TARGET / "cli-backend-probe"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/cli_backend_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-backend-", dir="/tmp") as directory:
        directory = Path(directory)
        physical = directory / "physical"
        physical.mkdir()
        logical = directory / "logical"
        logical.symlink_to(physical, target_is_directory=True)
        socket = directory / "s"
        capture = directory / "backend.json"
        config = directory / "config"
        config.write_text("set-option -g @bootstrap-marker BOOTSTRAP_CONFIG\n")
        helper = directory / "backend"
        helper.write_text(
            "#!/usr/bin/python3\n"
            "import os, sys, json, signal, hashlib\n"
            "assert 'daemon' in sys.argv\n"
            "fds = {}\n"
            "for key in os.listdir('/proc/self/fd'):\n"
            "    try: fds[key] = os.readlink('/proc/self/fd/' + key)\n"
            "    except FileNotFoundError: pass\n"
            "record = {'argv':sys.argv, 'fds':fds, 'sid':os.getsid(0), 'pid':os.getpid(), 'blocked':sorted(signal.pthread_sigmask(signal.SIG_BLOCK, [])), 'removed':[key for key in ['ZZ_TMUX_EXECUTABLE','ZZ_APP_STARTUP_DIRECTORY'] if key in os.environ], 'keep':os.environ.get('SPAWN_KEEP'), 'large':hashlib.sha256(os.environ['SPAWN_LARGE'].encode()).hexdigest(), 'cwd':os.getcwd()}\n"
            "with open(os.environ['SPAWN_REPORT'], 'w') as output: json.dump(record, output)\n"
            "os.execv(os.environ['SPAWN_BACKEND'], sys.argv)\n"
        )
        helper.chmod(0o700)
        environment = os.environ.copy()
        environment.update({"ZZ_DAEMON": str(helper), "ZZ_BEND": str(TARGET / "zz-bend"), "PWD": str(logical), "ZZ_TMUX_EXECUTABLE": "MUST_DROP", "ZZ_APP_STARTUP_DIRECTORY": "MUST_DROP", "SPAWN_KEEP": "PRESERVED", "SPAWN_LARGE": "x" * 100000, "SPAWN_REPORT": str(capture), "SPAWN_BACKEND": str(RUST)})
        leak = os.open(directory / "must-not-inherit", os.O_WRONLY | os.O_CREAT, 0o600)
        os.set_inheritable(leak, True)
        try:
            result = subprocess.run([str(PROBE), str(socket), str(config)], cwd=logical, env=environment, pass_fds=(leak,), capture_output=True, check=True, timeout=20)
            server, process = map(int, result.stdout.split())
            record = json.loads(capture.read_text())
            assert record["pid"] == process and record["sid"] == process
            assert record["blocked"] == [] and record["removed"] == []
            assert record["keep"] == "PRESERVED" and record["large"] == hashlib.sha256(b"x" * 100000).hexdigest()
            assert record["cwd"] == str(physical)
            args = record["argv"]
            ready = args[args.index("--bootstrap-ready-fd") + 1]
            assert set(record["fds"]) == {"0", "1", "2", ready}, record
            assert all(record["fds"][str(fd)] == "/dev/null" for fd in range(3))
            assert args[1:6] == ["--socket", str(socket), "-f", str(config), "daemon"]
            assert args[args.index("--bootstrap-server-id") + 1] == str(server)
            assert args[args.index("--bootstrap-client-cwd") + 1] == str(physical)
            response = subprocess.run([str(RUST), "-S", str(socket), "show-options", "-gqv", "@bootstrap-marker"], capture_output=True, check=True, timeout=10)
            assert response.stdout == b"BOOTSTRAP_CONFIG\n", response
            report = {"passed": True, "source": "actual native daemon behind a private argv/environment/fd/signal observer", "checks": ["daemon-only argv and exact config ordering", "bootstrap server identity matches Hello", "readiness descriptor remains inherited", "all other descriptors closed on exec", "standard streams are null", "new detached session", "child signal mask empty and caller restored", "native environment removals", "large inherited value preserved", "physical bootstrap working directory", "actual backend configuration loaded"]}
            (TARGET / "cli-backend-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            os.close(leak)
            subprocess.run([str(RUST), "-S", str(socket), "kill-server"], env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
