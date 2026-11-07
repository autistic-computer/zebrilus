from std.memory import ArcPointer
from .prompt_protocol import PromptState, PromptEvent
from .chooser_protocol import ChooseItem, ChooseState, ChooseSearch, ChooseEvent
from .chooser_buffer_protocol import (
    ChooseBufferItem,
    ChooseBufferState,
    ChooseBufferEvent,
)
from .chooser_plane_protocol import ChooserPlane, ChooserPlaneEvent


def next_chooser_revision(revision: UInt64) -> UInt64:
    return 1 if revision == UInt64.MAX else revision + 1


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


@fieldwise_init
struct ChooseBufferItems(Movable):
    var values: List[ChooseBufferItem]


struct ChooseBufferSnapshot(Copyable, Movable):
    var items: ArcPointer[ChooseBufferItems]
    var search: Optional[ChooseSearch]
    var selected: UInt32
    var filter_no_matches: Bool
    var help: Bool
    var prompt: String

    def __init__(out self, var state: ChooseBufferState):
        var items = List[ChooseBufferItem]()
        swap(items, state.items)
        self.items = ArcPointer(ChooseBufferItems(items^))
        self.search = None
        swap(self.search, state.search)
        self.selected = state.selected
        self.filter_no_matches = state.filter_no_matches
        self.help = state.help
        self.prompt = String()
        swap(self.prompt, state.prompt)


struct PaletteDialogs(Copyable, Movable):
    var prompt: Optional[ArcPointer[PromptState]]
    var chooser: Optional[ChooseSnapshot]
    var buffer: Optional[ChooseBufferSnapshot]
    var chooser_plane: Optional[ArcPointer[ChooserPlane]]
    var prompt_revision: UInt64
    var chooser_revision: UInt64
    var chooser_closed_revision: UInt64
    var buffer_revision: UInt64

    def __init__(out self):
        self.prompt = None
        self.chooser = None
        self.buffer = None
        self.chooser_plane = None
        self.prompt_revision = 0
        self.chooser_revision = 0
        self.chooser_closed_revision = 0
        self.buffer_revision = 0

    def apply_prompt(mut self, var event: PromptEvent):
        self.prompt = ArcPointer(
            event.state.take()
        ) if event.state else Optional[ArcPointer[PromptState]](None)
        self.prompt_revision = next_chooser_revision(self.prompt_revision)

    def apply_chooser(mut self, var event: ChooseEvent):
        if event.update:
            if self.chooser:
                ref state = self.chooser.value()
                swap(state.search, event.search)
                state.selected = min(
                    event.selected,
                    UInt32(max(len(state.items[].values), 1) - 1),
                )
                self.chooser_revision = next_chooser_revision(
                    self.chooser_revision
                )
            return
        self.chooser = ChooseSnapshot(
            event.state.take()
        ) if event.state else Optional[ChooseSnapshot](None)
        self.chooser_plane = None
        self.chooser_revision = next_chooser_revision(self.chooser_revision)
        if not self.chooser:
            self.chooser_closed_revision = self.chooser_revision

    def apply_buffer(mut self, var event: ChooseBufferEvent):
        if event.update:
            if self.buffer:
                ref state = self.buffer.value()
                swap(state.search, event.search)
                state.selected = min(
                    event.selected,
                    UInt32(max(len(state.items[].values), 1) - 1),
                )
                self.buffer_revision = next_chooser_revision(
                    self.buffer_revision
                )
            return
        self.buffer = ChooseBufferSnapshot(
            event.state.take()
        ) if event.state else Optional[ChooseBufferSnapshot](None)
        self.chooser_plane = None
        self.buffer_revision = next_chooser_revision(self.buffer_revision)

    def apply_plane(mut self, var event: ChooserPlaneEvent):
        self.chooser_plane = ArcPointer(
            event.plane.take()
        ) if event.plane else Optional[ArcPointer[ChooserPlane]](None)
        if self.chooser:
            self.chooser_revision = next_chooser_revision(self.chooser_revision)
        elif self.buffer:
            self.buffer_revision = next_chooser_revision(self.buffer_revision)

    def reset(mut self):
        self.prompt = None
        self.chooser = None
        self.buffer = None
        self.chooser_plane = None
        self.prompt_revision = next_chooser_revision(self.prompt_revision)
        self.chooser_revision = next_chooser_revision(self.chooser_revision)
        self.chooser_closed_revision = self.chooser_revision
        self.buffer_revision = next_chooser_revision(self.buffer_revision)

    def finish_group(mut self, revision: UInt64, closed: UInt64):
        if self.chooser_revision != revision:
            self.chooser_closed_revision = (
                closed if self.chooser else self.chooser_revision
            )
