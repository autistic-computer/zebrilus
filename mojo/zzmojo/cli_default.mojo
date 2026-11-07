from .cli_default_core import DefaultParse, DefaultCharacters, DefaultBuilder, MAX_DEFAULT_BYTES
from .cli_default_scan import scan_default


def parse_default(text: String, home: Optional[String] = None, homes: Dict[String, Optional[String]] = Dict[String, Optional[String]]()) raises -> DefaultParse:
    if text.byte_length() > MAX_DEFAULT_BYTES:
        var result = DefaultParse()
        result.kind = 2
        result.error = "default command exceeds its text limit"
        return result^
    var reader = DefaultCharacters(text, False)
    var builder = DefaultBuilder(False, home, homes)
    scan_default(reader, builder)
    if builder.needs_bytes:
        reader = DefaultCharacters(text, True)
        builder = DefaultBuilder(True, home, homes)
        scan_default(reader, builder)
    return builder.take_result()
