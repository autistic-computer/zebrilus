from std.sys import argv
from std.testing import assert_true
from zzmojo.stream_protocol import (
    decode_stream_frame,
    encode_stream_frame,
    interactive_hello,
    request_full,
    tree_sync,
    resize_terminal,
    client_terminal_size,
    client_focus,
    input_text,
    pong,
)
from zzmojo.tree_encode import snapshot_frame


def read_frame(path: String) raises -> List[UInt8]:
    with open(path, "r") as source:
        return source.read_bytes()


def main() raises:
    var args = argv()
    var command = args[1]
    var output: List[UInt8]
    var destination: String
    if command == "roundtrip":
        var frame = decode_stream_frame(read_frame(args[2]))
        output = encode_stream_frame(frame)
        destination = args[3]
    elif command == "reduce" or command == "reject":
        var before = decode_stream_frame(read_frame(args[2]))
        var update = decode_stream_frame(read_frame(args[3]))
        if not before.snapshot:
            raise Error("reference before frame has no snapshot")
        var snapshot = before.snapshot.value().copy()
        var rejected = False
        try:
            if update.delta:
                snapshot.apply(update.delta.value())
            elif update.view:
                update.view.value().apply(snapshot)
            else:
                raise Error("reference update is neither delta nor client view")
        except error:
            if command != "reject":
                raise error^
            rejected = True
        assert_true(rejected == (command == "reject"))
        output = snapshot_frame(snapshot)
        destination = args[4]
    elif command == "reject-frame":
        var rejected = False
        try:
            _ = decode_stream_frame(read_frame(args[2]))
        except:
            rejected = True
        assert_true(rejected)
        return
    elif command == "hello":
        output = interactive_hello(
            UInt64.MAX, UInt32.MAX, "$18446744073709551585", 120, 40, 9, 18
        )
        destination = args[2]
    elif command == "request-full":
        output = request_full(UInt64.MAX)
        destination = args[2]
    elif command == "resize-terminal-v2":
        output = resize_terminal(UInt64.MAX, 120, 40, 9, 18, UInt64.MAX)
        destination = args[2]
    elif command == "client-size-v2":
        output = client_terminal_size(120, 40, UInt64.MAX)
        destination = args[2]
    elif command == "client-focus":
        output = client_focus(True)
        destination = args[2]
    elif command == "text-input":
        output = input_text(UInt64.MAX, "世界\n")
        destination = args[2]
    elif command == "tree-sync":
        output = tree_sync()
        destination = args[2]
    elif command == "pong":
        output = pong(UInt64.MAX)
        destination = args[2]
    else:
        raise Error("unknown control probe command")
    with open(destination, "w") as target:
        target.write_all(Span(output))
