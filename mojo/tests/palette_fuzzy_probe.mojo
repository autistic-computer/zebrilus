from std.sys import argv
from zzmojo.protocol import Decoder, append_uint
from zzmojo.palette_fuzzy import PaletteMatcher


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 32 * 1024 * 1024:
        raise Error("palette fixture batch exceeds its byte budget")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("palette fixture count exceeds its limit")
    var matcher = PaletteMatcher()
    var output = List[UInt8]()
    for _ in range(Int(count)):
        var query = reader.text()
        var label = reader.text()
        var item = List[UInt8]()
        try:
            var found = matcher.find(query, label)
            append_uint(item, UInt64(2 if found else 1))
            if found:
                append_uint(item, UInt64(found.value().score))
                append_uint(item, UInt64(len(found.value().hits)))
                for hit in found.value().hits:
                    append_uint(item, UInt64(hit.start))
                    append_uint(item, UInt64(hit.end))
        except:
            append_uint(item, 0)
        append_uint(output, UInt64(len(item)))
        output.extend(Span(item))
        if len(output) > 16 * 1024 * 1024:
            raise Error("palette fixture output exceeds its byte budget")
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
