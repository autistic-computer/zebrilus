from std.sys import argv
from std.testing import assert_equal, assert_false, assert_true
from editor_fixture import Sink, Source
from zzmojo.file_picker import (
    PICKER_ACCEPT_ROW,
    PICKER_ACCEPT_TYPED,
    FilePicker,
    PickerEntry,
    PickerFlow,
    PickerLabels,
    PickerRanker,
    directory_query,
    entry_prior,
    preserved_selection,
    validate_selection,
)

comptime SLICE_NS: UInt64 = 60000000000


def located(root: String, name: String) -> String:
    if name.startswith("/"):
        return name
    return root + "/" + name


def state(mut sink: Sink, flow: PickerFlow):
    sink.text(flow.query)
    sink.index(flow.selected)
    sink.flag(flow.scanning)
    sink.flag(flow.limited)
    sink.optional(flow.error)
    sink.uint(flow.rank_generation, 8)
    sink.uint(flow.accept_generation, 8)


def step(mut source: Source, mut flow: PickerFlow) raises -> List[UInt8]:
    var sink = Sink()
    var kind = Int(source.uint(1))
    if kind == 0:
        var ok = source.flag()
        var limited = source.flag()
        var error = source.text()
        if ok:
            flow.scan_finished(limited)
        else:
            flow.scan_failed(error)
    elif kind == 1:
        var query = source.text()
        var indexed = source.flag()
        sink.flag(flow.query_changed(query, indexed))
    elif kind == 2:
        sink.uint(flow.begin_rank(source.flag()), 8)
    elif kind == 3:
        sink.uint(flow.begin_accept(), 8)
    elif kind == 4:
        var target = flow.accept_target()
        sink.bytes.append(target.kind)
        if target.kind == PICKER_ACCEPT_TYPED:
            sink.text(target.path)
        elif target.kind == PICKER_ACCEPT_ROW:
            sink.uint(UInt64(target.row), 4)
    elif kind == 5:
        var count = Int(source.uint(4))
        var direction = Int(source.uint(1))
        if direction >= 128:
            direction -= 256
        sink.index(flow.navigate(count, direction))
    elif kind == 6:
        sink.flag(flow.hover(Int(source.uint(4))))
    elif kind == 7:
        sink.flag(flow.shows_first_batch(source.flag()))
    elif kind == 8:
        sink.flag(flow.scanning_note(source.flag()))
    elif kind == 9:
        var input_empty = source.flag()
        var directories = source.flag()
        sink.text(flow.empty_message(input_empty, directories))
    elif kind == 10:
        flow.selected = source.index()
    elif kind == 11:
        flow.error = Optional[String](source.text())
    elif kind == 12:
        flow.rank_generation = source.uint(8)
        flow.accept_generation = source.uint(8)
    else:
        raise Error("unknown picker flow operation")
    state(sink, flow)
    return sink.bytes.copy()


def labels(picker: FilePicker) -> String:
    var text = String()
    for row in picker.rows:
        text += row.relative + "=" + String(row.prior) + "\n"
    return text


def settled(root: String, directories: Bool) raises -> FilePicker:
    var picker = FilePicker(root, directories)
    assert_true(picker.advance(SLICE_NS))
    assert_false(picker.advance(SLICE_NS))
    return picker^


def scan_checks(root: String) raises:
    var folders = settled(root, True)
    assert_false(folders.flow.scanning)
    assert_false(folders.flow.limited)
    assert_false(Bool(folders.flow.error))
    assert_equal(folders.scan.visited, 18)
    var listing = labels(folders)
    assert_equal(
        listing,
        "workspace=1087\nMusic=63\ndeep=63\nplain=63\ndeep/1=62\nworkspace/src=62\n"
        + "deep/1/2=61\ndeep/1/2/3=60\ndeep/1/2/3/4=59\ndeep/1/2/3/4/5=58\n"
        + "deep/1/2/3/4/5/6=57\ndeep/1/2/3/4/5/6/7=56\n",
    )
    assert_equal(folders.flow.selected.value(), 0)
    assert_equal(folders.selected_path().value(), root + "/workspace")
    assert_equal(folders.navigate(-1).value(), 11)
    assert_equal(folders.navigate(1).value(), 0)
    assert_equal(folders.navigate(1).value(), 1)
    folders.set_query("deep/1/2/3/4/")
    assert_equal(folders.rows[0].relative, "deep/1/2/3/4")
    assert_equal(folders.flow.selected.value(), 0)
    folders.set_query("src")
    assert_equal(labels(folders), "workspace/src=62\n")
    assert_equal(folders.accept_selected().value(), root + "/workspace/src")
    assert_false(folders.hover(0))
    folders.set_query("zzzz")
    assert_equal(len(folders.rows), 0)
    assert_false(Bool(folders.flow.selected))
    assert_equal(folders.empty_message(), "Nothing matches that search.")
    assert_false(Bool(folders.accept_selected()))
    folders.set_query("  " + root + "/file.txt ")
    assert_false(Bool(folders.accept_selected()))
    assert_equal(folders.flow.error.value(), root + "/file.txt is not a folder.")
    folders.set_query(root + "/gone")
    assert_false(Bool(folders.flow.error))
    assert_false(Bool(folders.accept_selected()))
    assert_equal(
        folders.flow.error.value(),
        "Cannot open " + root + "/gone: No such file or directory (os error 2)",
    )
    folders.set_query(root + "/plain")
    assert_equal(folders.accept_selected().value(), root + "/plain")

    var files = settled(root, False)
    assert_false(files.flow.limited)
    assert_equal(
        labels(files),
        "deep/1/2/3/4/5/6/near.txt=0\nfile.txt=0\nplain/note.md=0\n",
    )
    files.set_query("not")
    assert_equal(labels(files), "plain/note.md=0\n")
    assert_equal(files.accept(0).value(), root + "/plain/note.md")
    files.set_query(root + "/plain")
    assert_false(Bool(files.accept_selected()))
    assert_equal(files.flow.error.value(), root + "/plain is not a file.")

    var empty = settled(root + "/workspace/src", True)
    assert_equal(len(empty.rows), 0)
    assert_equal(empty.empty_message(), "No folders under this folder.")
    assert_false(empty.indexed)

    var missing = FilePicker(root + "/gone", True)
    assert_false(missing.flow.scanning)
    assert_false(missing.advance(SLICE_NS))
    assert_equal(
        missing.empty_message(),
        "Cannot read " + root + "/gone: No such file or directory (os error 2)",
    )
    missing.set_query("x")
    assert_equal(
        missing.empty_message(),
        "Cannot read " + root + "/gone: No such file or directory (os error 2)",
    )

    var sliced = FilePicker(root, True)
    assert_true(sliced.flow.scanning)
    assert_equal(sliced.empty_message(), "Scanning…")
    var turns = 0
    while sliced.advance(SLICE_NS, 1):
        turns += 1
        assert_true(turns < 1000)
    assert_true(turns > 18)
    assert_equal(sliced.scan.visited, 18)
    assert_equal(labels(sliced), listing)


def main() raises:
    var args = argv()
    if len(args) != 4:
        raise Error("usage: file_picker_probe VECTORS READ_ROOT SCAN_ROOT")
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var source = Source(bytes^)
    var read_root = String(args[2])

    var priors = Int(source.uint(4))
    for _ in range(priors):
        var depth = source.uint(8)
        var workspace = source.flag()
        assert_equal(UInt64(entry_prior(depth, workspace)), source.uint(4))

    var selections = Int(source.uint(4))
    for _ in range(selections):
        var entries = List[String]()
        for _ in range(Int(source.uint(4))):
            entries.append(source.text())
        var preferred = source.optional()
        var expected = source.index()
        var actual = preserved_selection(entries, preferred)
        assert_equal(Bool(actual), Bool(expected))
        if expected:
            assert_equal(actual.value(), expected.value())

    var queries = Int(source.uint(4))
    for _ in range(queries):
        var query = source.text()
        assert_equal(directory_query(query), source.text())

    var ranker = PickerRanker()
    var sets = Int(source.uint(4))
    var ranked_queries = 0
    var ranked_rows = 0
    for _ in range(sets):
        var index = PickerLabels()
        for _ in range(Int(source.uint(4))):
            var label = source.text()
            index.entries.append(PickerEntry(label, label, UInt32(source.uint(4))))
        for _ in range(Int(source.uint(4))):
            var query = source.text()
            var rows = ranker.rank(query, index)
            assert_equal(len(rows), Int(source.uint(4)), query)
            for row in rows:
                assert_equal(row, Int(source.uint(4)), query)
            ranked_queries += 1
            ranked_rows += len(rows)

    var validations = Int(source.uint(4))
    for _ in range(validations):
        var name = source.text()
        var directories = source.flag()
        var ok = source.flag()
        var message = source.text()
        var refused = validate_selection(located(read_root, name), directories)
        assert_equal(not refused, ok, name)
        if refused:
            assert_equal(refused.value(), message)

    var sequences = Int(source.uint(4))
    var operations = 0
    for _ in range(sequences):
        var flow = PickerFlow()
        for _ in range(Int(source.uint(4))):
            var body = step(source, flow)
            assert_true(source.matches(body), "picker flow diverged")
            operations += 1
    assert_equal(source.offset, len(source.bytes))

    scan_checks(String(args[3]))
    print(
        priors,
        selections,
        queries,
        sets,
        ranked_queries,
        ranked_rows,
        validations,
        sequences,
        operations,
    )
