import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / 'mojo'
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port'
BUILD = 'mojo build --fp-mode=contract=off -j 1 -I src -I "$1" -I "$1/tests" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/tests/editor_pane_probe.mojo" -o "$2"'


def fixtures(root):
    shutil.rmtree(root, ignore_errors=True)
    (root / 'src').mkdir(parents=True)
    (root / 'src/main.rs').write_bytes(b'fn main() {}\n')
    (root / 'notes.md').write_bytes(b'# notes\n')


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    root = Path(os.path.realpath(TARGET / 'editor-pane-fixtures'))
    fixtures(root)
    binary = TARGET / 'editor-pane-probe'
    subprocess.run(['pixi', 'run', 'bash', '-c', BUILD, '_', str(PORT), str(binary)], cwd=MJUI, check=True)
    subprocess.run(['python3', str(PORT / 'scripts/resources.py'), str(binary)], cwd=ROOT, check=True)
    result = subprocess.run([str(binary), str(root)], cwd=MJUI, check=True, capture_output=True, text=True, timeout=120)
    report = {'passed': True, 'scenarios': int(result.stdout.split()[-1]), 'checks': ['scratch pane, open shortcut, picker scan, typed query, Enter opens the file and publishes path and title', 'typed text marks the pane dirty, the save shortcut writes the file and clears the mark, an unfocused pane ignores text', 'a missing file shows the open error, the retry button reloads once the file exists', 'save without a path asks the host for one, save as creates the folder, a blocked or relative path shows the save error and retry keeps it', 'refused descriptors, commands and constructor paths raise typed text', 'picker row click, outside press, cancel command, arrow keys and Escape', 'deck: snapshot sync, changed path, focus routing, refused descriptor raised once, 64-pane limit, retired panes']}
    (TARGET / 'editor-pane.json').write_text(json.dumps(report, indent=2) + '\n')
    shutil.rmtree(root, ignore_errors=True)
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
