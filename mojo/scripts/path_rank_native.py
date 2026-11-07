import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import tomllib


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port/path-rank-native"


EXPOSE = r'''
pub fn zz_path_vector_bytes() -> u32 {
    #[cfg(target_arch="x86_64")]
    {
        if <prefilter::backend::PrefilterAVX512 as prefilter::Kernel>::is_available() && <smith_waterman::SmithWatermanAVX512 as smith_waterman::Kernel>::is_available() {return 64;}
        if <prefilter::backend::PrefilterAVX as prefilter::Kernel>::is_available() && <smith_waterman::SmithWatermanAVX as smith_waterman::Kernel>::is_available() {return 32;}
    }
    16
}

fn zz_path_debug_impl<P: prefilter::Kernel, S: smith_waterman::Kernel>(query: &str, label: &str) -> (bool,usize,usize,u16,bool,Vec<u32>) {
    use prefilter::Kernel as PrefilterKernel;
    use smith_waterman::Kernel as ScoreKernel;
    if query.is_empty() {return (true,0,label.len(),0,false,vec![]);}
    let sensitive = query.chars().any(char::is_uppercase);
    let unicode = !query.is_ascii();
    let typos = u16::try_from(query.chars().count()/4).unwrap_or(6).min(6);
    if label.len() < query.chars().count().saturating_sub(typos as usize) {return (false,0,label.len(),0,false,vec![]);}
    let mut filter = <P as PrefilterKernel>::new(query,sensitive);
    let bytes = label.as_bytes();
    let window = match (typos,unicode) {
        (0,false)=><P as PrefilterKernel>::match_haystack(&filter,bytes),
        (0,true)=><P as PrefilterKernel>::match_haystack_unicode(&filter,bytes),
        (1,false)=><P as PrefilterKernel>::match_haystack_1_typo(&filter,bytes),
        (1,true)=><P as PrefilterKernel>::match_haystack_unicode_1_typo(&filter,bytes),
        (2,false)=><P as PrefilterKernel>::match_haystack_2_typos(&filter,bytes),
        (2,true)=><P as PrefilterKernel>::match_haystack_unicode_2_typos(&filter,bytes),
        (_,false)=><P as PrefilterKernel>::match_haystack_many_typos(&mut filter,bytes,typos),
        (_,true)=><P as PrefilterKernel>::match_haystack_unicode_many_typos(&mut filter,bytes,typos),
    };
    if !window.0 {return (false,window.1,window.2,0,false,vec![]);}
    let start = window.1.saturating_sub(1);
    let trimmed = &bytes[start..window.2];
    let mut scorer = <S as ScoreKernel>::new(query,&Scoring::default(),sensitive);
    let (mut score,indices) = if unicode {<S as ScoreKernel>::score_haystack_unicode_indices(&mut scorer,trimmed,start,Some(typos))} else {<S as ScoreKernel>::score_haystack_indices(&mut scorer,trimmed,start,Some(typos))};
    let exact = start==0 && window.2==bytes.len() && query.as_bytes()==trimmed;
    if exact {score=score.saturating_add(Scoring::default().exact_match_bonus);}
    (true,window.1,window.2,score,exact,indices)
}

pub fn zz_path_debug(query: &str, label: &str) -> (bool,usize,usize,u16,bool,Vec<u32>) {
    let small = smith_waterman::score_fits_in_u8(query.len(),&Scoring::default());
    macro_rules! selected {
        ($p:ty,$s:ty) => {
            if <$p as prefilter::Kernel>::is_available() && <$s as smith_waterman::Kernel>::is_available() {
                return zz_path_debug_impl::<$p,$s>(query,label);
            }
        };
    }
    #[cfg(target_arch="x86_64")]
    {
        if small {
            selected!(prefilter::backend::PrefilterAVX512,smith_waterman::SmithWatermanAVX512U8);
            selected!(prefilter::backend::PrefilterAVX,smith_waterman::SmithWatermanAVXU8);
            selected!(prefilter::backend::PrefilterSSE,smith_waterman::SmithWatermanSSEU8);
        } else {
            selected!(prefilter::backend::PrefilterAVX512,smith_waterman::SmithWatermanAVX512);
            selected!(prefilter::backend::PrefilterAVX,smith_waterman::SmithWatermanAVX);
            selected!(prefilter::backend::PrefilterSSE,smith_waterman::SmithWatermanSSE);
        }
    }
    #[cfg(target_arch="aarch64")]
    {
        if small {return zz_path_debug_impl::<prefilter::backend::PrefilterNEON,smith_waterman::SmithWatermanNEONU8>(query,label);}
        return zz_path_debug_impl::<prefilter::backend::PrefilterNEON,smith_waterman::SmithWatermanNEON>(query,label);
    }
    #[cfg(not(target_arch="aarch64"))]
    {
        if small {zz_path_debug_impl::<prefilter::backend::PrefilterScalar,smith_waterman::SmithWatermanScalarU8>(query,label)} else {zz_path_debug_impl::<prefilter::backend::PrefilterScalar,smith_waterman::SmithWatermanScalar>(query,label)}
    }
}

pub fn zz_path_debug_at(width: u32, query: &str, label: &str) -> (bool,usize,usize,u16,bool,Vec<u32>) {
    let small = smith_waterman::score_fits_in_u8(query.len(),&Scoring::default());
    macro_rules! backend {
        ($p:ty,$small:ty,$wide:ty) => {
            if small {return zz_path_debug_impl::<$p,$small>(query,label);}
            return zz_path_debug_impl::<$p,$wide>(query,label);
        };
    }
    #[cfg(target_arch="x86_64")]
    {
        if width==64 && zz_path_vector_bytes()>=64 {backend!(prefilter::backend::PrefilterAVX512,smith_waterman::SmithWatermanAVX512U8,smith_waterman::SmithWatermanAVX512);}
        if width==32 && zz_path_vector_bytes()>=32 {backend!(prefilter::backend::PrefilterAVX,smith_waterman::SmithWatermanAVXU8,smith_waterman::SmithWatermanAVX);}
        if width==16 {backend!(prefilter::backend::PrefilterSSE,smith_waterman::SmithWatermanSSEU8,smith_waterman::SmithWatermanSSE);}
    }
    zz_path_debug(query,label)
}
'''


def write_changed(path, content):
    if not path.exists() or path.read_text() != content:
        path.write_text(content)


def capture(vectors, forced=None):
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    pinned = next(package for package in lock["package"] if package["name"] == "neo_frizbee")
    sources = list((Path.home() / ".cargo/registry/src").glob("*/neo_frizbee-" + pinned["version"]))
    if len(sources) != 1:
        raise AssertionError("native path reference source is ambiguous or missing")
    source = sources[0]
    archives = list((Path.home() / ".cargo/registry/cache").glob("*/neo_frizbee-" + pinned["version"] + ".crate"))
    if len(archives) != 1 or hashlib.sha256(archives[0].read_bytes()).hexdigest() != pinned["checksum"]:
        raise AssertionError("native path reference differs from the lockfile")
    with tarfile.open(archives[0], "r:gz") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            relative = Path(*Path(member.name).parts[1:])
            data = archive.extractfile(member).read()
            if (source / relative).read_bytes() != data:
                raise AssertionError("modified native path reference source: " + str(relative))
    copied = TARGET / "source"
    if not copied.exists():
        shutil.copytree(source, copied)
    write_changed(copied / "src/lib.rs", (source / "src/lib.rs").read_text() + EXPOSE)
    project = TARGET / "probe"
    (project / "src").mkdir(parents=True, exist_ok=True)
    write_changed(project / "Cargo.toml", '[package]\nname="zz-path-reference"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nneo_frizbee={path="../source"}\n[workspace]\n')
    write_changed(project / "src/main.rs", (ROOT / "mojo/scripts/path_rank_native.rs").read_text())
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(TARGET / "build")
    subprocess.run(["cargo", "build", "--quiet", "--offline", "--manifest-path", str(project / "Cargo.toml")], env=environment, check=True, timeout=300)
    pairs = {}
    for case in vectors["cases"]:
        scores = {item[0]:item[1:] for item in case["scores"]}
        indices = {item[0]:item[1:] for item in case["indices"]}
        for index, label in enumerate(case["labels"]):
            pairs[(case["query"],label)] = None if index not in scores else [*scores[index],indices[index][2]]
    request = bytearray(struct.pack("<I",len(pairs)))
    for query,label in pairs:
        for value in (query,label):
            value = value.encode()
            request.extend(struct.pack("<I",len(value)))
            request.extend(value)
    (TARGET / "input.bin").write_bytes(request)
    subprocess.run([str(TARGET / "build/debug/zz-path-reference"),str(TARGET / "input.bin"),str(TARGET / "output.bin"),str(forced or 0)], check=True, timeout=180)
    data = (TARGET / "output.bin").read_bytes()
    host_vector_bytes = struct.unpack_from("<I",data,4)[0]
    vector_bytes = forced or host_vector_bytes
    offset = 8
    cases = []
    mismatches = []
    for (query,label), expected in pairs.items():
        accepted,start,end,score,exact,count = struct.unpack_from("<BIIHBI",data,offset)
        offset += 16
        indices = list(struct.unpack_from("<"+"I"*count,data,offset))
        offset += count*4
        actual = [score,bool(exact),indices] if accepted else None
        if actual != expected:
            mismatches.append({"query":query,"label":label,"observed":actual,"public":expected})
        cases.append({"query":query,"label":label,"accepted":bool(accepted),"start":start,"end":end,"score":score,"exact":bool(exact),"indices":indices})
    if offset != len(data):
        raise AssertionError("native private reference returned trailing bytes")
    report = {"cases":cases,"vector_bytes":vector_bytes,"host_vector_bytes":host_vector_bytes,"public_mismatches":mismatches,"source_checksum":pinned["checksum"],"reference":"unchanged pinned source with an added native-backend observation function; registry source remains untouched"}
    (ROOT / ("target/port/path-rank-private-vectors"+("-"+str(forced) if forced else "")+".json")).write_text(json.dumps(report)+"\n")
    return report
