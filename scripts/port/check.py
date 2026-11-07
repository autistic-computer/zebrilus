#!/usr/bin/env python3
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import runpy
import shlex
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(Path(__file__).with_name("guard.py")))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("front", choices=("core", "app", "appearance", "all"), default="all", nargs="?")
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    if Path("/usr/bin/clang").is_file():
        environment["PATH"] = "/usr/bin:" + environment["PATH"]
        environment["CC"] = "/usr/bin/clang"
        environment["CXX"] = "/usr/bin/clang++"
        environment["MJ_CC"] = "/usr/bin/clang"
        environment["MJ_CXX"] = "/usr/bin/clang++"
    report = {"front": args.front, "started": datetime.now(timezone.utc).isoformat(), "scope": "configured evidence checks; full migration and pixel identity remain tracked separately", "stages": []}
    report_path = TARGET / ("verification-" + args.front + ".json")

    def stage(name, command, cwd=ROOT, output=None):
        print(name + ": " + shlex.join(map(str, command)), flush=True)
        log_path = TARGET / ("check-" + name + ".log")
        started = time.monotonic()
        with log_path.open("w") as log:
            if output:
                with output.open("w") as destination:
                    result = subprocess.run(command, cwd=cwd, env=environment, stdout=destination, stderr=log, timeout=1800)
            else:
                result = subprocess.run(command, cwd=cwd, env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=1800)
        report["stages"].append({"name": name, "command": list(map(str, command)), "exit_code": result.returncode, "seconds": round(time.monotonic() - started, 3), "log": str(log_path.relative_to(ROOT))})
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        if result.returncode:
            print(log_path.read_text()[-12000:], flush=True)
            raise SystemExit(f"{name} failed; see {log_path}")
        print(name + ": passed", flush=True)

    if args.front in ("core", "app", "all"):
        tools = runpy.run_path(str(ROOT / "bend2/toolchain.py"))["ensure"]()
        environment["PATH"] = os.pathsep.join([str(Path(tools["bend"]).parent), str(Path(tools["lean"]).parent), environment["PATH"]])
        report["toolchain"] = tools
        stage("proof", [tools["bend"], "bend2/PROOF.bend", "--verdict"])
        for name in ("core", "layout_edit", "replay", "terminal", "terminal_cells", "terminal_cells_origins", "tree_reducer"):
            stage("bend-" + name, [tools["bend"], f"bend2/{name}.bend", "-o", str(TARGET / (name + ".mjs"))])
        stage("bend-native", [tools["bend"], "bend2/bridge.bend", "-o", str(TARGET / "zz-bend.next")])
        (TARGET / "zz-bend.next").replace(TARGET / "zz-bend")
        stage("bend-tree-native", [tools["bend"], "bend2/tree_bridge.bend", "-o", str(TARGET / "zz-bend-tree.next")])
        (TARGET / "zz-bend-tree.next").replace(TARGET / "zz-bend-tree")
        stage("rust-oracle", ["cargo", "run", "--locked", "-p", "zz-client", "--example", "port_oracle"], output=TARGET / "oracle.json")
        stage("core-differential", ["node", "bend2/check.mjs"])
        stage("rust-terminal-plan", ["cargo", "run", "--locked", "-p", "zz-client", "--example", "terminal_plan_oracle"], output=TARGET / "terminal-plan-oracle.json")
        stage("terminal-plan-differential", ["node", "bend2/terminal_check.mjs"])
        stage("rust-terminal-cells", ["cargo", "run", "--locked", "-p", "zz-client", "--example", "terminal_cells_oracle"], output=TARGET / "terminal-cells-oracle.json")
        stage("terminal-cells-differential", ["node", "bend2/terminal_cells_check.mjs"])
        stage("terminal-cells-origins", ["node", "bend2/terminal_cells_origins_check.mjs"])
        stage("terminal-cells-native", ["python3", "scripts/port/terminal_cells_check.py"])
        stage("rust-tree-seed", ["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "control_port_vectors"], output=TARGET / "control-vectors.json")
        stage("rust-tree-oracle", ["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "tree_reducer_oracle"], output=TARGET / "tree-reducer-oracle.json")
        stage("tree-differential", ["node", "bend2/tree_check.mjs"])
        stage("tree-native", ["python3", "scripts/port/tree_check.py"])
        stage("tree-stress", ["node", "bend2/tree_stress.mjs"])

    if args.front in ("app", "all"):
        stage("rust-daemon", ["cargo", "build", "--locked", "-p", "zz-cli"])
        for task in ("build", "test", "test-tree-codec", "differential-tree", "differential-protocol", "differential-client-context", "differential-terminal-appearance", "differential-terminal-render", "differential-terminal-overlays", "test-terminal-render-state", "terminal-pixels", "test-appearance-runtime", "appearance-workspace-smoke", "test-terminal-settings", "differential-terminal-settings", "differential-client-files", "test-client-files", "client-files-smoke", "differential-exec", "test-exec-context", "exec-smoke", "differential-cli-globals", "differential-cli-command-help", "differential-cli-preflight", "differential-palette-fuzzy", "differential-palette-commands", "differential-palette-navigation", "differential-palette-dialogs", "differential-palette-dialog-state", "differential-path-list", "differential-path-text", "test-path-service", "test-path-mailbox", "path-list-daemon-smoke", "differential-path-rank", "differential-path-rows", "test-path-rank-budget", "differential-path-rank-lifecycle", "test-path-rank-job", "differential-palette-files", "test-palette-files-worker", "differential-palette-ui-keys", "palette-files-daemon-smoke", "palette-files-workspace-smoke", "differential-palette-places", "differential-palette-verbs", "test-palette-controller", "differential-palette-model", "differential-palette-shortcuts", "differential-palette-projection", "palette-live-smoke", "differential-palette-component-model", "test-palette-component", "differential-palette-actions", "differential-palette-keys", "palette-backend-smoke", "palette-workspace-smoke", "differential-palette-assets", "test-cli-parse", "cli-globals-smoke", "cli-backend-smoke", "cli-bootstrap-smoke", "differential-cli-default", "test-cli-default-home", "differential-cli-attach", "differential-app-resume", "app-resume-smoke", "differential-control", "differential-terminal", "differential-colors", "differential-layout", "differential-input", "differential-client-events", "differential-client-attachment", "differential-terminal-projection", "test-terminal-history", "test-local-scroll", "test-history-painter", "differential-history", "history-smoke", "history-workspace-smoke", "test-keyboard", "keyboard-smoke", "test-clipboard", "differential-clipboard", "clipboard-host-smoke", "clipboard-workspace-smoke", "test-terminal-search", "test-search-controller", "test-search-chrome", "differential-search", "search-smoke", "search-workspace-smoke", "differential-primary-paste", "test-pointer", "pointer-smoke", "test-command-output", "differential-command-output", "command-output-smoke", "test-painter", "test-readiness", "smoke", "stream-smoke", "state-smoke"):
            stage("mojo-" + task, ["pixi", "run", "--manifest-path", "mojo/pixi.toml", task])
        stage("mojo-cli-default-runtime", ["python3", "mojo/scripts/cli_default_smoke.py"])
        stage("terminal-differential", ["python3", "scripts/port/terminal_differential.py"])
        stage("mojo-ui", ["pixi", "run", "--manifest-path", "mojo/pixi.toml", "ui-smoke"])
        stage("mojo-native-build", ["pixi", "run", "--manifest-path", "mojo/pixi.toml", "build-app"])
        stage("mojo-native", ["pixi", "run", "--manifest-path", "mojo/pixi.toml", "native-smoke"])
        stage("mojo-native-launch", ["python3", "mojo/scripts/app_launch_smoke.py"])

    if args.front in ("appearance", "all"):
        mjui = ROOT.parent / "mjui"
        penna = ROOT.parent / "penna"
        stage("penna-export", ["cargo", "run", "--locked", "--example", "theme_css", "--", str(mjui / "build/penna-tokens")], cwd=penna)
        stage("appearance-values", ["python3", "scripts/port/appearance_differential.py"])
        stage("mjui-controls", ["bash", "scripts/penna/check.sh"], cwd=mjui)
        stage("mjui-native-components", ["bash", "scripts/penna/components_check.sh"], cwd=mjui)
        stage("mjui-native-gallery", ["bash", "scripts/penna/native_gallery_check.sh"], cwd=mjui)
        stage("mjui-native-glass-lifecycle", ["bash", "scripts/penna/glass_lifecycle_check.sh"], cwd=mjui)
        stage("mjui-native-bounds", ["python3", "scripts/penna/bounds_vectors.py"], cwd=mjui)
        stage("mjui-native-scene", ["python3", "scripts/penna/scene_vectors.py"], cwd=mjui)
        stage("mjui-native-replay", ["python3", "scripts/penna/replay_vectors.py"], cwd=mjui)
        stage("mjui-native-path", ["bash", "scripts/penna/path_check.sh"], cwd=mjui)
        stage("terminal-scene-pixels", ["python3", "mojo/scripts/terminal_scene_pixels.py"])
        stage("mjui-native-palette-rows", ["python3", "scripts/penna/palette_rows.py"], cwd=mjui)
        stage("mjui-native-palette-chrome", ["python3", "scripts/penna/palette_chrome_pixels.py"], cwd=mjui)
        stage("mjui-native-palette-surface", ["python3", "scripts/penna/palette_surface_pixels.py"], cwd=mjui)
        stage("mjui-native-palette-focus", ["python3", "scripts/penna/palette_focus_pixels.py"], cwd=mjui)
        stage("mjui-native-text-layers", ["bash", "scripts/penna/text_layer_cache_check.sh"], cwd=mjui)
        stage("mjui-default-textedit", ["bash", "scripts/penna/textedit_check.sh"], cwd=mjui)
        stage("appearance-render", ["bash", "scripts/penna/render.sh"], cwd=mjui)
        stage("appearance-pixels", ["python3", "scripts/port/appearance_pixels.py"])

    report["finished"] = datetime.now(timezone.utc).isoformat()
    report["passed"] = True
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{args.front}: all selected checks passed; {report_path}")


if __name__ == "__main__":
    main()
