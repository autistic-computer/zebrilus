from pathlib import Path
import subprocess


root = Path(__file__).resolve().parents[1]
for suite in ("test_protocol", "test_client_context", "test_terminal_view", "test_bend_gate", "test_terminal_commit", "test_actions"):
    subprocess.run(["mojo", "--fp-mode=contract=off", "-I", ".", "tests/" + suite + ".mojo"], cwd=root, check=True)
