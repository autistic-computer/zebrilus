import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    probe = TARGET / "path-daemon-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/path_daemon_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    backend = TARGET / "path-backend-probe"
    compile_command = 'mojo build --fp-mode=contract=off -I src -I "$1" $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/tests/path_backend_probe.mojo" -o "$2"'
    subprocess.run(["pixi", "run", "bash", "-c", compile_command, "_", str(PORT), str(backend)], cwd=ROOT.parent / "mjui", check=True)
    native = ROOT / "target/debug/zz_cli"
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    with tempfile.TemporaryDirectory(prefix="zzpath-daemon-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        files = directory / "files"
        files.mkdir()
        (files / "alpha.txt").write_text("alpha\n")
        (files / "quote's space 世界").write_text("unicode\n")
        (files / "sub").mkdir()
        (files / "sub/nested.txt").write_text("nested\n")
        (files / "link").symlink_to("alpha.txt")
        try:
            subprocess.run([str(native), "-S", str(socket), "-f", "/dev/null", "new-session", "-d", "-s", "path-proof", "sleep 120"], cwd=files, env=environment, check=True, capture_output=True, timeout=30)
            subprocess.run([str(probe), str(socket), str(files)], cwd=files, env=environment, check=True, timeout=30)
            subprocess.run([str(backend), str(socket), str(files)], cwd=files, env=environment, check=True, timeout=60)
            report = {"passed": True, "source": "actual private native daemon", "scope": "typed path root/entries/symlink, native error, cancellation/replacement, exact-once worker handoff, picker notifications, attachment close and reconnect provenance"}
            (TARGET / "path-list-daemon.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(native), "-S", str(socket), "kill-server"], cwd=files, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
