from std.sys import argv
from std.testing import assert_equal
from zzmojo.protocol import Decoder
from zzmojo.tree_wire import read_text
from zzmojo.chooser_plane_protocol import ChooserPlane, decode_chooser_plane
from zzmojo.terminal_model import TerminalViewport
from terminal_codec_support import viewport_fingerprint


def _n(mut output: String, value: Int):
    output += String(value) + ","


def _text(mut output: String, value: String):
    _n(output, value.byte_length())
    for byte in value.as_bytes():
        _n(output, Int(byte))


def _viewport(mut output: String, value: Optional[TerminalViewport]):
    _n(output, Int(Bool(value)))
    if value:
        output += viewport_fingerprint(value.value())


def plane_fingerprint(plane: ChooserPlane) -> String:
    var output = String()
    _n(output, Int(plane.selected))
    _n(output, len(plane.rows))
    for index in range(len(plane.rows)):
        ref row = plane.rows[index]
        _text(output, row.name)
        _text(output, row.text)
        _n(output, Int(row.align))
    _text(output, plane.sort)
    _text(output, plane.view)
    _n(output, Int(plane.filter))
    _text(output, plane.selection_style)
    _text(output, plane.border_style)
    _text(output, plane.prompt_style)
    _n(output, Int(plane.preview_size))
    _n(output, Int(Bool(plane.preview)))
    if plane.preview:
        ref preview = plane.preview.value()
        _n(output, Int(preview.kind))
        _n(output, len(preview.tiles))
        for index in range(len(preview.tiles)):
            ref tile = preview.tiles[index]
            _text(output, tile.label)
            _text(output, tile.label_style)
            _text(output, tile.border_style)
            _viewport(output, tile.viewport)
        _n(output, Int(preview.current))
        _viewport(output, preview.viewport)
        _n(output, len(preview.lines))
        for line in preview.lines:
            _text(output, line)
        _text(output, preview.status_style)
        _n(output, Int(preview.status_width))
        _text(output, preview.border_style)
    _n(output, Int(plane.cursor_style))
    _n(output, Int(Bool(plane.cursor_colour)))
    if plane.cursor_colour:
        _n(output, Int(plane.cursor_colour.value().kind))
        _n(output, Int(plane.cursor_colour.value().value))
    _n(output, Int(plane.prompt_column))
    _text(output, plane.prompt_line)
    return output^


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(String(args[1]), "r") as source:
        bytes = source.read_bytes()
    var reader = Decoder(bytes^)
    reader.offset = 0
    var count = Int(reader.uint())
    for index in range(count):
        var frame = reader.bytes()
        var expected = reader.byte()
        var sequence = reader.uint()
        var fingerprint = read_text(reader)
        var status: UInt8 = 0
        var actual = String()
        var actual_sequence: UInt64 = 0
        try:
            var event = decode_chooser_plane(frame^)
            status = 2
            if event:
                status = 1
                actual_sequence = event.value().sequence
                _n(actual, Int(Bool(event.value().plane)))
                if event.value().plane:
                    actual += plane_fingerprint(event.value().plane.value())
        except:
            pass
        assert_equal(status, expected, "chooser plane status " + String(index))
        if status == 1:
            assert_equal(actual_sequence, sequence)
            assert_equal(
                actual, fingerprint, "chooser plane metadata " + String(index)
            )
    reader.finish()
    print(count)
