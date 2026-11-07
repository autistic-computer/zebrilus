from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    probe = ROOT / "target/port/appearance-runtime-probe"
    probe.parent.mkdir(parents=True, exist_ok=True)
    compile_command = 'mojo build --fp-mode=contract=off -I src -I "$1" $MJ_LINK "$1/tests/test_appearance_runtime.mojo" -o "$2"'
    subprocess.run(["pixi", "run", "bash", "-c", compile_command, "_", str(ROOT / "mojo"), str(probe)], cwd=ROOT.parent / "mjui", check=True, timeout=300)
    subprocess.run([str(probe)], cwd=ROOT.parent / "mjui", check=True, timeout=30)


if __name__ == "__main__":
    main()
