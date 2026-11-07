import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import time

from path_list_vectors import text, uint


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    result = subprocess.run(["cargo","run","--quiet","--locked","--offline","-p","zz-ui","--no-default-features","--example","path_rows_vectors"],cwd=ROOT,stdout=subprocess.PIPE,check=True,timeout=300)
    vectors = json.loads(result.stdout)
    (TARGET / "path-rows-vectors.json").write_text(json.dumps(vectors)+"\n")
    request = bytearray()
    expected = bytearray()
    records = []
    uint(request,len(vectors["cases"]))
    for case in vectors["cases"]:
        text(request,case["base"])
        text(request,case["filter"])
        uint(request,len(case["chunks"]))
        for chunk in case["chunks"]:
            uint(request,len(chunk))
            for rel,kind,symlink in chunk:
                text(request,rel)
                uint(request,kind)
                request.append(int(symlink))
        uint(request,len(case["marks"]))
        for rel,kind in case["marks"]:
            text(request,rel)
            uint(request,kind)
        record = bytearray()
        uint(record,len(case["rows"]))
        for rel,kind,symlink,name,mark,hits in case["rows"]:
            text(record,rel)
            uint(record,kind)
            record.append(int(symlink))
            text(record,name)
            uint(record,0 if mark is None else mark+1)
            uint(record,len(hits))
            for start,end in hits:
                uint(record,start)
                uint(record,end)
        records.append(record)
        expected.extend(record)
    probe = TARGET / "path-rows-probe"
    subprocess.run(["pixi","run","--manifest-path",str(PORT/"pixi.toml"),"mojo","build","--fp-mode=contract=off","-I",str(PORT),str(PORT/"tests/path_rows_probe.mojo"),"-o",str(probe)],cwd=ROOT,check=True)
    with tempfile.TemporaryDirectory(prefix="zzpath-rows-") as directory:
        source = Path(directory)/"source.bin"
        actual = Path(directory)/"actual.bin"
        source.write_bytes(request)
        started = time.perf_counter()
        subprocess.run([str(probe),str(source),str(actual)],check=True,timeout=180)
        runtime_seconds = time.perf_counter()-started
        output = actual.read_bytes()
        offset = 0
        for index,(case,record) in enumerate(zip(vectors["cases"],records)):
            if output[offset:offset+len(record)] != record:
                raise AssertionError("path row differs: "+json.dumps({"index":index,"base":case["base"],"filter":case["filter"],"expected":case["rows"]})+" actual="+repr(output[offset:offset+min(len(record),512)]))
            offset += len(record)
        if offset != len(output):
            raise AssertionError("path row output length differs")
    print(json.dumps({"passed":True,"cases":len(vectors["cases"]),"bytes":len(expected),"runtime_seconds":runtime_seconds,"scope":"actual native file_rows full typed rows, marks, duplicate ordering and byte highlights; includes 50,000-entry retained chunks"}))


if __name__ == "__main__":
    main()
