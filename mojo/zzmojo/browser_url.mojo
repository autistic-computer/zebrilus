from std.ffi import external_call
from .palette_text import palette_trim

comptime MAX_BROWSER_URL_BYTES = 65536
comptime MAX_BROWSER_URL_OUTPUT = 256 * 1024


@fieldwise_init
struct BrowserUrlResult(Copyable, Movable):
    var status: UInt8
    var text: String


@inline(.never)
def browser_url(
    input: String, operation: UInt32 = 0, provider: UInt32 = 0
) raises -> BrowserUrlResult:
    if input.byte_length() > MAX_BROWSER_URL_BYTES:
        raise Error("browser URL input exceeds byte limit")
    var scratch = UInt8(0)
    var count = external_call["zz_browser_url", Int64](
        operation,
        provider,
        input.unsafe_ptr(),
        input.byte_length(),
        Pointer(to=scratch),
        0,
    )
    if count >= -3 and count <= -1:
        return BrowserUrlResult(UInt8(-count), String())
    if count < 0 or count > MAX_BROWSER_URL_OUTPUT:
        raise Error("browser URL boundary refused input or output capacity")
    if count == 0:
        return BrowserUrlResult(0, String())
    var bytes = List[UInt8](length=Int(count), fill=0)
    var result = external_call["zz_browser_url", Int64](
        operation,
        provider,
        input.unsafe_ptr(),
        input.byte_length(),
        bytes.unsafe_ptr(),
        len(bytes),
    )
    if result != count:
        raise Error("browser URL boundary changed its output size")
    return BrowserUrlResult(0, String(unsafe_from_utf8=Span(bytes)))


def browser_profile(input: String) raises -> String:
    var value = palette_trim(input)
    if value.byte_length() == 0 or value.byte_length() > 64:
        raise Error("browser profile has invalid byte length")
    for scalar in value.codepoints():
        var point = Int(scalar)
        if point <= 31 or point >= 127 and point <= 159:
            raise Error("browser profile contains a control character")
    return String("default") if value == "zz-default" else value^


def browser_load_error(
    url: String, description: String, background: Bool
) raises -> String:
    var diagnostic = browser_url(url, 2).text
    return (
        (String("Could not load ") if background else String())
        + diagnostic
        + ": "
        + description
    )
