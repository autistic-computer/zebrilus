from mjui.ui import Ui, Style, TextStyle
from mjui.text.paragraph import TextOpts
from mjui.widgets.penna import PennaTheme, label
from mjui.widgets.icons import ICON_COPY, ICON_FILE, ICON_ARROW_DOWN
from .terminal_model import TerminalMode


@fieldwise_init
struct ModeIndicator(Copyable, Movable):
    var label: Optional[String]
    var detail: String
    var icon: UInt32


def mode_indicator(
    mode: TerminalMode, unseen: UInt32
) -> Optional[ModeIndicator]:
    if mode.kind == 0:
        if unseen == 0:
            return None
        return ModeIndicator(
            None, "+" + String(unseen) + " output", ICON_ARROW_DOWN
        )
    if mode.kind == 1:
        var detail = String()
        if not mode.hide_position:
            detail = String(mode.position, " / ", mode.total)
        if unseen > 0:
            if detail.byte_length() > 0:
                detail += " · "
            detail += "+" + String(unseen) + " output"
        return ModeIndicator(String("Copy mode"), detail^, ICON_COPY)
    if mode.kind == 2:
        return ModeIndicator(
            String("View mode"),
            String(mode.position, " / ", mode.total),
            ICON_FILE,
        )
    return None


def render_mode_indicator(
    mut ui: Ui,
    theme: PennaTheme,
    mode: TerminalMode,
    unseen: UInt32,
    key: UInt64,
    width: Float32,
) raises -> Bool:
    var indicator = mode_indicator(mode, unseen)
    if not indicator:
        return False
    var paint = theme.paint("chip.floating")
    ui.open(
        Style.row()
        .at(8, 8)
        .width(max(width - 16, Float32(0)))
        .height(28)
        .align(2, 1)
        .id(key)
    )
    ui.open(paint.style(Style.row().height(28).pad(10, 5).spacing(7).center()))
    paint.decorations(ui)
    ui.open(Style.col().width(13).height(16))
    ui.open(Style.col().at(0, 2).width(13).height(13))
    var icon = String(chr(Int(indicator.value().icon)))
    ui.text_opts(
        icon.unsafe_ptr(),
        icon.byte_length(),
        TextStyle(theme.flat.icons, 13, paint.ink, False),
        TextOpts().line_height(13),
    )
    ui.close()
    ui.close()
    if indicator.value().label:
        label(ui, theme, indicator.value().label.value(), paint.ink, 13, 16)
    if indicator.value().detail.byte_length() > 0:
        var detail = indicator.value().detail
        ui.text_opts(
            detail.unsafe_ptr(),
            detail.byte_length(),
            TextStyle(theme.flat.mono, 12, theme.flat.dim, False),
            TextOpts().line_height(16),
        )
    ui.close()
    ui.close()
    return True
