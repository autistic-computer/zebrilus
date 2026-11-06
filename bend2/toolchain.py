import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import urllib.request


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure():
    manifest = json.loads(Path(__file__).with_name("toolchain.json").read_text())
    system = platform.system().lower()
    machine = platform.machine().lower()
    architecture = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    target = f"{system}-{architecture}"
    cache = Path(os.environ.get("ZZ_PORT_TOOLCHAINS", str(Path.home() / ".cache/zz-port")))
    cache.mkdir(parents=True, exist_ok=True)
    result = {}
    for name, config in manifest.items():
        if target not in config["archives"]:
            raise RuntimeError(f"{name} has no pinned archive for {target}")
        installation = cache / name
        executable = installation / "bin" / name
        if executable.is_file():
            version = subprocess.run([str(executable), "version" if name == "bend" else "--version"],
                                     capture_output=True, text=True, check=True).stdout
            if re.search(r"(?<![0-9.])" + re.escape(config["version"]) + r"(?![0-9.])", version):
                result[name] = str(executable)
                continue
            raise RuntimeError(f"{executable} exists with an unexpected version: {version.strip()}")
        archive_name, digest = config["archives"][target]
        archive = cache / archive_name
        if not archive.is_file() or sha256(archive) != digest:
            temporary = archive.with_suffix(archive.suffix + ".part")
            url = f'{config["repository"]}/releases/download/v{config["version"]}/{archive_name}'
            with urllib.request.urlopen(url) as source, temporary.open("wb") as destination:
                shutil.copyfileobj(source, destination)
            if sha256(temporary) != digest:
                temporary.unlink()
                raise RuntimeError(f"checksum mismatch for {archive_name}")
            temporary.replace(archive)
        with tempfile.TemporaryDirectory(prefix=f"{name}-", dir=cache) as staging:
            unpacked = Path(staging) / "installation"
            unpacked.mkdir()
            subprocess.run(["tar", "-xf", str(archive), "-C", str(unpacked), "--strip-components=1"], check=True)
            if installation.exists():
                raise RuntimeError(f"refusing to replace existing installation: {installation}")
            unpacked.rename(installation)
        result[name] = str(executable)
    return result


if __name__ == "__main__":
    print(json.dumps(ensure()))
