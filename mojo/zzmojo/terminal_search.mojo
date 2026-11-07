from .input_protocol import KeyInput
from .terminal_search_protocol import (
    SearchQuery,
    search_request,
    SEARCH_BEGIN,
    SEARCH_UPDATE,
    SEARCH_NEXT,
    SEARCH_PREVIOUS,
    SEARCH_CLOSE,
)

comptime SEARCH_NAVIGATE: UInt8 = 0
comptime SEARCH_ACCEPT_AND_CLOSE: UInt8 = 1
comptime SEARCH_FOCUS: UInt8 = 1
comptime TERMINAL_FOCUS: UInt8 = 2


def search_key_action(mut query: SearchQuery, input: KeyInput) -> Bool:
    if input.key != 0 or input.modifiers & 4 == 0:
        return False
    if input.codepoint == 114 or input.codepoint == 82:
        query.mode = UInt8(1) if query.mode == 0 else UInt8(0)
        return True
    if input.codepoint == 99 or input.codepoint == 67:
        query.case_policy = (
            query.case_policy + 1 if query.case_policy < 2 else UInt8(0)
        )
        return True
    return False


def search_shortcut(
    input: KeyInput, claims_prefix: Bool = False, modal_capture: Bool = False
) -> Bool:
    if claims_prefix or modal_capture:
        return False
    var codepoint = input.codepoint
    if input.key != 0:
        if input.key != 16 or not input.text:
            return False
        var count = 0
        for character in input.text.value().codepoints():
            codepoint = UInt32(character)
            count += 1
        if count != 1:
            return False
    if input.modifiers == 3:
        if codepoint >= 65 and codepoint <= 90:
            codepoint += 32
    elif input.modifiers != 8 and input.modifiers != 9:
        return False
    return codepoint == 102


def search_case_label(case_policy: UInt8) raises -> String:
    if case_policy == 0:
        return "Smart case"
    if case_policy == 1:
        return "Match case"
    if case_policy == 2:
        return "Ignore case"
    raise Error("unknown search case policy")


struct SearchEffect(Copyable, Movable):
    var frame: List[UInt8]
    var focus: UInt8
    var cancel_scroll: Bool
    var clear_marked: Bool
    var redraw: Bool

    def __init__(out self):
        self.frame = List[UInt8]()
        self.focus = 0
        self.cancel_scroll = False
        self.clear_marked = False
        self.redraw = False


struct SearchSession(Copyable, Movable):
    var query: Optional[SearchQuery]
    var behavior: UInt8
    var swallowed: Optional[UInt8]

    def __init__(out self):
        self.query = None
        self.behavior = SEARCH_NAVIGATE
        self.swallowed = None

    def begin(
        mut self,
        pane: UInt64,
        direction: UInt8 = 0,
        behavior: UInt8 = SEARCH_NAVIGATE,
        command_output: Bool = False,
    ) raises -> SearchEffect:
        if direction > 1 or behavior > 1:
            raise Error("unknown search prompt behavior")
        var query = SearchQuery(direction=direction)
        var effect = SearchEffect()
        effect.frame = search_request(
            pane, SEARCH_BEGIN, query.copy(), command_output
        )
        effect.focus = SEARCH_FOCUS
        effect.cancel_scroll = True
        effect.clear_marked = True
        effect.redraw = True
        self.query = query^
        self.behavior = behavior
        return effect^

    def close(
        mut self, pane: UInt64, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        effect.frame = search_request(
            pane, SEARCH_CLOSE, command_output=command_output
        )
        effect.focus = TERMINAL_FOCUS
        effect.clear_marked = True
        effect.redraw = True
        self.query = None
        return effect^

    def edit(
        mut self, pane: UInt64, text: String, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        if self.query:
            self.query.value().text = text
            effect.frame = search_request(
                pane, SEARCH_UPDATE, self.query.value().copy(), command_output
            )
            effect.cancel_scroll = True
            effect.clear_marked = True
            effect.redraw = True
        return effect^

    def append(
        mut self, pane: UInt64, text: String, command_output: Bool = False
    ) raises -> SearchEffect:
        if not self.query or text.byte_length() == 0:
            return SearchEffect()
        return self.edit(pane, self.query.value().text + text, command_output)

    def toggle_regex(
        mut self, pane: UInt64, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        if self.query:
            self.query.value().mode = UInt8(
                1
            ) if self.query.value().mode == 0 else UInt8(0)
            effect.frame = search_request(
                pane, SEARCH_UPDATE, self.query.value().copy(), command_output
            )
            effect.redraw = True
        return effect^

    def toggle_case(
        mut self, pane: UInt64, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        if self.query:
            self.query.value().case_policy = UInt8(
                2
            ) if self.query.value().case_policy == 1 else UInt8(1)
            effect.frame = search_request(
                pane, SEARCH_UPDATE, self.query.value().copy(), command_output
            )
            effect.redraw = True
        return effect^

    def key_action(
        mut self, pane: UInt64, input: KeyInput, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        if self.query and search_key_action(self.query.value(), input):
            effect.frame = search_request(
                pane, SEARCH_UPDATE, self.query.value().copy(), command_output
            )
            effect.redraw = True
        return effect^

    def step(
        self, pane: UInt64, backward: Bool, command_output: Bool = False
    ) raises -> SearchEffect:
        var effect = SearchEffect()
        effect.frame = search_request(
            pane,
            SEARCH_PREVIOUS if backward else SEARCH_NEXT,
            command_output=command_output,
        )
        effect.cancel_scroll = True
        return effect^

    def enter(
        mut self, pane: UInt64, shift: Bool, command_output: Bool = False
    ) raises -> SearchEffect:
        if not self.query:
            return SearchEffect()
        if self.behavior == SEARCH_ACCEPT_AND_CLOSE:
            self.query = None
            self.swallowed = UInt8(2)
            var effect = SearchEffect()
            effect.focus = TERMINAL_FOCUS
            effect.redraw = True
            return effect^
        var backward = (self.query.value().direction == 1) != shift
        return self.step(pane, backward, command_output)

    def escape(
        mut self, pane: UInt64, command_output: Bool = False
    ) raises -> SearchEffect:
        self.swallowed = UInt8(4)
        return self.close(pane, command_output)

    def swallow_key(mut self, key: UInt8, release: Bool) -> Bool:
        if not self.swallowed or self.swallowed.value() != key:
            return False
        if release:
            self.swallowed = None
        return True
