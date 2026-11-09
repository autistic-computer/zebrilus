from std.sys import argv
from std.testing import assert_equal, assert_true
from editor_fixture import Sink, Source
from zzmojo.editor_state import (
    EDITOR_PROMPT_NEW_PATH,
    EDITOR_SET_BUFFER,
    EDITOR_WRITE,
    EditorEffect,
    EditorModel,
    descriptor_path_error,
    descriptor_sync_action,
    editor_language,
    editor_title,
)
from zzmojo.editor_files import read_editor_file, save_editor_file


def located(root: String, name: String) -> String:
    if name.startswith("/"):
        return name
    return root + "/" + name


def fnv(bytes: Span[UInt8, _]) -> UInt64:
    var hash: UInt64 = 0xCBF29CE484222325
    for byte in bytes:
        hash = (hash ^ UInt64(byte)) * 0x100000001B3
    return hash


def dump(result: List[UInt8], effects: List[EditorEffect], model: EditorModel) -> List[UInt8]:
    var sink = Sink()
    sink.bytes.extend(Span(result))
    sink.uint(UInt64(len(effects)), 4)
    for effect in effects:
        sink.bytes.append(effect.kind)
        sink.text(effect.text)
        if (
            effect.kind == EDITOR_SET_BUFFER
            or effect.kind == EDITOR_WRITE
            or effect.kind == EDITOR_PROMPT_NEW_PATH
        ):
            sink.text(effect.detail)
    sink.text(model.cwd)
    sink.optional(model.path)
    sink.optional(model.snapshot_path)
    sink.optional(model.pending_path)
    sink.text(model.saved_contents)
    sink.flag(model.dirty)
    sink.text(model.title)
    sink.flag(Bool(model.error))
    if model.error:
        sink.text(model.error.value().message)
        sink.bytes.append(model.error.value().retry)
        sink.text(model.error.value().path)
        sink.text(model.error.value().title())
    sink.flag(model.picker_open)
    return sink.bytes.copy()


def step(mut source: Source, mut model: EditorModel, mut result: List[UInt8]) raises -> List[EditorEffect]:
    var kind = source.uint(1)
    if kind == 0:
        var snapshot = source.optional()
        return model.synchronize(snapshot, source.text())
    if kind == 1:
        return model.buffer_changed(source.text())
    if kind == 2:
        result.append(UInt8(1) if model.open_picker() else UInt8(0))
        return List[EditorEffect]()
    if kind == 3:
        return model.picker_closed(source.optional())
    if kind == 4:
        var loaded_path = source.text()
        var loaded = source.flag()
        var payload = source.text()
        if loaded:
            return model.finish_load(loaded_path, payload)
        model.load_failed(loaded_path, payload)
        return List[EditorEffect]()
    if kind == 5:
        return model.save(source.text())
    if kind == 6:
        var target = source.text()
        return model.begin_save(target, source.text())
    if kind == 7:
        var saved_path = source.text()
        var saved = source.text()
        var written = source.flag()
        var message = source.text()
        var contents = source.text()
        if written:
            return model.finish_save(saved_path, saved, contents)
        model.save_failed(saved_path, message)
        return List[EditorEffect]()
    if kind == 8:
        var retry = UInt8(source.uint(1))
        var retry_path = source.text()
        return model.retry(retry, retry_path, source.text())
    if kind == 9:
        var empty = source.flag()
        result.append(UInt8(1) if model.scratch_hint(empty) else UInt8(0))
        return List[EditorEffect]()
    raise Error("unknown editor model operation")


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) < 8 * 1024 * 1024)
    var read_root = String(args[2])
    var save_root = String(args[3])
    var source = Source(bytes^)

    var syncs = Int(source.uint(4))
    for _ in range(syncs):
        var pending = source.optional()
        var snapshot = source.optional()
        var local = source.optional()
        assert_equal(
            descriptor_sync_action(pending, snapshot, local), UInt8(source.uint(1))
        )

    var names = Int(source.uint(4))
    for _ in range(names):
        var path = source.text()
        assert_equal(editor_language(path), source.text())
        assert_equal(editor_title(path, False), source.text())
        assert_equal(editor_title(path, True), source.text())
    assert_equal(editor_title(None, False), source.text())
    assert_equal(editor_title(None, True), source.text())

    var validations = Int(source.uint(4))
    for _ in range(validations):
        var path = source.text()
        var cwd = source.text()
        var accepted = source.flag()
        var expected = source.text()
        var refused = descriptor_path_error(path, cwd)
        assert_equal(not refused, accepted)
        assert_equal(refused.value() if refused else path, expected)

    var reads = Int(source.uint(4))
    for _ in range(reads):
        var name = source.text()
        var loaded = read_editor_file(located(read_root, name))
        assert_equal(loaded.ok, source.flag(), name)
        if loaded.ok:
            assert_equal(UInt64(loaded.contents.byte_length()), source.uint(8), name)
            assert_equal(fnv(loaded.contents.as_bytes()), source.uint(8), name)
        else:
            assert_equal(loaded.message, source.text())

    var saves = Int(source.uint(4))
    for _ in range(saves):
        var name = source.text()
        var contents = source.text()
        var failure = save_editor_file(located(save_root, name), contents)
        var message = String()
        if failure:
            message = failure.value().replace(save_root, "<root>")
        assert_equal(not failure, source.flag(), name)
        assert_equal(message, source.text())

    var scenarios = Int(source.uint(4))
    var steps = 0
    for scenario in range(scenarios):
        var path = source.optional()
        var cwd = source.text()
        var effects = List[EditorEffect]()
        var model = EditorModel.open(path, cwd, effects)
        assert_true(source.matches(dump(List[UInt8](), effects, model)), String("scenario ", scenario, " start"))
        var count = Int(source.uint(4))
        for index in range(count):
            var result = List[UInt8]()
            var produced = step(source, model, result)
            assert_true(
                source.matches(dump(result, produced, model)),
                String("scenario ", scenario, " step ", index),
            )
        steps += count
    assert_equal(source.offset, len(source.bytes))
    print(syncs, names, validations, reads, saves, scenarios, steps)
