from pathlib import Path
import subprocess
import tempfile


PORT = Path(__file__).resolve().parents[1]
ROOT = PORT.parent


def main():
    rust = ROOT / "target/debug/zz_cli"
    binary = ROOT / "target/port/stream-probe"
    subprocess.run(["mojo", "build", "-I", ".", "stream_probe.mojo", "-o", str(binary)], cwd=PORT, check=True)
    with tempfile.TemporaryDirectory(prefix="zzstream-", dir="/tmp") as directory:
        socket = str(Path(directory) / "s")
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "stream-proof", "printf '\033[31mREDMARK\033[0m'; IFS= read -r line; printf 'ACK:%s' \"$line\"; sleep 30"], check=True, timeout=20)
            subprocess.run([str(binary), socket], check=True, timeout=30)
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
