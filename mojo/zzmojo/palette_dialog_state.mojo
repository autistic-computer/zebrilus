from std.memory import ArcPointer
from .prompt_protocol import PromptState, PromptEvent
from .chooser_protocol import ChooseItem, ChooseState, ChooseSearch, ChooseEvent


@fieldwise_init
struct ChooseItems(Movable):
    var values: List[ChooseItem]


struct ChooseSnapshot(Copyable, Movable):
    var items: ArcPointer[ChooseItems]
    var search: Optional[ChooseSearch]
    var selected: UInt32
    var kind: UInt8
    var filter_no_matches: Bool
    var prompt: String
    var help: Bool

    def __init__(out self, var state: ChooseState):
        var items = List[ChooseItem]()
        swap(items, state.items)
        self.items = ArcPointer(ChooseItems(items^))
        self.search = None
        swap(self.search, state.search)
        self.selected = state.selected
        self.kind = state.kind
        self.filter_no_matches = state.filter_no_matches
        self.prompt = String()
        swap(self.prompt, state.prompt)
        self.help = state.help


struct PaletteDialogs(Copyable, Movable):
    var prompt: Optional[ArcPointer[PromptState]]
    var chooser: Optional[ChooseSnapshot]
    var prompt_revision: UInt64
    var chooser_revision: UInt64

    def __init__(out self):
        self.prompt = None
        self.chooser = None
        self.prompt_revision = 0
        self.chooser_revision = 0

    def apply_prompt(mut self, var event: PromptEvent):
        self.prompt = ArcPointer(event.state.take()) if event.state else Optional[ArcPointer[PromptState]](None)
        self.prompt_revision += 1

    def apply_chooser(mut self, var event: ChooseEvent):
        if event.update:
            if self.chooser:
                ref state = self.chooser.value()
                swap(state.search, event.search)
                state.selected = min(event.selected, UInt32(max(len(state.items[].values), 1) - 1))
                self.chooser_revision += 1
            return
        self.chooser = ChooseSnapshot(event.state.take()) if event.state else Optional[ChooseSnapshot](None)
        self.chooser_revision += 1
