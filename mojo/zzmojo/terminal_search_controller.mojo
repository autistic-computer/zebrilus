from mjui.ui import TextStyle
from .input_protocol import KeyInput
from .terminal_search import SearchEffect, search_shortcut, SEARCH_NAVIGATE
from .terminal_search_editor import SearchEditor
from .terminal_search_chrome import SearchKeys

comptime SEARCH_POOL_BYTES: Int = 8_388_608
comptime SEARCH_POOL_ENTRIES: Int = 512


@fieldwise_init
struct SearchTarget(TrivialRegisterPassable):
    var pane: UInt64
    var command_output: Bool
    var owner: UInt64

    def name(self) -> String:
        return String(self.command_output, ":", self.owner, ":", self.pane)


@fieldwise_init
struct TargetSearchEffect(Copyable, Movable):
    var target: SearchTarget
    var effect: SearchEffect
    var passive: Bool


struct SearchKeyResult(Copyable, Movable):
    var handled: Bool
    var effect: SearchEffect

    def __init__(out self):
        self.handled = False
        self.effect = SearchEffect()


struct TerminalSearchController(Movable):
    var editors: Dict[String, SearchEditor]
    var targets: Dict[String, SearchTarget]
    var keys: Dict[String, SearchKeys]
    var last_used: Dict[String, UInt64]
    var server: UInt64
    var client: UInt64
    var clock: UInt64
    var reserved_entries: Int

    def __init__(out self):
        self.editors = Dict[String, SearchEditor]()
        self.targets = Dict[String, SearchTarget]()
        self.keys = Dict[String, SearchKeys]()
        self.last_used = Dict[String, UInt64]()
        self.server = 0
        self.client = 0
        self.clock = 0
        self.reserved_entries = 0

    def touch(mut self, name: String):
        if self.clock == UInt64.MAX:
            self.clock = 0
            for key in self.last_used.keys():
                self.last_used[key] = 0
        self.clock += 1
        self.last_used[name] = self.clock

    def ensure(mut self, target: SearchTarget, keys: SearchKeys, style: TextStyle) raises:
        var name = target.name()
        if name not in self.editors:
            if len(self.editors) >= SEARCH_POOL_ENTRIES:
                raise Error("retained terminal search limit reached")
            self.editors[name] = SearchEditor(keys.field, style)
            self.targets[name] = target
            self.keys[name] = keys
            self.reserved_entries = max(self.reserved_entries, len(self.editors))
        self.touch(name)

    def begin(
        mut self,
        target: SearchTarget,
        direction: UInt8 = 0,
        behavior: UInt8 = SEARCH_NAVIGATE,
    ) raises -> SearchEffect:
        var name = target.name()
        if name not in self.editors:
            raise Error("terminal search editor has not been prepared")
        var effect = self.editors[name].session.begin(target.pane, direction, behavior, target.command_output)
        self.editors[name].reset_text()
        self.touch(name)
        return effect^

    def route_key(
        mut self,
        target: SearchTarget,
        input: KeyInput,
        search_focused: Bool,
        terminal_focused: Bool,
        claims_prefix: Bool,
        modal_capture: Bool,
    ) raises -> SearchKeyResult:
        var result = SearchKeyResult()
        var name = target.name()
        if name not in self.editors:
            return result^
        ref editor = self.editors[name]
        if editor.session.swallow_key(input.key, input.action == 2):
            result.handled = True
            return result^
        if input.action == 2:
            return result^
        if search_focused and editor.session.query:
            result.effect = editor.session.key_action(target.pane, input, target.command_output)
            result.handled = len(result.effect.frame) > 0
        elif terminal_focused and search_shortcut(input, claims_prefix, modal_capture):
            result.effect = self.begin(target)
            result.handled = True
        if result.handled:
            self.touch(name)
        return result^

    def remove(mut self, name: String) raises:
        _ = self.editors.pop(name)
        _ = self.targets.pop(name)
        _ = self.keys.pop(name)
        _ = self.last_used.pop(name)

    def target_for_field(self, field: UInt64) raises -> Optional[SearchTarget]:
        for name in self.keys.keys():
            if self.keys[name].field == field:
                return self.targets[name]
        return None

    def owns_key(self, key: UInt64) raises -> Bool:
        if key == 0:
            return False
        for name in self.keys.keys():
            if not self.editors[name].session.query:
                continue
            var keys = self.keys[name]
            if key == keys.panel or key == keys.field or key == keys.container or key == keys.regex or key == keys.case_policy or key == keys.previous or key == keys.next or key == keys.close:
                return True
        return False

    def observe(mut self, server: UInt64, client: UInt64, valid: Span[SearchTarget, _]) raises:
        if self.server != server or self.client != client:
            self.editors = Dict[String, SearchEditor]()
            self.targets = Dict[String, SearchTarget]()
            self.keys = Dict[String, SearchKeys]()
            self.last_used = Dict[String, UInt64]()
            self.server = server
            self.client = client
            self.clock = 0
            self.reserved_entries = 0
            return
        var live = Dict[String, Bool]()
        for target in valid:
            live[target.name()] = True
        var removed = List[String]()
        for name in self.last_used.keys():
            if name not in live:
                removed.append(name.copy())
        for name in removed:
            self.remove(name)

    def owned_bytes(self) raises -> Int:
        var bytes = self.reserved_entries * 4096
        for name in self.last_used.keys():
            bytes += self.editors[name].owned_bytes()
        return bytes

    def enforce(mut self, protected: Optional[String] = None) raises -> List[TargetSearchEffect]:
        var effects = List[TargetSearchEffect]()
        if self.owned_bytes() <= SEARCH_POOL_BYTES and len(self.editors) <= SEARCH_POOL_ENTRIES:
            return effects^
        for name in self.last_used.keys():
            self.editors[name].discard_undo()
        while self.owned_bytes() > SEARCH_POOL_BYTES or len(self.editors) > SEARCH_POOL_ENTRIES:
            var oldest: Optional[String] = None
            var stamp = UInt64.MAX
            for name in self.last_used.keys():
                if protected and name == protected.value():
                    continue
                if self.last_used[name] <= stamp:
                    oldest = name.copy()
                    stamp = self.last_used[name]
            if not oldest:
                break
            var name = oldest.value()
            if self.editors[name].session.query:
                var target = self.targets[name]
                var effect = self.editors[name].session.close(target.pane, target.command_output)
                effect.focus = 0
                effects.append(TargetSearchEffect(target, effect^, True))
            self.remove(name)
        return effects^
