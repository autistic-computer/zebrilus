from mjui.input import (
    MouseEvent,
    MOUSE_MOVED,
    MOUSE_DOWN,
    MOUSE_DRAGGED,
    MOUSE_UP,
    MOUSE_EXITED,
    MOUSE_RIGHT_DOWN,
    MOUSE_RIGHT_DRAGGED,
    MOUSE_RIGHT_UP,
    MOUSE_MIDDLE_DOWN,
    MOUSE_MIDDLE_UP,
    MOUSE_MIDDLE_DRAGGED,
    MOD_SHIFT,
    MOD_CTRL,
    MOD_CMD,
)
from mjui.scroll import ScrollEvent
from .state import AppModel
from .terminal_view import (
    PointerGrid,
    PointerCell,
    accumulate_scroll,
    selection_autoscroll_lines,
)
from .input_protocol import (
    mjui_mouse_input,
    mjui_scroll_input,
    input_mouse,
    input_scroll,
    input_view,
    input_selection,
)


struct PointerSurface(TrivialRegisterPassable):
    var pane: UInt64
    var grid: PointerGrid
    var command_output: Bool

    def __init__(
        out self, pane: UInt64, grid: PointerGrid, command_output: Bool = False
    ):
        self.pane = pane
        self.grid = grid
        self.command_output = command_output


@fieldwise_init
struct RoutedPointer(Copyable, Movable):
    var frame: List[UInt8]
    var pane: UInt64
    var epoch: UInt64
    var activate: Bool
    var passive: Bool


@fieldwise_init
struct _Capture(TrivialRegisterPassable):
    var surface: PointerSurface
    var epoch: UInt64
    var force_selection: Bool
    var selection: Bool
    var output_id: UInt64


def _viewport_exists(model: AppModel, surface: PointerSurface) -> Bool:
    if surface.command_output:
        return model.command_output.pane == Optional[UInt64](
            surface.pane
        ) and Bool(model.command_output.viewport)
    return surface.pane in model.terminals


def _tracking(model: AppModel, surface: PointerSurface) raises -> Bool:
    if surface.command_output:
        return model.command_output.viewport.value().metadata.mouse_tracking
    return model.terminals[surface.pane].metadata.mouse_tracking


def _hit(
    surfaces: Span[PointerSurface, _], x: Float32, y: Float32
) -> Optional[PointerSurface]:
    for index in range(len(surfaces) - 1, -1, -1):
        var surface = surfaces[index]
        if (
            surface.grid.columns > 0
            and surface.grid.rows > 0
            and surface.grid.contains(x, y)
        ):
            return surface
    return None


def _current(
    surfaces: Span[PointerSurface, _], previous: PointerSurface
) -> Optional[PointerSurface]:
    for surface in surfaces:
        if (
            surface.pane == previous.pane
            and surface.command_output == previous.command_output
        ):
            if surface.grid.columns > 0 and surface.grid.rows > 0:
                return surface
    return None


def _routed(
    surface: PointerSurface,
    var frame: List[UInt8],
    epoch: UInt64,
    activate: Bool = False,
    passive: Bool = False,
) -> RoutedPointer:
    return RoutedPointer(frame^, surface.pane, epoch, activate, passive)


struct PointerController(Movable):
    var claimed: Bool
    var captures: Dict[UInt8, _Capture]
    var hover: Optional[PointerSurface]
    var scroll_rows: Dict[UInt64, Float32]
    var output_rows: Float32
    var output_id: UInt64
    var selection_pointer: Optional[PointerCell]
    var selection_lines: Int32
    var source_server: UInt64
    var source_client: UInt64

    def __init__(out self):
        self.claimed = False
        self.captures = Dict[UInt8, _Capture]()
        self.hover = None
        self.scroll_rows = Dict[UInt64, Float32]()
        self.output_rows = 0
        self.output_id = 0
        self.selection_pointer = None
        self.selection_lines = 0
        self.source_server = 0
        self.source_client = 0

    def reset(mut self):
        self.claimed = False
        self.captures.clear()
        self.hover = None
        self.scroll_rows.clear()
        self.output_rows = 0
        self.selection_pointer = None
        self.selection_lines = 0

    def remove_pane(mut self, pane: UInt64) raises:
        var remove = List[UInt8]()
        for entry in self.captures.items():
            if entry.value.surface.pane == pane:
                remove.append(entry.key)
        for button in remove:
            _ = self.captures.pop(button)
        if pane in self.scroll_rows:
            _ = self.scroll_rows.pop(pane)
        if self.hover and self.hover.value().pane == pane:
            self.hover = None
        if UInt8(0) not in self.captures:
            self.selection_pointer = None
            self.selection_lines = 0

    def blur(
        mut self,
        model: AppModel,
        pane: UInt64,
        command_output: Bool = False,
        epoch: UInt64 = 0,
    ) raises -> Optional[RoutedPointer]:
        if not self._sync(model):
            return None
        var removed = List[UInt8]()
        for entry in self.captures.items():
            if (
                entry.key != 1
                and entry.value.surface.pane == pane
                and entry.value.surface.command_output == command_output
            ):
                removed.append(entry.key)
        for button in removed:
            _ = self.captures.pop(button)
        if UInt8(0) not in self.captures:
            self.selection_pointer = None
            self.selection_lines = 0
        if (
            self.hover
            and self.hover.value().pane == pane
            and self.hover.value().command_output == command_output
        ):
            self.hover = None
        if command_output:
            if (
                model.command_output.pane != Optional[UInt64](pane)
                or not model.command_output.viewport
            ):
                return None
        elif pane not in model.terminals:
            return None
        var frame = input_view(pane, 10, command_output=command_output)
        self.claimed = True
        return RoutedPointer(frame^, pane, epoch, False, True)

    def _sync(mut self, model: AppModel) raises -> Bool:
        self.claimed = False
        if (
            self.source_server != model.source_server
            or self.source_client != model.source_client
        ):
            self.reset()
            self.source_server = model.source_server
            self.source_client = model.source_client
        if not model.session or not model.pane:
            self.reset()
            return False
        var id = UInt64(0)
        if model.command_output.id:
            id = model.command_output.id.value()
        if self.output_id != id:
            self.output_id = id
            self.output_rows = 0
        var removed = List[UInt64]()
        for entry in self.scroll_rows.items():
            if entry.key not in model.terminals:
                removed.append(entry.key)
        for pane in removed:
            _ = self.scroll_rows.pop(pane)
        return True

    def _capture(
        mut self,
        model: AppModel,
        surfaces: Span[PointerSurface, _],
        button: UInt8,
    ) raises -> Optional[_Capture]:
        if button not in self.captures:
            return None
        var capture = self.captures[button]
        var surface = _current(surfaces, capture.surface)
        if (
            not surface
            or not _viewport_exists(model, capture.surface)
            or (
                capture.surface.command_output
                and capture.output_id != self.output_id
            )
        ):
            _ = self.captures.pop(button)
            return None
        capture.surface = surface.value()
        self.captures[button] = capture
        return capture

    def route(
        mut self,
        model: AppModel,
        surfaces: Span[PointerSurface, _],
        event: MouseEvent,
        epoch: UInt64 = 0,
    ) raises -> List[RoutedPointer]:
        var result = List[RoutedPointer](capacity=2)
        if not self._sync(model):
            return result^
        var hit: Optional[PointerSurface] = None
        if event.kind != MOUSE_EXITED:
            hit = _hit(surfaces, event.x, event.y)
        if self.hover and (
            not hit
            or self.hover.value().pane != hit.value().pane
            or self.hover.value().command_output != hit.value().command_output
        ):
            var previous = self.hover.value()
            self.hover = None
            if _viewport_exists(model, previous):
                result.append(
                    _routed(
                        previous,
                        input_view(
                            previous.pane,
                            10,
                            command_output=previous.command_output,
                        ),
                        epoch,
                        passive=True,
                    )
                )
        if event.kind != MOUSE_EXITED:
            var routed = self._route(model, surfaces, event, epoch)
            if routed:
                result.append(routed.value().copy())
        if self.hover and not self.hover.value().grid.contains(
            event.x, event.y
        ):
            self.hover = None
        self.claimed = self.claimed or len(result) > 0
        return result^

    def _route(
        mut self,
        model: AppModel,
        surfaces: Span[PointerSurface, _],
        event: MouseEvent,
        epoch: UInt64,
    ) raises -> Optional[RoutedPointer]:
        var press = event.kind == MOUSE_DOWN or event.kind == MOUSE_RIGHT_DOWN or event.kind == MOUSE_MIDDLE_DOWN
        var release = event.kind == MOUSE_UP or event.kind == MOUSE_RIGHT_UP or event.kind == MOUSE_MIDDLE_UP
        var drag = (
            event.kind == MOUSE_DRAGGED or event.kind == MOUSE_RIGHT_DRAGGED or event.kind == MOUSE_MIDDLE_DRAGGED
        )
        var button = UInt8(
            2 if event.kind == MOUSE_RIGHT_DOWN
            or event.kind == MOUSE_RIGHT_DRAGGED
            or event.kind == MOUSE_RIGHT_UP else 1 if event.kind == MOUSE_MIDDLE_DOWN or event.kind == MOUSE_MIDDLE_UP or event.kind == MOUSE_MIDDLE_DRAGGED else 0
        )
        var capture: Optional[_Capture] = None
        var surface: Optional[PointerSurface]
        if release or drag:
            capture = self._capture(model, surfaces, button)
            if not capture:
                return None
            surface = capture.value().surface
        else:
            surface = _hit(surfaces, event.x, event.y)
        if not surface or not _viewport_exists(model, surface.value()):
            return None
        var target = surface.value()
        if (
            drag
            and not capture.value().selection
            and not target.grid.contains(event.x, event.y)
        ):
            self.claimed = True
            return _routed(
                target,
                input_view(
                    target.pane, 10, command_output=target.command_output
                ),
                capture.value().epoch,
                passive=True,
            )
        if press:
            var forced = (
                button == 0
                and event.clicks >= 3
                and event.mods & (MOD_CTRL | MOD_CMD) != 0
            )
            var selecting = button == 0 and (
                not _tracking(model, target)
                or event.mods & MOD_SHIFT != 0
                or forced
            )
            capture = _Capture(target, epoch, forced, selecting, self.output_id)
            self.captures[button] = capture.value()
            if button == 0:
                self.selection_pointer = None
                self.selection_lines = 0
        elif event.kind == MOUSE_MOVED:
            var middle: Optional[_Capture] = None
            if UInt8(1) in self.captures:
                middle = self.captures[UInt8(1)]
            self.captures.clear()
            if middle:
                self.captures[UInt8(1)] = middle.value()
            self.selection_pointer = None
            self.selection_lines = 0
        elif not release and not drag:
            return None
        var owner_epoch = capture.value().epoch if capture else epoch
        var forced = capture.value().force_selection if capture else False
        var phase = UInt8(0 if press else 1 if release else 2)
        var choice: Optional[UInt8] = None
        if press or release or drag:
            choice = button
        var input = mjui_mouse_input(
            target.grid,
            event.x,
            event.y,
            phase,
            choice,
            Int(event.clicks) if press or release else 1,
            event.mods,
            forced,
        )
        if drag and capture.value().selection:
            self.selection_pointer = input.pointer
            self.selection_lines = Int32(
                selection_autoscroll_lines(
                    target.grid.top,
                    target.grid.top + target.grid.height,
                    target.grid.line_height,
                    event.y,
                )
            )
        if release:
            _ = self.captures.pop(button)
            if button == 0:
                self.selection_pointer = None
                self.selection_lines = 0
        self.hover = target
        self.claimed = True
        return _routed(
            target,
            input_mouse(target.pane, input, target.command_output),
            owner_epoch,
            activate=press and not target.command_output,
            passive=not press or target.command_output,
        )

    def scroll(
        mut self,
        model: AppModel,
        surfaces: Span[PointerSurface, _],
        event: ScrollEvent,
        x: Float32,
        y: Float32,
        mods: UInt32,
        epoch: UInt64 = 0,
    ) raises -> Optional[RoutedPointer]:
        if not self._sync(model):
            return None
        var surface = _hit(surfaces, x, y)
        if not surface or not _viewport_exists(model, surface.value()):
            return None
        var target = surface.value()
        var delta = (
            -event.dy / target.grid.line_height if event.precise else -event.dy
        )
        var remainder: Float32 = 0
        if target.command_output:
            remainder = self.output_rows
        elif target.pane in self.scroll_rows:
            remainder = self.scroll_rows[target.pane]
        var lines = Int32(accumulate_scroll(remainder, delta))
        if target.command_output:
            self.output_rows = remainder
        else:
            self.scroll_rows[target.pane] = remainder
        self.claimed = event.dy != 0
        if lines == 0:
            return None
        var input = mjui_scroll_input(target.grid, x, y, lines, mods)
        self.claimed = True
        return _routed(
            target,
            input_scroll(target.pane, lines, input, target.command_output),
            epoch,
            passive=True,
        )

    def modifiers(
        mut self,
        model: AppModel,
        surfaces: Span[PointerSurface, _],
        x: Float32,
        y: Float32,
        mods: UInt32,
        epoch: UInt64 = 0,
    ) raises -> Optional[RoutedPointer]:
        if not self._sync(model):
            return None
        var surface = _hit(surfaces, x, y)
        if not surface or not _viewport_exists(model, surface.value()):
            return None
        var target = surface.value()
        var forced = False
        var owner_epoch = epoch
        var capture = self._capture(model, surfaces, 0)
        if (
            capture
            and capture.value().surface.pane == target.pane
            and capture.value().surface.command_output == target.command_output
        ):
            forced = capture.value().force_selection
            owner_epoch = capture.value().epoch
        var input = mjui_mouse_input(
            target.grid, x, y, 2, None, 1, mods, forced
        )
        self.hover = target
        self.claimed = True
        return _routed(
            target,
            input_mouse(target.pane, input, target.command_output),
            owner_epoch,
            passive=True,
        )

    def autoscroll(mut self, model: AppModel) raises -> Optional[RoutedPointer]:
        if (
            not self._sync(model)
            or not self.selection_pointer
            or self.selection_lines == 0
            or UInt8(0) not in self.captures
        ):
            return None
        var capture = self.captures[UInt8(0)]
        if (
            not capture.selection
            or not _viewport_exists(model, capture.surface)
            or (
                capture.surface.command_output
                and capture.output_id != self.output_id
            )
        ):
            return None
        self.claimed = True
        return _routed(
            capture.surface,
            input_selection(
                capture.surface.pane,
                7,
                self.selection_pointer.value(),
                self.selection_lines,
                capture.surface.command_output,
            ),
            capture.epoch,
            passive=True,
        )
