from std.math import floor
from mjui.ui import Ui, Style, TextStyle, START, CENTER, END, CURSOR_HAND
from mjui.sem import ROLE_BUTTON, ROLE_TAB, ROLE_TAB_LIST, ROLE_TOOLBAR, ROLE_TEXTBOX, ROLE_ALERT, ROLE_STATUS, ROLE_LINK, SEM_DISABLED, SEM_SELECTED, SEM_PLACEHOLDER
from mjui.native_color import NativeColor
from mjui.native_quad import NativeSurface
from mjui.native_geometry import native_snap
from mjui.text.paragraph import TextOpts, TEXT_CENTER
from mjui.text.native_truncate import native_end_truncate
from mjui.widgets.penna import PennaTheme
from mjui.widgets.penna_paint import PennaPaint
from mjui.widgets.penna_native import root_color
from mjui.widgets.penna_primitives_native import root_color as primitive_root, field_faint
from mjui.widgets.penna_contexts import appearance, recipe, state as context_state, ground, inherited
from mjui.widgets.penna_geometry import length, stroke, text_width
from mjui.widgets.penna_icon_names import named_icon
from mjui.widgets.penna_palette_shortcuts import _packed
from mjui.widgets.penna_palette_surface_data import surface_rule
from mjui.widgets.controls import tooltip
from mjui.widgets.key import sub
from .browser_presentation import BrowserPresentation, BrowserTabLabel, BROWSER_STATE_EMPTY, BROWSER_STATE_ERROR, BROWSER_RETRY_LABEL

comptime BROWSER_CHROME_GAP: Float32 = 6
comptime BROWSER_CONTROL_HEIGHT: Float32 = 28
comptime BROWSER_TAB_ROW_HEIGHT: Float32 = 40
comptime BROWSER_HEADER_HEIGHT: Float32 = 74
comptime BROWSER_TAB_MAX_WIDTH: Float32 = 180
comptime BROWSER_TAB_MIN_WIDTH: Float32 = 36
comptime BROWSER_TAB_LABEL_WIDTH: Float32 = 84
comptime BROWSER_TAB_GAP: Float32 = 4
comptime BROWSER_TAB_RIM: Float32 = 2
comptime BROWSER_LABEL_FADE: Float32 = 28
comptime BROWSER_ADDRESS_PLACEHOLDER = "Search or enter a URL"
comptime BROWSER_ERROR_TITLE = "This page couldn’t load"
comptime BROWSER_EMPTY_TITLE = "Where to?"
comptime BROWSER_EMPTY_CAPTION = "Type a URL to get started. Pages you visit will show up here."

comptime BROWSER_CHROME_NONE: UInt8 = 0
comptime BROWSER_CHROME_SELECT_TAB: UInt8 = 1
comptime BROWSER_CHROME_CLOSE_TAB: UInt8 = 2
comptime BROWSER_CHROME_NEW_TAB: UInt8 = 3
comptime BROWSER_CHROME_BACK: UInt8 = 4
comptime BROWSER_CHROME_FORWARD: UInt8 = 5
comptime BROWSER_CHROME_RELOAD: UInt8 = 6
comptime BROWSER_CHROME_PICKER: UInt8 = 7
comptime BROWSER_CHROME_MORE: UInt8 = 8
comptime BROWSER_CHROME_SITE_CONTROLS: UInt8 = 9
comptime BROWSER_CHROME_FOCUS_ADDRESS: UInt8 = 10
comptime BROWSER_CHROME_PANE_ACTION: UInt8 = 11
comptime BROWSER_CHROME_RETRY: UInt8 = 12
comptime BROWSER_CHROME_RECENT: UInt8 = 13

comptime BROWSER_PANE_SPLIT_BOTTOM: UInt8 = 0
comptime BROWSER_PANE_SPLIT_RIGHT: UInt8 = 1
comptime BROWSER_PANE_FLOAT: UInt8 = 2
comptime BROWSER_PANE_TILE: UInt8 = 3
comptime BROWSER_PANE_ZOOM: UInt8 = 4
comptime BROWSER_PANE_CLOSE: UInt8 = 5

comptime _KEY_TITLE = 1
comptime _KEY_NEW_TAB = 2
comptime _KEY_BACK = 3
comptime _KEY_FORWARD = 4
comptime _KEY_RELOAD = 5
comptime _KEY_ADDRESS = 6
comptime _KEY_SITE = 7
comptime _KEY_PICKER = 8
comptime _KEY_MORE = 9
comptime _KEY_ACTIONS = 16
comptime _KEY_TABS = 32


@fieldwise_init
struct BrowserChromeEvent(TrivialRegisterPassable):
    var kind: UInt8
    var value: UInt64


@fieldwise_init
struct BrowserChromeInput(Copyable, Movable):
    var focused: Bool
    var lone: Bool
    var floats: Bool
    var zoomed: Bool
    var movable: Bool
    var inset: Float32
    var picking: Bool
    var address_focused: Bool


@fieldwise_init
struct BrowserPaneAction(Copyable, Movable):
    var action: UInt8
    var icon: String
    var label: String


@fieldwise_init
struct BrowserTabLayout(TrivialRegisterPassable):
    var strip: Float32
    var group: Float32
    var content: Float32
    var pill: Float32
    var narrow: Bool


def browser_pane_actions(floats: Bool, lone: Bool, zoomed: Bool) -> List[BrowserPaneAction]:
    var actions = List[BrowserPaneAction]()
    if floats:
        actions.append(BrowserPaneAction(BROWSER_PANE_TILE, "layout-columns", "Tile back"))
    else:
        actions.append(BrowserPaneAction(BROWSER_PANE_SPLIT_BOTTOM, "panel-bottom", "Split bottom"))
        actions.append(BrowserPaneAction(BROWSER_PANE_SPLIT_RIGHT, "panel-right", "Split right"))
        if not lone and not zoomed:
            actions.append(BrowserPaneAction(BROWSER_PANE_FLOAT, "picture-in-picture", "Float"))
    if floats or not lone:
        if zoomed:
            actions.append(BrowserPaneAction(BROWSER_PANE_ZOOM, "shrink", "Unzoom"))
        else:
            actions.append(BrowserPaneAction(BROWSER_PANE_ZOOM, "expand", "Zoom"))
    actions.append(BrowserPaneAction(BROWSER_PANE_CLOSE, "xmark", "Close pane"))
    return actions^


def browser_tab_layout(
    width: Float32,
    edge: Float32,
    trailing: Float32,
    gap: Float32,
    actions: Float32,
    title: Float32,
    tabs: Int,
    tab_max: Float32,
    tab_min: Float32,
    tab_gap: Float32,
    new_tab: Float32,
    label: Float32,
) -> BrowserTabLayout:
    var count = Float32(max(tabs, 1))
    var strip = max(width - edge - trailing - gap - actions, Float32(0))
    var natural = count * tab_max + (count - 1) * tab_gap + tab_gap + new_tab
    var group = max(min(natural, strip - title - gap), Float32(0))
    var content = max(group - tab_gap - new_tab, Float32(0))
    var each = (content - (count - 1) * tab_gap) / count
    return BrowserTabLayout(strip, group, content, min(max(each, tab_min), tab_max), each < label)


def _label(mut ui: Ui, text: StringSpan, font: Int32, size: Float32, line: Float32, color: NativeColor) raises:
    ui.text_opts(text.unsafe_ptr(), text.byte_length(), TextStyle(font, size, _packed(color), False, color), TextOpts().line_height(line))


def _icon(mut ui: Ui, th: PennaTheme, name: StringSpan, size: Float32, color: NativeColor, drop: Float32 = 0) raises:
    ui.open(Style.col().width(length(ui, size)).height(size))
    ui.open(Style.col().width(length(ui, size)).height(length(ui, size)).at(0, drop))
    named_icon(ui, th.flat, name, length(ui, size), _packed(color), color, physical_layout=True)
    ui.close()
    ui.close()


def _width(mut ui: Ui, text: StringSpan, font: Int32, size: Float32, line: Float32) -> Float32:
    return text_width(ui, text, TextStyle(font, size, 0, False), line)


def _muted(th: PennaTheme) -> NativeColor:
    return primitive_root(th.theme, 3).opacity(1)


def _button(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    icon: StringSpan,
    title: StringSpan,
    role: StringSpan,
    profile: Int,
    side: Float32,
    mark: Float32,
    drop: Float32,
    ink: Optional[NativeColor] = None,
    disabled: Bool = False,
    selected: Bool = False,
    shown: Bool = True,
    x: Float32 = -1,
    y: Float32 = 0,
    radius: Float32 = -1,
) raises -> Bool:
    var size = length(ui, side)
    var style = Style.row().width(size).height(size).center().id(key)
    if x >= 0:
        style = style.at(x, y)
    if not shown:
        ui.open(style)
        ui.close()
        return False
    var hovered = False
    var held = False
    var clicked = False
    if not disabled:
        var response = ui.interact(key, focusable=True, focus_on_press=False, activate_on_release=True)
        hovered, held, clicked = response.hovered, response.held, response.clicked
        clicked = clicked or ui.nav(key).activate()
        if hovered or held:
            ui.cursor = CURSOR_HAND
    var state = "disabled" if disabled else "selected" if selected else "active" if held else "hover" if hovered else "rest"
    var paint = appearance(th.theme, recipe(role), context_state(state), profile, th.flat.penna_reduced)
    if radius >= 0:
        paint.share = 0
        paint.radius = radius
    ui.open(paint.style(style))
    paint.decorations(ui, ui.focus_ring(key), radius=radius)
    ui.sem(ROLE_BUTTON, title, (SEM_DISABLED if disabled else 0) | (SEM_SELECTED if selected else 0))
    var color = paint.color(1)
    if ink and not disabled and not selected:
        color = ink.value()
    _icon(ui, th, icon, mark, color, drop)
    ui.close()
    tooltip(ui, th.flat, key, title)
    return clicked


def _tab(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    tab: BrowserTabLabel,
    index: Int,
    count: Int,
    active: Bool,
    narrow: Bool,
    width: Float32,
    left: Float32,
    bar: Int,
) raises -> UInt8:
    var closable = count > 1
    var close_key = sub(key, 1)
    var response = ui.interact(key)
    var over = ui.hovering(key)
    if over:
        ui.cursor = CURSOR_HAND
    var state = "current" if active else "active" if response.held else "hover" if over else "rest"
    var paint = appearance(th.theme, recipe("tab.page"), context_state(state), bar, th.flat.penna_reduced)
    var muted = paint.color(2)
    var ink = paint.color(1) if active else muted
    var inner = inherited("tab.page", "current") if active else bar
    var height = length(ui, BROWSER_CONTROL_HEIGHT)
    var mark = length(ui, 14)
    var result: UInt8 = 0
    if narrow:
        ui.open(paint.style(Style.row().width(width).height(height).center().id(key)))
        paint.decorations(ui)
        ui.sem(ROLE_TAB, tab.label, SEM_SELECTED if active else 0, Int32(index), Int32(count))
        ui.open(Style.row().width(height).height(height).center())
        if not (closable and over):
            _icon(ui, th, "globe", 14, muted, 0.5)
        if closable:
            var side = length(ui, 20)
            if _button(ui, th, close_key, "xmark", "Close tab", "icon-button", inner, 20, 12, 0, muted, shown=over, x=(height - side) / 2, y=(height - side) / 2):
                result = 2
        ui.close()
        ui.close()
    else:
        var lead = length(ui, 12)
        var gap = length(ui, 6)
        var trail = height if closable else lead
        var words = max(width - lead - mark - gap - trail, Float32(0))
        var start = left + lead + mark + gap
        var snapped = max(native_snap((start + words) * ui.scale) / ui.scale - start, Float32(0))
        ui.open(paint.style(Style.row().width(width).height(height).pad_sides(lead, 0, trail, 0).spacing(gap).align(START, CENTER).id(key)))
        paint.decorations(ui)
        ui.sem(ROLE_TAB, tab.label, SEM_SELECTED if active else 0, Int32(index), Int32(count))
        _icon(ui, th, "globe", 14, muted, 0.5)
        ui.open(Style.col().width(snapped).height(16))
        ui.text_opts(tab.label.unsafe_ptr(), tab.label.byte_length(), TextStyle(th.flat.font, 13, _packed(ink), False, ink), TextOpts().line_height(16).fade(BROWSER_LABEL_FADE))
        ui.close()
        if closable:
            var side = length(ui, 20)
            if _button(ui, th, close_key, "xmark", "Close tab", "icon-button", inner, 20, 12, 0, muted, shown=over, x=width - length(ui, 4) - side, y=(height - side) / 2):
                result = 2
        ui.close()
    if tab.detail.byte_length() > 0:
        tooltip(ui, th.flat, key, tab.detail)
    if closable and ui.middle and (ui.middle_press_hot == key or ui.middle_press_hot == close_key):
        return 2
    if result == 0 and response.clicked:
        result = 1
    return result


def browser_header(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    width: Float32,
    view: BrowserPresentation,
    input: BrowserChromeInput,
) raises -> BrowserChromeEvent:
    var event = BrowserChromeEvent(BROWSER_CHROME_NONE, 0)
    var bar = ground("bar.pane")
    var reduced = th.flat.penna_reduced
    var fg = root_color(th.theme, 1)
    var rest = th.paint("row.nav").color(1)
    var muted = _muted(th)
    var edge = length(ui, BROWSER_CHROME_GAP)
    var gap = length(ui, 8)
    var control = length(ui, BROWSER_CONTROL_HEIGHT)
    var tab_gap = length(ui, BROWSER_TAB_GAP)
    var rim = length(ui, BROWSER_TAB_RIM)
    var actions = browser_pane_actions(input.floats, input.lone, input.zoomed)
    var action_side = length(ui, 22)
    var action_gap = length(ui, 4)
    var actions_width = Float32(len(actions)) * action_side + Float32(max(len(actions) - 1, 0)) * action_gap
    var mark = length(ui, 14)
    var title_lead = length(ui, 6)
    var title_trail = length(ui, 8)
    var title_width = min(title_lead + mark + title_lead + _width(ui, "Browser", th.flat.font, 13, 16) + title_trail, length(ui, 168))
    var count = len(view.tabs)
    var layout = browser_tab_layout(
        width,
        edge,
        length(ui, BROWSER_CHROME_GAP + input.inset),
        gap,
        actions_width,
        title_width,
        count,
        length(ui, BROWSER_TAB_MAX_WIDTH),
        length(ui, BROWSER_TAB_MIN_WIDTH),
        tab_gap,
        control,
        length(ui, BROWSER_TAB_LABEL_WIDTH),
    )
    var origin = ui.rect_of(key).x0
    var over = ui.hovering(key)
    ui.open(Style.col().width(width).height(length(ui, BROWSER_HEADER_HEIGHT)).id(key))
    ui.open(Style.row().width(width).height(length(ui, BROWSER_TAB_ROW_HEIGHT)).pad_sides(edge, 0, length(ui, BROWSER_CHROME_GAP + input.inset), 0).spacing(gap).align(START, CENTER))
    ui.open(Style.row().width(layout.strip).height(length(ui, BROWSER_TAB_ROW_HEIGHT)).spacing(gap).align(START, CENTER))
    var title_key = sub(key, _KEY_TITLE)
    var title_over = ui.hovering(title_key)
    var title = appearance(th.theme, recipe("button.title"), context_state("hover" if title_over else "rest"), bar, reduced)
    ui.open(title.style(Style.row().width(title_width).height(action_side).pad_sides(title_lead, 0, title_trail, 0).spacing(title_lead).align(START, CENTER).id(title_key).clipped()))
    title.decorations(ui)
    _icon(ui, th, "arrows-move" if title_over and input.movable else "globe", 14, rest)
    _label(ui, "Browser", th.flat.font, 13, 16, rest)
    ui.close()
    ui.open(Style.row().width(layout.group).height(control).spacing(tab_gap).align(START, CENTER))
    ui.open(Style.row().width(layout.content).height(control))
    ui.open(Style.row().width(layout.content + 2 * rim).height(control + 2 * rim).pad(rim, rim).spacing(tab_gap).align(START, CENTER).at(-rim, -rim).clipped())
    ui.sem(ROLE_TAB_LIST, "Browser tabs")
    var left = origin + edge + title_width + gap
    for index in range(count):
        var outcome = _tab(ui, th, sub(key, _KEY_TABS + 2 * index), view.tabs[index], index, count, index == view.active, layout.narrow, layout.pill, left, bar)
        if outcome == 1:
            event = BrowserChromeEvent(BROWSER_CHROME_SELECT_TAB, view.tabs[index].id)
        elif outcome == 2:
            event = BrowserChromeEvent(BROWSER_CHROME_CLOSE_TAB, view.tabs[index].id)
        left += layout.pill + tab_gap
    ui.close()
    ui.close()
    if _button(ui, th, sub(key, _KEY_NEW_TAB), "plus", "New tab", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted):
        event = BrowserChromeEvent(BROWSER_CHROME_NEW_TAB, 0)
    ui.close()
    ui.close()
    ui.open(Style.row().width(actions_width).height(action_side).spacing(action_gap).align(START, CENTER))
    var shown = input.focused or over
    for index in range(len(actions)):
        if _button(ui, th, sub(key, _KEY_ACTIONS + index), actions[index].icon, actions[index].label, "icon-button.bar", bar, 22, 14, 0.5, rest, shown=shown):
            event = BrowserChromeEvent(BROWSER_CHROME_PANE_ACTION, UInt64(actions[index].action))
    ui.close()
    ui.close()
    ui.open(Style.row().width(width).height(control).pad(edge, 0).spacing(action_gap).align(START, CENTER))
    ui.sem(ROLE_TOOLBAR, "Browser navigation")
    ui.open(Style.row().spacing(action_gap).align(START, CENTER))
    if _button(ui, th, sub(key, _KEY_BACK), "arrow-left", "Back", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted, disabled=not view.can_go_back):
        event = BrowserChromeEvent(BROWSER_CHROME_BACK, 0)
    if _button(ui, th, sub(key, _KEY_FORWARD), "arrow-right", "Forward", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted, disabled=not view.can_go_forward):
        event = BrowserChromeEvent(BROWSER_CHROME_FORWARD, 0)
    if _button(ui, th, sub(key, _KEY_RELOAD), "redo-2", "Reload", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted):
        event = BrowserChromeEvent(BROWSER_CHROME_RELOAD, 0)
    ui.close()
    var address_key = sub(key, _KEY_ADDRESS)
    var field_state = context_state("focused" if input.address_focused else "rest")
    var field = appearance(th.theme, recipe("field.address"), field_state, bar, reduced)
    var border = stroke(ui, field.border_width)
    var site_key = sub(key, _KEY_SITE)
    var address = ui.interact(address_key)
    ui.open(field.style(Style.row().grow().height(control).pad(border, 0).align(START, CENTER).id(address_key)))
    field.decorations(ui)
    ui.open(Style.row().pad_sides(length(ui, 1.5), 0, 0, 0))
    var site_radius = field.radius if field.radius >= 4096 else max(field.radius - Float32(2.5), Float32(0))
    if _button(ui, th, site_key, "site-controls", "Site controls", "icon-button.flat", bar, 24, 14, 0.5, muted, radius=site_radius):
        event = BrowserChromeEvent(BROWSER_CHROME_SITE_CONTROLS, 0)
    ui.close()
    ui.open(Style.row().grow().height(control).pad(gap, 0).align(START, CENTER).clipped())
    if view.address.byte_length() == 0:
        ui.sem(ROLE_TEXTBOX, BROWSER_ADDRESS_PLACEHOLDER, SEM_PLACEHOLDER)
        _label(ui, BROWSER_ADDRESS_PLACEHOLDER, th.flat.font, 13, 16, field_faint(th.theme, field_state, bar, reduced))
    else:
        ui.sem(ROLE_TEXTBOX, view.address)
        _label(ui, view.address, th.flat.font, 13, 16, fg)
    ui.close()
    ui.close()
    if address.pressed:
        event = BrowserChromeEvent(BROWSER_CHROME_FOCUS_ADDRESS, 0)
    ui.open(Style.row().spacing(action_gap).align(START, CENTER))
    if _button(ui, th, sub(key, _KEY_PICKER), "inspector", "Cancel element picker" if input.picking else "Pick an element", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted, selected=input.picking):
        event = BrowserChromeEvent(BROWSER_CHROME_PICKER, 0)
    if _button(ui, th, sub(key, _KEY_MORE), "ellipsis-vertical", "More browser actions", "icon-button", bar, BROWSER_CONTROL_HEIGHT, 14, 0.5, muted):
        event = BrowserChromeEvent(BROWSER_CHROME_MORE, 0)
    ui.close()
    ui.close()
    if view.state != BROWSER_STATE_EMPTY:
        var rule = surface_rule(th.theme)
        var line = stroke(ui, 1)
        ui.open(Style.col().width(width).height(line).fill(_packed(rule)).native_paint().at(0, length(ui, BROWSER_HEADER_HEIGHT) - line))
        ui.native_surface(NativeSurface(rule, NativeColor(), NativeColor(), 0, 0, 2, 0, False))
        ui.close()
    ui.close()
    return event


def browser_start_surface(mut ui: Ui, key: UInt64, width: Float32, height: Float32) -> Bool:
    var response = ui.interact(key)
    ui.open(Style.col().width(width).height(height).pad(length(ui, 12), 0).center().at(0, 0).id(key))
    return response.pressed


def browser_start_surface_end(mut ui: Ui):
    ui.close()


def browser_empty_hint(mut ui: Ui, th: PennaTheme, available: Float32) raises:
    var fg = root_color(th.theme, 1)
    var muted = _muted(th)
    var pad = length(ui, 20)
    var room = max(min(length(ui, 440), available) - 2 * pad, Float32(0))
    var caption = _width(ui, BROWSER_EMPTY_CAPTION, th.flat.font, 11, 16)
    var title = _width(ui, BROWSER_EMPTY_TITLE, th.flat.font_bold, 13, 18)
    var inner = min(max(max(caption, title), length(ui, 32)), room)
    ui.open(Style.col().width(inner + 2 * pad).pad(pad, pad).spacing(length(ui, 10)).align(CENTER, START))
    var chip = th.paint("badge.icon")
    ui.open(chip.style(Style.row().width(length(ui, 32)).height(length(ui, 32)).center()))
    chip.decorations(ui)
    _icon(ui, th, "globe", 12, muted)
    ui.close()
    _label(ui, BROWSER_EMPTY_TITLE, th.flat.font_bold, 13, 18, fg)
    ui.open(Style.col().width(inner))
    var text = BROWSER_EMPTY_CAPTION
    ui.text_opts(text.unsafe_ptr(), text.byte_length(), TextStyle(th.flat.font, 11, _packed(muted), True, muted), TextOpts().line_height(16).align(TEXT_CENTER))
    ui.close()
    ui.close()


def browser_recent_rows(mut ui: Ui, th: PennaTheme, key: UInt64, urls: List[String], available: Float32) raises -> Int:
    var chosen = -1
    var fg = root_color(th.theme, 1)
    var muted = _muted(th)
    var width = min(length(ui, 360), available)
    var lead = length(ui, 12)
    var gap = length(ui, 8)
    var words = max(width - 2 * lead - length(ui, 14) - gap, Float32(0))
    ui.open(Style.col().width(width).spacing(length(ui, 4)))
    for index in range(len(urls)):
        var row = sub(key, index)
        var response = ui.interact(row)
        if response.hovered or response.held:
            ui.cursor = CURSOR_HAND
        var state = "active" if response.held else "hover" if response.hovered else "rest"
        var paint = appearance(th.theme, recipe("menu-item.tint"), context_state(state), 0, th.flat.penna_reduced)
        ui.open(paint.style(Style.row().width(width).height(length(ui, 32)).pad(lead, 0).spacing(gap).align(START, CENTER).id(row)))
        paint.decorations(ui)
        ui.sem(ROLE_LINK, urls[index])
        _icon(ui, th, "globe", 14, muted)
        var value = native_end_truncate(ui, urls[index], th.flat.font, 12, words)
        _label(ui, value, th.flat.font, 12, 16, fg)
        ui.close()
        if response.pressed:
            chosen = index
    ui.close()
    return chosen


def browser_error_panel(mut ui: Ui, th: PennaTheme, key: UInt64, message: StringSpan, retry: Bool, available: Float32) raises -> Bool:
    var clicked = False
    var fg = root_color(th.theme, 1)
    var muted = _muted(th)
    var paint = th.paint("inline-panel")
    var pad = length(ui, 12)
    var gap = length(ui, 8)
    var mark = length(ui, 14)
    var width = min(length(ui, 360), available)
    var body = max(width - 2 * pad, Float32(0))
    var words = max(body - mark - gap, Float32(0))
    ui.open(paint.style(Style.col().width(width).pad(pad, pad).spacing(gap)))
    paint.decorations(ui)
    ui.sem(ROLE_ALERT, BROWSER_ERROR_TITLE)
    ui.open(Style.row().width(body).spacing(gap).align(START, START))
    var danger = th.paint("label", tone="danger").color(1)
    ui.open(Style.col().width(mark).height(length(ui, 18)))
    ui.open(Style.col().width(mark).height(mark).at(0, length(ui, 2)))
    named_icon(ui, th.flat, "triangle-alert", mark, _packed(danger), danger, physical_layout=True)
    ui.close()
    ui.close()
    ui.open(Style.col().width(words).spacing(length(ui, 4)))
    _label(ui, BROWSER_ERROR_TITLE, th.flat.font, 13, 18, fg)
    ui.text_opts(message.unsafe_ptr(), message.byte_length(), TextStyle(th.flat.font, 12, _packed(muted), True, muted), TextOpts().line_height(16))
    ui.close()
    ui.close()
    if retry:
        var button_key = sub(key, 1)
        var response = ui.interact(button_key, focusable=True, focus_on_press=False, activate_on_release=True)
        clicked = response.clicked or ui.nav(button_key).activate()
        if response.hovered or response.held:
            ui.cursor = CURSOR_HAND
        var state = "active" if response.held else "hover" if response.hovered else "rest"
        var button = appearance(th.theme, recipe("button", prominent=True), context_state(state), ground("inline-panel"), th.flat.penna_reduced)
        var label = _width(ui, BROWSER_RETRY_LABEL, th.flat.font, 13, 18)
        ui.open(Style.row().width(body).height(length(ui, 28)).align(END, CENTER))
        ui.open(button.style(Style.row().width(label + 2 * pad).height(length(ui, 28)).center().id(button_key)))
        button.decorations(ui, ui.focus_ring(button_key))
        ui.sem(ROLE_BUTTON, BROWSER_RETRY_LABEL)
        _label(ui, BROWSER_RETRY_LABEL, th.flat.font, 13, 18, button.color(1))
        ui.close()
        ui.close()
    ui.close()
    return clicked


def browser_pick_status(mut ui: Ui, th: PennaTheme, text: StringSpan, width: Float32, height: Float32) raises:
    var paint = th.paint("chip.floating")
    var side = length(ui, 16)
    var tall = length(ui, 28)
    var lead = length(ui, 10)
    var gap = length(ui, 6)
    var mark = length(ui, 12)
    var room = max(width - 2 * side, Float32(0))
    var words = _width(ui, text, th.flat.font, 12, 16)
    var chip = min(2 * lead + mark + gap + words, room)
    ui.open(Style.row().width(room).height(tall).align(CENTER, CENTER).at(side, height - length(ui, 14) - tall))
    ui.open(paint.style(Style.row().width(chip).height(tall).pad(lead, 0).spacing(gap).align(START, CENTER).clipped()))
    paint.decorations(ui)
    ui.sem(ROLE_STATUS, text)
    _icon(ui, th, "inspector", 12, paint.color(1), 0.5)
    _label(ui, text, th.flat.font, 12, 16, paint.color(1))
    ui.close()
    ui.close()
