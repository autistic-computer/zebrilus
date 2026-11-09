from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    binary = TARGET / "window-chooser-component-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/tests/window_chooser_component_probe.mojo" -o "$2"', "_", str(ROOT), str(binary)], cwd=MJUI, check=True)
    subprocess.run([str(binary)], cwd=MJUI, check=True, timeout=60)


if __name__ == "__main__":
    main()
