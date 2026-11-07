from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool
from zzmojo.path_protocol import (
    PathMessage,
    PathRoot,
    PathEntry,
    PATH_BEGIN,
    PATH_CHUNK,
    decode_path_message,
)
from zzmojo.path_service import PathService, PathSource, PathCommand
from zzmojo.path_rank import PathRanker
from zzmojo.path_rank_job import PathRanking, PathRankJob, run_path_rank
from zzmojo.path_text import FilesPath, split_rest, parent_dir, path_last_name


def optional_text(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


@fieldwise_init
struct Call(Copyable, Movable):
    var kind: UInt8
    var request: UInt64
    var dir: String


struct Lifecycle(Movable):
    var service: PathService
    var state: PathRanking
    var worker: PathRanker
    var job: Optional[PathRankJob]
    var path: FilesPath
    var calls: List[Call]

    def __init__(out self) raises:
        self.service = PathService()
        self.state = PathRanking(PathSource(1, 2, 3))
        self.worker = PathRanker()
        self.job = None
        self.path = split_rest("", "/root/")
        self.calls = List[Call]()

    def command(mut self, command: PathCommand) raises:
        var decoded = decode_path_message(command.frame.copy())
        if not decoded:
            raise Error("path lifecycle command has no wire payload")
        self.calls.append(
            Call(
                UInt8(0 if decoded.value().kind == 0 else 1),
                command.request,
                decoded.value()
                .dir.value()
                .copy() if decoded.value()
                .dir else String(),
            )
        )

    def prepare(mut self) raises:
        if self.service.listing and not self.job:
            self.job = self.state.prepare(
                self.service.listing.value(), self.path.dir, self.path.filter
            )

    def rest(mut self, value: String) raises:
        var path = split_rest(value, "/root/")
        if (
            path.dir == self.path.dir
            and path.typed == self.path.typed
            and path.filter == self.path.filter
            and self.service.listing
        ):
            return
        var moved = path.dir != self.path.dir
        self.path = path^
        var replaced = (
            not self.service.listing
            or not self.service.listing.value().covers("local", self.path.dir)
        )
        if replaced:
            var commands = self.service.set_directory(
                PathSource(1, 2, 3), "local", self.path.dir
            )
            for command in commands:
                self.command(command)
            self.state.replace_listing(not moved)
        else:
            self.state.refresh(not moved)
        self.prepare()

    def apply(mut self, var message: PathMessage) raises:
        if self.service.listing:
            var changed = self.service.listing.value().apply(message^)
            if self.service.listing.value().done:
                self.service.pending = None
            if changed:
                self.state.refresh(True)
                self.prepare()

    def flush(mut self) raises:
        while self.job:
            var result = run_path_rank(self.worker, self.job.take())
            _ = self.state.complete(result^, self.service.listing)
            self.prepare()

    def up(mut self) raises:
        var root: Optional[String] = None
        if (
            self.service.listing
            and self.service.listing.value().root
            and self.path.dir.startswith(self.service.listing.value().dir)
        ):
            var prefix = String(
                self.service.listing.value().root.value().root.rstrip("/")
            )
            var under = String(
                from_utf8_lossy=self.path.dir.as_bytes()[
                    self.service.listing.value().dir.byte_length() :
                ]
            )
            root = String(prefix, "/", under)
        var parent = parent_dir(self.path.dir, root)
        if parent:
            self.state.pending = path_last_name(
                root.value() if root else self.path.dir
            )
            self.rest(parent.value())

    def close(mut self) raises:
        var command = self.service.close()
        if command:
            self.command(command.value())
        self.state.reset(PathSource(1, 2, 3))
        self.job = None
        self.path = split_rest("", "/root/")

    def write(self, mut output: List[UInt8]):
        append_uint(output, self.state.generation)
        output.append(UInt8(Bool(self.state.active)))
        output.append(UInt8(self.state.dirty))
        append_uint(output, UInt64(self.state.selected))
        optional_text(output, self.state.pending)
        optional_text(output, self.state.wanted)
        append_text(output, self.path.dir)
        append_text(output, self.path.typed)
        append_text(output, self.path.filter)
        output.append(UInt8(Bool(self.service.listing)))
        if self.service.listing:
            ref listing = self.service.listing.value()
            append_text(output, listing.dir)
            output.append(UInt8(Bool(listing.request)))
            if listing.request:
                append_uint(output, listing.request.value())
            output.append(UInt8(listing.done))
            optional_text(output, listing.error)
        append_uint(output, UInt64(len(self.state.rows)))
        for row in self.state.rows:
            append_text(output, row.entry.rel)
            append_text(output, row.name)
        append_uint(output, UInt64(len(self.calls)))
        for call in self.calls:
            append_uint(output, UInt64(call.kind))
            append_uint(output, call.request)
            if call.kind == 0:
                append_text(output, call.dir)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as file:
        bytes = file.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var output = List[UInt8]()
    for _ in range(Int(reader.uint())):
        var lifecycle = Lifecycle()
        lifecycle.write(output)
        for _ in range(Int(reader.uint())):
            var kind = reader.uint()
            if kind == 0:
                lifecycle.rest(reader.text())
            elif kind == 1:
                var message = PathMessage(PATH_BEGIN)
                message.root = PathRoot(reader.text(), "~/", None, 0)
                lifecycle.apply(message^)
            elif kind == 2:
                var message = PathMessage(PATH_CHUNK)
                for _ in range(Int(reader.uint())):
                    var rel = reader.text()
                    message.entries.append(
                        PathEntry(
                            rel.copy(),
                            UInt8(1 if rel.endswith("/") else 0),
                            False,
                        )
                    )
                message.done = read_bool(reader)
                lifecycle.apply(message^)
            elif kind == 3:
                var message = PathMessage(PATH_BEGIN)
                message.error = String("missing")
                lifecycle.apply(message^)
            elif kind == 4:
                lifecycle.state.wanted = reader.text()
            elif kind == 5:
                _ = lifecycle.state.navigate(-1 if read_bool(reader) else 1)
            elif kind == 6:
                lifecycle.up()
            elif kind == 7:
                lifecycle.flush()
            elif kind == 8:
                lifecycle.close()
            else:
                raise Error("unknown path lifecycle operation")
            lifecycle.write(output)
    reader.finish()
    with open(args[2], "w") as file:
        file.write_bytes(Span(output))
