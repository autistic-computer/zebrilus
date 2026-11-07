from pathlib import Path
import subprocess
import sys


root = Path(__file__).resolve().parents[2]
target = root / "target/port/zz-mojo"
target.parent.mkdir(parents=True, exist_ok=True)
temporary = target.with_suffix(".next")
subprocess.run(["mojo", "build", "--fp-mode=contract=off", "main.mojo", "-o", str(temporary)], cwd=root / "mojo", check=True)
temporary.replace(target)
subprocess.run([sys.executable, str(root / "mojo/scripts/resources.py"), str(target), "--backend-only"], check=True)
