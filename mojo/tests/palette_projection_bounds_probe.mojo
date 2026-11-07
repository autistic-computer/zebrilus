from std.testing import assert_equal, assert_true
from zzmojo.tree_model import MuxSnapshot, SessionSnapshot, WindowSnapshot
from zzmojo.palette_programs import PalettePrograms
from zzmojo.palette_snapshot import project_palette_snapshot
from zzmojo.palette_places_model import MAX_FLEET_NODES, MAX_FLEET_BYTES


def repeated(count: Int) raises -> String:
    var bytes = List[UInt8](length=count, fill=UInt8(120))
    return String(from_utf8=Span(bytes))


def main() raises:
    var programs = PalettePrograms()
    var candidates = List[String](length=4096, fill=String())
    assert_equal(programs.resolve(candidates), Optional[Int](None))
    candidates.append(String())
    var refused = False
    try:
        _ = programs.resolve(candidates)
    except:
        refused = True
    assert_true(refused)
    candidates.clear()
    candidates.append(repeated(1024 * 1024 + 1))
    refused = False
    try:
        _ = programs.resolve(candidates)
    except:
        refused = True
    assert_true(refused)
    var source = MuxSnapshot()
    source.sessions.resize(MAX_FLEET_NODES - 1, SessionSnapshot())
    for index in range(len(source.sessions)):
        source.sessions[index].id = UInt64(index)
    var projected = project_palette_snapshot(source, None, "h", programs)
    assert_equal(len(projected.fleet.hosts[0].sessions), MAX_FLEET_NODES - 1)
    source.sessions.append(SessionSnapshot())
    refused = False
    try:
        _ = project_palette_snapshot(source, None, "h", programs)
    except:
        refused = True
    assert_true(refused)
    source.sessions.clear()
    var session = SessionSnapshot()
    session.name = "x"
    var window = WindowSnapshot()
    window.name = repeated(MAX_FLEET_BYTES - 25)
    session.windows.append(window^)
    source.sessions.append(session^)
    projected = project_palette_snapshot(source, None, "h", programs)
    assert_equal(
        projected.fleet.hosts[0].sessions[0].windows[0].name.byte_length(),
        MAX_FLEET_BYTES - 25,
    )
    source.sessions[0].windows[0].name += "x"
    refused = False
    try:
        _ = project_palette_snapshot(source, None, "h", programs)
    except:
        refused = True
    assert_true(refused)
    print("palette projection exact node/text/process bounds passed")
