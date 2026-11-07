from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    (ROOT / 'target/port').mkdir(parents=True, exist_ok=True)
    output = ROOT / 'target/port/browser-controller-probe'
    library = ROOT / 'mojo/native/browser-url/target/release/libzzbrowserurl.a'
    subprocess.run(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path', str(ROOT / 'mojo/native/browser-url/Cargo.toml')], cwd=ROOT, check=True)
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/browser_controller_probe.mojo'), '-Xlinker', str(library), '-Xlinker', '-ldl', '-Xlinker', '-lpthread', '-Xlinker', '-lm', '-o', str(output)], cwd=ROOT, check=True)
    subprocess.run([str(output)], cwd=ROOT, check=True, timeout=30)


if __name__ == '__main__':
    main()
