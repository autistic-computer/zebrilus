from std.testing import assert_equal, assert_true, assert_false
from zzmojo.palette_files import (
    PaletteFiles,
    FILES_TAB,
    FILES_ENTER,
    FILES_COPY,
    FILES_LEFT,
    FILES_SET_REST,
    FILES_INSERT,
    FILES_COPIED,
    FILES_UNHANDLED,
)
from zzmojo.path_context import FilesPlace
from zzmojo.path_service import PathSource
from zzmojo.path_protocol import (
    PathRoot,
    PathEntry,
    PathMessage,
    PATH_BEGIN,
    PATH_CHUNK,
)
from zzmojo.palette_path_worker import PalettePathWorker


def main() raises:
    var files = PaletteFiles()
    _ = files.open(
        PathSource(1, 2, 3),
        FilesPlace(0, None, None, None),
        "here",
        True,
        "/root/",
        UInt64(9),
        7,
    )
    var commands = files.set_rest("", UInt64(9))
    assert_equal(len(commands), 1)
    var request = commands[0].request
    var begin = PathMessage(PATH_BEGIN, request)
    begin.root = PathRoot("/root", "~/", "/root", 0)
    _ = files.apply(PathSource(1, 2, 3), begin^)
    var chunk = PathMessage(PATH_CHUNK, request)
    chunk.entries.append(PathEntry("dir/", 1, False))
    chunk.entries.append(PathEntry("λ file", 0, False))
    chunk.done = True
    _ = files.apply(PathSource(1, 2, 3), chunk^)
    var worker = PalettePathWorker()
    var job = files.prepare()
    assert_true(Bool(job))
    var owned = job.take()
    assert_true(worker.submit(owned^))
    var result = worker.collect(True)
    assert_true(Bool(result))
    var landed = files.complete(result.take())
    assert_true(landed.landed)
    assert_equal(len(files.ranking.rows), 2)
    assert_equal(files.ranking.rows[0].name, "dir")
    var effect = files.key(FILES_TAB, True)
    assert_equal(effect.kind, FILES_SET_REST)
    assert_equal(effect.text, "dir/")
    effect = files.key(FILES_ENTER, True)
    assert_equal(effect.kind, FILES_INSERT)
    assert_equal(effect.pane, Optional[UInt64](9))
    assert_equal(effect.entry.value().rel, "dir/")
    effect = files.key(FILES_COPY, True)
    assert_equal(effect.kind, FILES_COPIED)
    assert_equal(effect.text, "/root/dir")
    assert_equal(files.picked_folder(), Optional[String]("~/dir"))
    assert_equal(files.key(FILES_LEFT, True).text, "/")
    var next = files.set_rest("λ", UInt64(9))
    assert_equal(len(next), 0)
    assert_equal(files.key(FILES_LEFT, True).kind, FILES_UNHANDLED)
    next = files.set_rest("../other/", UInt64(9))
    assert_equal(len(next), 1)
    var pending = files.service.pending
    var retired = files.service.retire_pending()
    assert_true(Bool(retired))
    assert_equal(retired.value().request, pending.value())
    assert_true(Bool(files.service.listing))
    assert_equal(files.service.listing.value().dir, "/other/")
    assert_equal(len(files.ranking.rows), 2)
    assert_false(Bool(files.service.pending))
    var before = files.service.next_request
    _ = files.close()
    assert_equal(files.service.next_request, before)
    assert_false(files.active)
    print(
        "file level typed navigation, root metadata, insertion/copy and real"
        " background rank worker passed"
    )
