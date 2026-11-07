from mjui.input import (
    KeyEvent,
    TextEvent,
    TextInput,
    TEXT_INSERT,
    TEXT_COMMAND,
    CMD_COPY,
    CMD_PASTE,
    CMD_SELECT_ALL,
    MOD_CTRL,
    MOD_CMD,
)
from mjui.keys import Key
from .state import AppModel
from .input_protocol import KeyInput, mjui_key_input, should_send_key


@fieldwise_init
struct RoutedKey(Copyable, Movable):
    var pane: UInt64
    var epoch: UInt64
    var input: KeyInput
    var release: Bool


@fieldwise_init
struct _Owner(TrivialRegisterPassable):
    var pane: UInt64
    var epoch: UInt64


def _pane_exists(model: AppModel, pane: UInt64) -> Bool:
    if pane in model.terminals:
        return True
    if (
        model.command_output.pane == Optional[UInt64](pane)
        and model.command_output.viewport
    ):
        return True
    for record in model.panes:
        if record.id == pane:
            return True
    for session in model.snapshot.sessions:
        for window in session.windows:
            if pane in window.panes:
                return True
    return False


def _kitty(model: AppModel, pane: UInt64) raises -> Bool:
    if (
        model.command_output.pane == Optional[UInt64](pane)
        and model.command_output.viewport
    ):
        return model.command_output.viewport.value().metadata.kitty_keyboard
    if pane in model.terminals:
        return model.terminals[pane].metadata.kitty_keyboard
    return False


struct KeyboardController(Movable):
    var claimed: Bool
    var forwarded: Dict[UInt16, _Owner]
    var consumed: List[Tuple[Int32, Int32]]
    var source_server: UInt64
    var source_client: UInt64
    var command_claims: Dict[String, Int]
    var command_positions: Dict[Int32, String]
    var consumed_commands: Dict[Int32, Bool]

    def __init__(out self):
        self.claimed = False
        self.forwarded = Dict[UInt16, _Owner]()
        self.consumed = List[Tuple[Int32, Int32]]()
        self.source_server = 0
        self.source_client = 0
        self.command_claims = Dict[String, Int]()
        self.command_positions = Dict[Int32, String]()
        self.consumed_commands = Dict[Int32, Bool]()

    def reset(mut self):
        self.claimed = False
        self.forwarded.clear()

    def remove_pane(mut self, pane: UInt64) raises:
        var removed = List[UInt16]()
        for entry in self.forwarded.items():
            if entry.value.pane == pane:
                removed.append(entry.key)
        for key in removed:
            _ = self.forwarded.pop(key)

    def claim_text(mut self, key: KeyEvent, bytes: Span[UInt8, _]) raises:
        self.claimed = True
        self.claim_command(key)
        if (
            key.text_lo >= 0
            and key.text_hi > key.text_lo
            and Int(key.text_hi) <= len(bytes)
        ):
            _ = String(from_utf8=bytes[Int(key.text_lo) : Int(key.text_hi)])
            self.consumed.append((key.text_lo, key.text_hi))

    def claim_command(mut self, key: KeyEvent) raises:
        if key.release or key.mods & (MOD_CTRL | MOD_CMD) == 0:
            return
        var command: UInt8 = 0
        if key.key == Key.C:
            command = CMD_COPY
        elif key.key == Key.V:
            command = CMD_PASTE
        elif key.key == Key.A:
            command = CMD_SELECT_ALL
        if command != 0:
            var name = String(command, ":", key.mods)
            if key.text_event >= 0:
                if key.text:
                    self.command_positions[key.text_event] = name
            else:
                self.command_claims[name] = self.command_claims.get(name, 0) + 1

    def claim_text_command(mut self, key: KeyEvent):
        if not key.release and key.text and key.text_event >= 0:
            self.consumed_commands[key.text_event] = True

    def route(
        mut self,
        model: AppModel,
        focused: Bool,
        key: KeyEvent,
        bytes: Span[UInt8, _],
        epoch: UInt64 = 0,
        target: Optional[UInt64] = None,
    ) raises -> Optional[RoutedKey]:
        self.claimed = False
        if (
            model.source_server != self.source_server
            or model.source_client != self.source_client
        ):
            self.reset()
            self.source_server = model.source_server
            self.source_client = model.source_client
        if not model.session or not model.pane:
            self.reset()
            return None
        if key.release:
            if key.key not in self.forwarded:
                return None
            var owner = self.forwarded.pop(key.key)
            if not _pane_exists(model, owner.pane):
                return None
            var input = mjui_key_input(
                key.key, key.ch, key.unshifted, key.mods, key.action()
            )
            self.claimed = True
            return RoutedKey(owner.pane, owner.epoch, input^, True)
        if not focused or key.consumed:
            return None
        var pane = target.value() if target else model.pane.value()
        var captured_epoch = epoch
        if key.repeat and key.key in self.forwarded:
            var owner = self.forwarded[key.key]
            pane = owner.pane
            captured_epoch = owner.epoch
        if not _pane_exists(model, pane):
            return None
        var text: Optional[String] = None
        if (
            key.text_lo >= 0
            and key.text_hi > key.text_lo
            and Int(key.text_hi) <= len(bytes)
        ):
            text = String(from_utf8=bytes[Int(key.text_lo) : Int(key.text_hi)])
        var input = mjui_key_input(
            key.key, key.ch, key.unshifted, key.mods, key.action(), text
        )
        var kitty = _kitty(model, pane)
        var output = model.command_output.pane == Optional[UInt64](
            pane
        ) and Bool(model.command_output.viewport)
        var prefix = not output and model.routing.claims_prefix_input(input)
        if not should_send_key(input, kitty, prefix):
            return None
        if prefix and key.repeat and model.routing.armed:
            if text:
                self.consumed.append((key.text_lo, key.text_hi))
            self.claimed = True
            self.claim_command(key)
            return None
        if (
            kitty
            and input.key == 0
            and input.modifiers & 14 == 0
            and not text
            and not prefix
        ):
            return None
        if not key.repeat or key.key not in self.forwarded:
            self.forwarded[key.key] = _Owner(pane, captured_epoch)
        if text:
            self.consumed.append((key.text_lo, key.text_hi))
        self.claimed = True
        self.claim_command(key)
        return RoutedKey(pane, captured_epoch, input^, False)

    def filter_text(
        mut self, mut input: TextInput, stamped_commands: Bool = False
    ):
        var events = List[TextEvent]()
        for index in range(len(input.events)):
            var event = input.events[index]
            if event.kind == TEXT_COMMAND:
                var name = String(event.cmd, ":", event.mods)
                var position = event.start if stamped_commands else Int32(index)
                if self.consumed_commands.get(position, False):
                    continue
                if self.command_positions.get(position, String()) == name:
                    continue
                var claimed = self.command_claims.get(name, 0)
                if claimed > 0:
                    self.command_claims[name] = claimed - 1
                    continue
            if event.kind != TEXT_INSERT:
                events.append(event)
                continue
            var segments: List[Tuple[Int32, Int32]] = [(event.start, event.end)]
            for consumed in self.consumed:
                var remaining = List[Tuple[Int32, Int32]]()
                for segment in segments:
                    if consumed[1] <= segment[0] or consumed[0] >= segment[1]:
                        remaining.append(segment)
                    else:
                        if segment[0] < consumed[0]:
                            remaining.append((segment[0], consumed[0]))
                        if segment[1] > consumed[1]:
                            remaining.append((consumed[1], segment[1]))
                segments = remaining^
            for segment in segments:
                events.append(
                    TextEvent(
                        event.kind,
                        event.cmd,
                        event.mods,
                        segment[0],
                        segment[1],
                    )
                )
        input.events = events^
        self.consumed.clear()
        self.command_claims.clear()
        self.command_positions.clear()
        self.consumed_commands.clear()
