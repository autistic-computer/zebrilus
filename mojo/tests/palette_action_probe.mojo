from std.sys import argv
from std.testing import assert_equal, assert_true
from zzmojo.protocol import Decoder, append_bytes, append_uint
from zzmojo.palette_protocol import (
    palette_command_request,
    read_palette_command,
    palette_cancel_prefix,
    PALETTE_COMMAND_BYTES,
)
from zzmojo.exec_protocol import Invocation
from zzmojo.prompt_protocol import PromptAction, prompt_request
from zzmojo.chooser_protocol import ChooseAction, choose_request


def boundaries() raises:
    var command = Invocation()
    command.name = "x"
    command.args.append(
        List[UInt8](length=PALETTE_COMMAND_BYTES - 23, fill=120)
    )
    var frame = palette_command_request(0, command)
    assert_equal(len(frame), PALETTE_COMMAND_BYTES)
    var decoded = read_palette_command(frame^)
    assert_equal(len(decoded.command.args[0]), PALETTE_COMMAND_BYTES - 23)
    command.args[0].append(120)
    var refused = False
    try:
        _ = palette_command_request(0, command)
    except:
        refused = True
    assert_true(refused)
    command.args.clear()
    for _ in range(4096):
        command.args.append(List[UInt8]())
    _ = palette_command_request(0, command)
    command.args.append(List[UInt8]())
    refused = False
    try:
        _ = palette_command_request(0, command)
    except:
        refused = True
    assert_true(refused)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[1], "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    var output = List[UInt8]()
    append_uint(output, UInt64(count))
    for _ in range(count):
        var kind = reader.uint()
        var request = reader.uint()
        var frame = reader.bytes()
        var actual: List[UInt8]
        if kind == 0:
            var command = read_palette_command(frame.copy())
            actual = palette_command_request(command.request, command.command)
        elif kind == 1:
            actual = palette_cancel_prefix(request)
        elif kind == 2:
            actual = prompt_request(PromptAction(2))
        else:
            assert_equal(kind, UInt64(3))
            actual = choose_request(ChooseAction(17))
        assert_equal(len(actual), len(frame))
        for index in range(len(frame)):
            assert_equal(actual[index], frame[index])
        append_bytes(output, Span(actual))
    reader.finish()
    boundaries()
    with open(args[2], "w") as target:
        target.write_all(Span(output))
    print(
        "palette command metadata and native prefix/prompt/chooser actions"
        " passed"
    )
