import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / 'mojo'
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port'
BUILD = 'mojo build --fp-mode=contract=off -j 1 -I src -I "$1" -I "$1/tests" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/tests/agent_pane_probe.mojo" -o "$2"'


def scenarios(result):
    return int(result.stdout.split()[-1])


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    model = TARGET / 'agent-model-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(PORT / 'pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-j', '1', '-I', str(PORT), '-I', str(PORT / 'tests'), str(PORT / 'tests/agent_model_probe.mojo'), '-o', str(model)], cwd=ROOT, check=True)
    logic = subprocess.run([str(model)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=120)
    pane = TARGET / 'agent-pane-probe'
    subprocess.run(['pixi', 'run', 'bash', '-c', BUILD, '_', str(PORT), str(pane)], cwd=MJUI, check=True)
    subprocess.run(['python3', str(PORT / 'scripts/resources.py'), str(pane)], cwd=ROOT, check=True)
    drawn = subprocess.run([str(pane)], cwd=MJUI, check=True, capture_output=True, text=True, timeout=120)
    report = {'passed': True, 'model_scenarios': scenarios(logic), 'pane_scenarios': scenarios(drawn), 'checks': ['model: prompt, queue and stop effects with offline, read-only, empty, image, queue and outbox refusals', 'model: draft editing, settings, completions, permission and question answers, restored prompts, replay after a gap', 'pane: welcome and offline text, refused provider and command', 'pane: a finished turn draws the prompt, the answer and the fold, the fold and run rows expand on click', 'pane: streamed text grows in place, the live line shows elapsed time, the action button stops the turn', 'pane: key routes, typed text, send by button and by Enter, an unfocused pane passes keys', 'pane: permission and question clicks produce one typed effect each', 'pane: queued prompts and unqueue, wheel stops following, jump and the follow command resume it']}
    (TARGET / 'agent-pane.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
