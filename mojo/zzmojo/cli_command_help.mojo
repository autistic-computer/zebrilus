from .cli_command_help_data import command_help_bytes
from .cli_preflight import _catalog, _resolve, CliPreflight
from .cli_parse import MAX_CLI_BYTES
from .protocol import Decoder


@fieldwise_init
struct CommandHelp(Copyable, Movable):
    var code: UInt8
    var text: String
    var error: Optional[String]


@inline(.never)
def command_help(name: String) raises -> CommandHelp:
    if name.byte_length() > MAX_CLI_BYTES:
        return CommandHelp(2, String(), "zz: command help name exceeds its byte limit")
    var entries = _catalog()
    var resolved = CliPreflight()
    var index = _resolve(entries, name, resolved)
    if index < 0:
        return CommandHelp(2, String(), resolved.error)
    var data = command_help_bytes()
    if len(data) > 512 * 1024:
        raise Error("command help catalog exceeds its byte limit")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 512:
        raise Error("command help catalog exceeds its record limit")
    var found: Optional[String] = None
    for _ in range(Int(count)):
        var key = reader.text()
        var value = reader.text()
        if key.byte_length() > 256 or value.byte_length() > 64 * 1024:
            raise Error("command help record exceeds its byte limit")
        if key == entries[index].name:
            found = value^
    reader.finish()
    if not found:
        raise Error("command help catalog is missing its resolved command")
    return CommandHelp(0, found.take(), None)
