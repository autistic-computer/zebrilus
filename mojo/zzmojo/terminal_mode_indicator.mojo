from .terminal_model import TerminalMode


@fieldwise_init
struct ModeIndicator(Copyable, Movable):
    var label: Optional[String]
    var detail: String


def mode_indicator(mode: TerminalMode, unseen_output: UInt32) raises -> Optional[ModeIndicator]:
    if mode.kind == 0:
        if unseen_output > 0:
            return ModeIndicator(None, String("+", unseen_output, " output"))
        return None
    if mode.kind == 1:
        var detail = String()
        if not mode.hide_position:
            detail = String(mode.position, " / ", mode.total)
        if unseen_output > 0:
            if detail.byte_length() > 0:
                detail += " · "
            detail += String("+", unseen_output, " output")
        return ModeIndicator(String("Copy mode"), detail)
    if mode.kind == 2:
        return ModeIndicator(String("View mode"), String(mode.position, " / ", mode.total))
    raise Error("unknown terminal mode")
