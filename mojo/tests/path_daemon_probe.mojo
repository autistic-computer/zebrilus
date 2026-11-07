from std.sys import argv
from zzmojo.transport import Connection
from zzmojo.stream_transport import connect_interactive
from zzmojo.stream_protocol import BATCH, decode_stream_frame
from zzmojo.protocol import control_tag
from zzmojo.path_protocol import decode_path_message
from zzmojo.path_service import PathService, PathSource, PathCommand


def send(connection: Connection, commands: Span[PathCommand, _], source: PathSource) raises:
    for command in commands:
        if not command.source.matches(source):
            raise Error("private path request targeted an old connection")
        connection.write(Span(command.frame))


def deliver(mut connection: Connection, mut service: PathService, source: PathSource, var frame: List[UInt8], nested: Bool = False) raises:
    if UInt64(control_tag(Span(frame))) == BATCH:
        if nested:
            raise Error("nested private path batch")
        var batch = decode_stream_frame(frame^)
        for item in batch.batch:
            deliver(connection, service, source, item.copy(), True)
        return
    var message = decode_path_message(frame^)
    if message:
        var effect = service.apply(source, message.take())
        if effect.cancel:
            connection.write(Span(effect.cancel.value().frame))


def complete(mut connection: Connection, mut service: PathService, source: PathSource) raises:
    for _ in range(4096):
        deliver(connection, service, source, connection.read_frame())
        if not service.pending:
            return
    raise Error("private path daemon did not finish its listing")


def main() raises:
    var args = argv()
    var interactive = connect_interactive(args[1], "path-proof", native_paths=True)
    var source = PathSource(interactive.server_id, interactive.client_id, 1)
    ref connection = interactive.transport
    var service = PathService()
    var commands = service.set_directory(source, "here", String(args[2], "/"))
    send(connection, Span(commands), source)
    complete(connection, service, source)
    if not service.listing or service.listing.value().error or not service.listing.value().root or service.listing.value().truncated:
        raise Error("native daemon failed the private path fixture: " + (service.listing.value().error.value() if service.listing and service.listing.value().error else String("missing root or truncated")))
    ref listing = service.listing.value()
    if listing.root.value().root != args[2]:
        raise Error("native daemon path root changed in the Mojo service")
    var found = 0
    for chunk in listing.chunks:
        for entry in chunk[].entries:
            if entry.rel == "alpha.txt" and entry.kind == 0 and not entry.symlink:
                found |= 1
            elif entry.rel == "quote's space 世界" and entry.kind == 0 and not entry.symlink:
                found |= 2
            elif entry.rel == "sub" and entry.kind == 1 and not entry.symlink:
                found |= 4
            elif entry.rel == "sub/nested.txt" and entry.kind == 0 and not entry.symlink:
                found |= 8
            elif entry.rel == "link" and entry.kind == 0 and entry.symlink:
                found |= 16
            else:
                raise Error("unexpected native path entry in private fixture: " + entry.rel)
    if found != 31:
        raise Error("private native listing lost a file, directory or symlink")
    commands = service.set_directory(source, "here", String(args[2], "-missing/"))
    send(connection, Span(commands), source)
    complete(connection, service, source)
    if not service.listing.value().error or not service.listing.value().done or not service.listing.value().is_empty():
        raise Error("native path error retained stale rows")
    commands = service.set_directory(source, "here", String(args[2], "/"))
    send(connection, Span(commands), source)
    commands = service.set_directory(source, "here", String(args[2], "/sub/"))
    if len(commands) != 2:
        raise Error("private native replacement omitted cancellation")
    send(connection, Span(commands), source)
    complete(connection, service, source)
    var entries = 0
    for chunk in service.listing.value().chunks:
        for entry in chunk[].entries:
            if entry.rel != "nested.txt" or entry.kind != 0 or entry.symlink:
                raise Error("a canceled native path request contaminated its replacement")
            entries += 1
    if entries != 1 or service.listing.value().error:
        raise Error("native path replacement was incomplete")
    _ = service.close()
    if service.listing or service.pending:
        raise Error("private path close retained daemon state")
    print("Private path daemon: exact files, symlink, error, cancellation and replacement passed")
