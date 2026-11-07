import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    environment = os.environ.copy()
    environment.update({"ZZ_BEND": str(TARGET / "zz-bend"), "MJ_A11Y": "0", "MJ_NATIVE_RASTER": "1", "MJ_CC": "/usr/bin/clang", "MJ_CXX": "/usr/bin/clang++", "MJUI_EXAMPLE_BUILD_ONLY": "1", "MJUI_EXAMPLE_OUT": str(TARGET / "appearance-workspace-smoke")})
    subprocess.run(["pixi", "run", "bash", "scripts/run_example.sh", str(ROOT / "mojo/appearance_workspace_smoke.mojo")], cwd=ROOT.parent / "mjui", env=environment, check=True, timeout=600)
    subprocess.run(["python3", str(ROOT / "mojo/scripts/resources.py"), str(TARGET / "appearance-workspace-smoke")], cwd=ROOT, env=environment, check=True, timeout=30)
    rust = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zz-appearance-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "printf 'APPEARANCE-OK\\n'; sleep 60"], env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(TARGET / "appearance-workspace-smoke"), socket, str(TARGET / "appearance-workspace.png")], cwd=directory, env=environment, check=True, timeout=60)
            report = {"passed": True, "source": "standard encoded appearance notifications injected through the idle real Connection.consume; private daemon and WorkspaceView", "scope": "font replacement, cell-height adjustment, asymmetric padding, daemon resize convergence, native cursor policy, pointer selection, visible selection recoloring and color-only font reuse"}
            (TARGET / "appearance-workspace-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
