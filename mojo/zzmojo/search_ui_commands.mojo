from .terminal_search_protocol import SearchUiCommand, decode_search_ui

comptime SEARCH_UI_COMMAND_LIMIT: Int = 512


@fieldwise_init
struct CapturedSearchCommand(Copyable, Movable):
    var server: UInt64
    var client: UInt64
    var pane: UInt64
    var direction: UInt8
    var output_owner: Optional[UInt64]


struct SearchUiCommands(Copyable, Movable):
    var events: List[CapturedSearchCommand]

    def __init__(out self):
        self.events = List[CapturedSearchCommand]()

    def consume(mut self, var frame: List[UInt8], server: UInt64, client: UInt64, output_pane: Optional[UInt64] = None, output_owner: Optional[UInt64] = None) raises -> Bool:
        var command = decode_search_ui(frame^)
        if not command:
            return False
        self.push(command.value(), server, client, output_owner if output_pane == Optional[UInt64](command.value().pane) else Optional[UInt64](None))
        return True

    def push(mut self, command: SearchUiCommand, server: UInt64, client: UInt64, output_owner: Optional[UInt64] = None) raises:
        if len(self.events) >= SEARCH_UI_COMMAND_LIMIT:
            raise Error("terminal search UI command limit reached")
        self.events.append(CapturedSearchCommand(server, client, command.pane, command.direction, output_owner))

    def take(mut self) -> List[CapturedSearchCommand]:
        var result = self.events^
        self.events = List[CapturedSearchCommand]()
        return result^

    def merge(mut self, var events: List[CapturedSearchCommand]) raises:
        if len(self.events) + len(events) > SEARCH_UI_COMMAND_LIMIT:
            raise Error("terminal search UI command limit reached")
        self.events.extend(events^)
