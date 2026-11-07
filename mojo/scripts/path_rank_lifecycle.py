import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from path_list_vectors import text, uint


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT/"mojo"
TARGET = ROOT/"target/port"


def optional(output,value):
    output.append(int(value is not None))
    if value is not None:
        text(output,value)


def state_bytes(state):
    output=bytearray()
    uint(output,state["generation"])
    output.append(int(state["running"]))
    output.append(int(state["dirty"]))
    uint(output,state["selected"])
    optional(output,state["pending"])
    optional(output,state["wanted"])
    for value in state["path"]:
        text(output,value)
    listing=state["listing"]
    output.append(int(listing is not None))
    if listing is not None:
        directory,request,done,error=listing
        text(output,directory)
        output.append(int(request is not None))
        if request is not None:uint(output,request)
        output.append(int(done))
        optional(output,error)
    uint(output,len(state["rows"]))
    for rel,name in state["rows"]:
        text(output,rel)
        text(output,name)
    uint(output,len(state["calls"]))
    for call in state["calls"]:
        uint(output,call[0]);uint(output,call[1])
        if call[0]==0:text(output,call[2])
    return output


def main():
    runpy.run_path(str(ROOT/"scripts/port/guard.py"))["ensure"]()
    captured=subprocess.run(["cargo","run","--quiet","--locked","--offline","-p","zz-ui","--no-default-features","--features","path-rank-fixtures","--example","path_rank_lifecycle_vectors"],cwd=ROOT,stdout=subprocess.PIPE,check=True,timeout=300)
    vectors=json.loads(captured.stdout)
    (TARGET/"path-rank-lifecycle-vectors.json").write_text(json.dumps(vectors)+"\n")
    request=bytearray()
    expected=bytearray()
    records=[]
    uint(request,len(vectors["traces"]))
    for trace in vectors["traces"]:
        uint(request,len(trace["operations"]))
        for operation in trace["operations"]:
            kind=operation[0];uint(request,kind)
            if kind in [0,1,4]:text(request,operation[1])
            elif kind==2:
                uint(request,len(operation[1]))
                for value in operation[1]:text(request,value)
                request.append(int(operation[2]))
            elif kind==5:request.append(int(operation[1]<0))
        for index,state in enumerate(trace["states"]):
            record=state_bytes(state)
            records.append((trace["seed"],index,state,record))
            expected.extend(record)
    probe=TARGET/"path-rank-lifecycle-probe"
    subprocess.run(["pixi","run","--manifest-path",str(PORT/"pixi.toml"),"mojo","build","--fp-mode=contract=off","-I",str(PORT),str(PORT/"tests/path_rank_lifecycle_probe.mojo"),"-o",str(probe)],cwd=ROOT,check=True)
    with tempfile.TemporaryDirectory(prefix="zzpath-lifecycle-") as directory:
        source=Path(directory)/"source.bin";actual=Path(directory)/"actual.bin"
        source.write_bytes(request)
        subprocess.run([str(probe),str(source),str(actual)],check=True,timeout=120)
        output=actual.read_bytes();offset=0
        for seed,index,state,record in records:
            if output[offset:offset+len(record)]!=record:
                raise AssertionError("path lifecycle differs "+json.dumps({"seed":seed,"step":index,"state":state})+" actual="+repr(output[offset:offset+len(record)]))
            offset+=len(record)
        if offset!=len(output):raise AssertionError("path lifecycle output length differs")
    print(json.dumps({"passed":True,"traces":len(vectors["traces"]),"states":len(records),"bytes":len(expected),"source":vectors["scope"]}))


if __name__=="__main__":main()
