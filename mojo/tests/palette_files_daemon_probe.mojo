from std.sys import argv
from std.ffi import external_call
from std.testing import assert_equal, assert_true, assert_false
from zzmojo.backend import AsyncBackend
from zzmojo.state import AppModel, _selected_text
from zzmojo.palette_files_runtime import PaletteFilesRuntime
from zzmojo.palette_files import FILES_ENTER, FILES_INSERT
from zzmojo.path_context import FilesPlace
from zzmojo.path_protocol import PATH_OPEN


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def pump(
    mut backend: AsyncBackend,
    mut model: AppModel,
    mut files: PaletteFilesRuntime,
) raises -> Bool:
    backend.submit(model, 0)
    _ = backend.collect(model, 0, True)
    var inbox = backend.paths.take()
    inbox.events.reverse()
    var opened = False
    while len(inbox.events) > 0:
        var delivery = inbox.events.pop()
        if delivery.message.kind == PATH_OPEN:
            opened = True
        else:
            _ = files.apply(backend, delivery^)
    files.observe_source(backend)
    _ = files.tick()
    pause()
    return opened


def main() raises:
    var args = argv()
    var backend = AsyncBackend(args[1])
    var model = AppModel()
    var files = PaletteFilesRuntime()
    for _ in range(200):
        _ = pump(backend, model, files)
        if UInt64(0) in model.terminals and "FILES_READY" in _selected_text(
            model.terminals, UInt64(0)
        ):
            break
    assert_equal(model.error, "")
    assert_equal(model.pane, Optional[UInt64](1))
    assert_true(Bool(backend.path_source))
    var source = backend.path_source.value()
    var start = files.open(
        backend,
        model,
        source,
        FilesPlace(0, None, None, None),
        "here",
        True,
        UInt64(0),
        None,
        String(args[2]),
        0,
    )
    files.rest(backend, model, start)
    for _ in range(200):
        _ = pump(backend, model, files)
        if (
            files.files.service.listing
            and files.files.service.listing.value().done
            and not files.busy()
            and not files.files.ranking.dirty
        ):
            break
    assert_true(Bool(files.files.service.listing.value().root))
    assert_false(Bool(files.files.service.listing.value().error))
    assert_true(len(files.files.ranking.rows) >= 2)
    files.rest(backend, model, "alp")
    for _ in range(200):
        _ = pump(backend, model, files)
        if (
            not files.busy()
            and not files.files.ranking.dirty
            and len(files.files.ranking.rows) == 1
        ):
            break
    assert_equal(len(files.files.ranking.rows), 1)
    assert_equal(files.files.ranking.rows[0].entry.rel, "alpha")
    var effect = files.files.key(FILES_ENTER, files.inserts(model))
    assert_equal(effect.kind, FILES_INSERT)
    assert_equal(effect.pane, Optional[UInt64](0))
    files.effect(backend, model, effect)
    assert_true(files.close_ready)
    for _ in range(20):
        _ = pump(backend, model, files)
    assert_equal(model.pane, Optional[UInt64](1))
    assert_equal(model.source_client, source.client)
    assert_equal(model.error, "")
    var counter = files.files.service.next_request
    files.close(backend)
    assert_equal(files.files.service.next_request, counter)
    assert_false(files.files.active)
    print(
        "native files: real listings, background rank and captured-pane paste"
        " without focus changes passed"
    )
