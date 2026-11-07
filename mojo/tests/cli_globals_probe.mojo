from std.sys import argv
from zzmojo.cli_parse import (
    parse_globals,
    application_words,
    terminal_feature_mask,
)
from zzmojo.cli_native import (
    label_socket,
    local_default_socket,
    environment_value,
)
from zzmojo.protocol import Decoder, append_uint, append_bytes, append_text


def texts(mut output: List[UInt8], words: List[List[UInt8]]):
    append_uint(output, UInt64(len(words)))
    for word in words:
        append_text(output, String(from_utf8_lossy=Span(word)))


def strings(mut output: List[UInt8], words: List[String]):
    append_uint(output, UInt64(len(words)))
    for word in words:
        append_text(output, word)


def optional(mut output: List[UInt8], value: Optional[String]):
    output.append(UInt8(Bool(value)))
    if value:
        append_text(output, value.value())


def main() raises:
    var args = argv()
    var output = List[UInt8]()
    if args[1] == "default":
        output = local_default_socket()
    else:
        var bytes: List[UInt8]
        with open(args[2], "r") as source:
            bytes = source.read_bytes()
        var reader = Decoder(bytes^)
        reader.offset = 0
        var count = Int(reader.uint())
        for _ in range(count):
            var words = List[List[UInt8]]()
            var length = Int(reader.uint())
            for _ in range(length):
                words.append(reader.bytes())
            if args[1] == "features":
                var specs = List[String]()
                for word in words:
                    specs.append(String(from_utf8=Span(word)))
                append_uint(output, UInt64(terminal_feature_mask(specs)))
                continue
            var cleaned = application_words(words)
            append_uint(output, UInt64(len(cleaned)))
            for word in cleaned:
                append_bytes(output, Span(word))
            var result = parse_globals(
                words, String("/oracle/default.sock").as_bytes()
            )
            if result.error_kind == 0 and result.socket_source == 2:
                try:
                    result.socket = label_socket(
                        Span(result.socket), environment_value("TMUX_TMPDIR")
                    )
                except error:
                    result.fail(2, String(error))
            output.append(result.error_kind)
            if result.error_kind != 0:
                append_text(output, result.error)
                continue
            append_bytes(output, Span(result.socket))
            output.append(result.socket_source)
            optional(output, result.host)
            texts(output, result.remaining)
            texts(output, result.configs)
            output.append(UInt8(result.no_start))
            output.append(result.control)
            optional(output, result.shell)
            output.append(UInt8(result.login))
            optional(output, result.early)
            output.append(UInt8(result.utf8))
            strings(output, result.features)
    with open(args[3], "w") as destination:
        destination.write_bytes(Span(output))
