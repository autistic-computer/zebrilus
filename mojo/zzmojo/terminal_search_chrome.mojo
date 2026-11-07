from mjui.ui import Ui, Style, TextStyle, CURSOR_HAND
from mjui.sem import ROLE_BUTTON, ROLE_REGION, SEM_DISABLED, SEM_SELECTED
from mjui.widgets.penna import PennaTheme, label
from mjui.widgets.penna_metrics import cap_center_drop
from mjui.widgets.controls import tooltip
from mjui.widgets.penna_contexts import appearance, recipe, state as context_state, ground
from mjui.widgets.icons import ICON_SEARCH, ICON_CHEVRON_UP, ICON_CHEVRON_DOWN, ICON_X, ICON_COPY, ICON_FILE, ICON_ARROW_DOWN
from mjui.text.paragraph import TextOpts, Q
from .terminal_model import TerminalSearch, TerminalMode
from .terminal_mode_indicator import mode_indicator
from .terminal_view import search_result_text
from .terminal_search import SearchEffect
from .terminal_search_editor import SearchEditor, SearchInputResult


@fieldwise_init
struct SearchKeys(TrivialRegisterPassable):
    var panel: UInt64
    var field: UInt64
    var container: UInt64
    var regex: UInt64
    var case_policy: UInt64
    var previous: UInt64
    var next: UInt64
    var close: UInt64


def _icon_button(
    mut ui: Ui,
    theme: PennaTheme,
    key: UInt64,
    icon: UInt32,
    title: StringSpan,
    selected: Bool = False,
    disabled: Bool = False,
) raises -> Bool:
    var hovered = False
    var held = False
    var clicked = False
    if not disabled:
        var response = ui.interact(key, focusable=True, focus_on_press=False, activate_on_release=True)
        hovered, held, clicked = response.hovered, response.held, response.clicked
        clicked = clicked or ui.nav(key).activate()
        if hovered or held:
            ui.cursor = CURSOR_HAND
    var state = "disabled" if disabled else "current" if selected else "active" if held else "hover" if hovered else "rest"
    var paint = appearance(theme.theme, recipe("icon-button.flat"), context_state(state), ground("popover"), theme.flat.penna_reduced)
    ui.open(paint.style(Style.row().width(24).height(24).center().id(key)))
    paint.decorations(ui, ui.focus_ring(key))
    ui.sem(ROLE_BUTTON, title, (SEM_DISABLED if disabled else 0) | (SEM_SELECTED if selected else 0))
    ui.open(Style.col().width(18).height(18))
    ui.open(Style.col().width(18).height(18).at(0, 0.5))
    ui.icon(icon, theme.flat.icons, 18, paint.ink, paint.color(1))
    ui.close()
    ui.close()
    ui.close()
    tooltip(ui, theme.flat, key, title)
    return clicked


def search_navigation_enabled(status: Optional[TerminalSearch]) -> Bool:
    return Bool(status) and status.value().total > 0 and not status.value().pending and not status.value().invalid_pattern


def build_search_chrome(
    mut editor: SearchEditor,
    mut ui: Ui,
    theme: PennaTheme,
    keys: SearchKeys,
    pane: UInt64,
    x: Float32,
    y: Float32,
    available_width: Float32,
    status: Optional[TerminalSearch],
    command_output: Bool = False,
) raises -> SearchInputResult:
    var result = SearchInputResult()
    if not editor.session.query:
        return result^
    var query = editor.session.query.value().copy()
    var panel = theme.paint("popover")
    var width = min(Float32(460), max(Float32(0), available_width))
    ui.overlay_begin(x, y, panel.style(Style.row().width(width).pad(4, 4).spacing(4).wrap().align(0, 1).id(keys.panel)))
    panel.decorations(ui)
    ui.sem(ROLE_REGION, "Terminal search")
    var field_response = ui.interact(keys.container)
    if field_response.pressed:
        ui.focus = keys.field
        editor.field.focused = True
        var position = ui.text_hit(keys.field, ui.press_x, ui.press_y)
        if position.byte >= 0:
            editor.field.select(position.byte, position.byte)
        editor.field.blink_t0 = ui.time
    var field = appearance(theme.theme, recipe("field"), context_state("focused" if ui.focus == keys.field else "rest"), ground("popover"), theme.flat.penna_reduced)
    ui.open(field.style(Style.row().grow().min_width(min(Float32(110), max(Float32(0), width - 8))).height(28).pad(8, 2).spacing(6).align(0, 1).id(keys.container)))
    field.decorations(ui, ui.focus_ring(keys.field))
    var drop = cap_center_drop(ui, theme.flat.font, 13, 13)
    ui.open(Style.col().width(13).height(18))
    ui.open(Style.col().width(13).height(13).at(0, 2.5 + drop))
    ui.icon(ICON_SEARCH, theme.flat.icons, 13, field.muted, field.color(2))
    ui.close()
    ui.close()
    editor.field.use_theme(theme.flat.on("popover"))
    editor.field.style = TextStyle(theme.flat.font, 13, field.ink, False, field.color(1))
    editor.field.build(ui, Style.row().grow().pad(0, 0))
    ui.close()
    ui.open(Style.row().spacing(2).align(0, 1))
    if query.text.byte_length() > 0:
        var value = status.value() if status else TerminalSearch(0, 0, False, False)
        var text = search_result_text(Int(value.current), Int(value.total), value.pending, value.invalid_pattern)
        var color = theme.flat.danger if value.invalid_pattern else panel.muted
        ui.open(Style.row().pad(4, 0))
        label(ui, theme, text, color, 11, 16)
        ui.close()
    if _icon_button(ui, theme, keys.regex, 0xE1EF, "Regular expression", query.mode == 1):
        result.append(editor.session.toggle_regex(pane, command_output))
    var case_title = "Match case" if query.case_policy == 1 else "Ignore case" if query.case_policy == 2 else "Smart case"
    if _icon_button(ui, theme, keys.case_policy, 0xE3D9, case_title, query.case_policy == 1):
        result.append(editor.session.toggle_case(pane, command_output))
    var disabled = not search_navigation_enabled(status)
    if _icon_button(ui, theme, keys.previous, ICON_CHEVRON_UP, "Previous match", disabled=disabled):
        result.append(editor.session.step(pane, True, command_output))
    if _icon_button(ui, theme, keys.next, ICON_CHEVRON_DOWN, "Next match", disabled=disabled):
        result.append(editor.session.step(pane, False, command_output))
    if _icon_button(ui, theme, keys.close, ICON_X, "Close search"):
        result.append(editor.session.close(pane, command_output))
    ui.close()
    ui.overlay_end()
    return result^


def _text_width(mut ui: Ui, text: String, font: Int32, size: Float32, color: UInt32) -> Float32:
    var index = ui.layout_text(text.unsafe_ptr(), text.byte_length(), TextStyle(font, size, color, False), 1_000_000)
    return Float32(ui.paras.paras[index].unwrapped) / Q


def build_mode_indicator(
    mut ui: Ui,
    theme: PennaTheme,
    key: UInt64,
    mode: TerminalMode,
    unseen_output: UInt32,
    right: Float32,
    top: Float32,
) raises:
    var indicator = mode_indicator(mode, unseen_output)
    if not indicator:
        return
    var value = indicator.take()
    var paint = theme.paint("chip.floating")
    var width: Float32 = 40
    if value.label:
        width += _text_width(ui, value.label.value(), theme.flat.font, 13, paint.ink)
    if value.detail.byte_length() > 0:
        width += _text_width(ui, value.detail, theme.flat.mono, 12, paint.muted)
        if value.label:
            width += 7
    ui.overlay_begin(right - width, top, paint.style(Style.row().min_height(28).pad(10, 5).spacing(7).align(0, 1).id(key)))
    paint.decorations(ui)
    var icon = ICON_COPY if mode.kind == 1 else ICON_FILE if mode.kind == 2 else ICON_ARROW_DOWN
    ui.open(Style.col().width(13).height(16))
    ui.open(Style.col().width(13).height(13).at(0, 2))
    ui.icon(icon, theme.flat.icons, 13, paint.ink, paint.color(1))
    ui.close()
    ui.close()
    ui.open(Style.row().spacing(7).align(0, 1))
    if value.label:
        label(ui, theme, value.label.value(), paint.ink, 13, 16, native=paint.color(1))
    if value.detail.byte_length() > 0:
        ui.text_opts(value.detail.unsafe_ptr(), value.detail.byte_length(), TextStyle(theme.flat.mono, 12, paint.muted, False, paint.color(2)), TextOpts().line_height(16))
    ui.close()
    ui.overlay_end()
