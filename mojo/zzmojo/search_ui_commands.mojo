from .terminal_search_protocol import SearchUiCommand, decode_search_ui

comptime SEARCH_UI_COMMAND_LIMIT: Int = 512


@fieldwise_init
struct CapturedSearchCommand(TrivialRegisterPassable):
    var server: UInt64
    var client: UInt64
    var pane: UInt64
    var direction: UInt8


struct SearchUiCommands(Copyable, Movable):
    var events: List[CapturedSearchCommand]

    def __init__(out self):
        self.events = List[CapturedSearchCommand]()

    def consume(mut self, var frame: List[UInt8], server: UInt64, client: UInt64) raises -> Bool:
        var command = decode_search_ui(frame^)
        if not command:
            return False
        self.push(command.value(), server, client)
        return True

    def push(mut self, command: SearchUiCommand, server: UInt64, client: UInt64) raises:
        if len(self.events) >= SEARCH_UI_COMMAND_LIMIT:
            raise Error("terminal search UI command limit reached")
        self.events.append(CapturedSearchCommand(server, client, command.pane, command.direction))

    def take(mut self) -> List[CapturedSearchCommand]:
        var result = self.events^
        self.events = List[CapturedSearchCommand]()
        return result^

    def merge(mut self, var events: List[CapturedSearchCommand]) raises:
        if len(self.events) + len(events) > SEARCH_UI_COMMAND_LIMIT:
            raise Error("terminal search UI command limit reached")
        self.events.extend(events^)
