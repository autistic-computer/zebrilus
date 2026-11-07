from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_bool, read_uint, read_text, read_optional_id
from zzmojo.palette_places_model import (
    PaletteFleet,
    PaletteFleetHost,
    PaletteFleetSession,
    PaletteFleetWindow,
    PaletteFleetPane,
    PalettePlace,
)


def optional_text(mut reader: Decoder) raises -> Optional[String]:
    if read_bool(reader):
        return read_text(reader, 4 * 1024 * 1024)
    return None


def read_place(mut reader: Decoder) raises -> PalettePlace:
    var host: Optional[UInt32] = None
    if read_bool(reader):
        host = UInt32(read_uint(reader, 4294967295))
    return PalettePlace(
        host,
        read_optional_id(reader),
        read_optional_id(reader),
        read_optional_id(reader),
    )


def write_optional(mut bytes: List[UInt8], value: Optional[UInt64]):
    append_uint(bytes, UInt64(Bool(value)))
    if value:
        append_uint(bytes, value.value())


def write_text(mut bytes: List[UInt8], value: Optional[String]):
    append_uint(bytes, UInt64(Bool(value)))
    if value:
        append_text(bytes, value.value())


def write_place(mut bytes: List[UInt8], value: PalettePlace):
    append_uint(bytes, UInt64(Bool(value.host)))
    if value.host:
        append_uint(bytes, UInt64(value.host.value()))
    write_optional(bytes, value.session)
    write_optional(bytes, value.window)
    write_optional(bytes, value.pane)


def read_fleet(mut reader: Decoder) raises -> PaletteFleet:
    var fleet = PaletteFleet()
    fleet.here = UInt32(read_uint(reader, 4294967295))
    var hosts = Int(read_uint(reader, 65536))
    for _ in range(hosts):
        var id = UInt32(read_uint(reader, 4294967295))
        var name = read_text(reader, 4 * 1024 * 1024)
        var online = read_bool(reader)
        var current = read_optional_id(reader)
        var sessions = List[PaletteFleetSession]()
        var session_count = Int(read_uint(reader, 65536))
        for _ in range(session_count):
            var session_id = reader.uint()
            var session_name = read_text(reader, 4 * 1024 * 1024)
            var label = read_text(reader, 4 * 1024 * 1024)
            var windows = List[PaletteFleetWindow]()
            var window_count = Int(read_uint(reader, 65536))
            for _ in range(window_count):
                var window_id = reader.uint()
                var index = UInt32(read_uint(reader, 4294967295))
                var title = read_text(reader, 4 * 1024 * 1024)
                var active = read_bool(reader)
                var active_pane = reader.uint()
                var panes = List[PaletteFleetPane]()
                var pane_count = Int(read_uint(reader, 65536))
                for _ in range(pane_count):
                    var pane_id = reader.uint()
                    var pane_index = UInt32(read_uint(reader, 4294967295))
                    var pane_title = read_text(reader, 4 * 1024 * 1024)
                    var program = optional_text(reader)
                    var cwd = read_text(reader, 4 * 1024 * 1024)
                    var terminal = read_bool(reader)
                    var needs = read_bool(reader)
                    panes.append(
                        PaletteFleetPane(
                            pane_id,
                            pane_index,
                            pane_title^,
                            program^,
                            cwd^,
                            terminal,
                            needs,
                        )
                    )
                windows.append(
                    PaletteFleetWindow(
                        window_id, index, title^, active, active_pane, panes^
                    )
                )
            sessions.append(
                PaletteFleetSession(session_id, session_name^, label^, windows^)
            )
        fleet.hosts.append(
            PaletteFleetHost(id, name^, online, current, sessions^)
        )
    fleet.validate()
    return fleet^
