from std.collections import Set
from .path_protocol import PathMessage, PATH_REQUEST, PATH_BEGIN, PATH_CHUNK, PATH_GIT, PATH_CANCEL, encode_path_message
from .path_listing import PathListing, PATH_RETAINED_BYTES


@fieldwise_init
struct PathSource(TrivialRegisterPassable):
    var server: UInt64
    var client: UInt64
    var epoch: UInt64

    def matches(self, other: Self) -> Bool:
        return self.server == other.server and self.client == other.client and self.epoch == other.epoch


@fieldwise_init
struct PathCommand(Copyable, Movable):
    var source: PathSource
    var request: UInt64
    var frame: List[UInt8]


@fieldwise_init
struct PathApplied(Copyable, Movable):
    var changed: Bool
    var cancel: Optional[PathCommand]


struct PathRegistry(Copyable, Movable):
    var source: PathSource
    var pending: Set[UInt64]

    def __init__(out self, source: PathSource):
        self.source = source
        self.pending = Set[UInt64]()

    def reset(mut self, source: PathSource):
        self.source = source
        self.pending.clear()

    def register(mut self, source: PathSource, request: UInt64) raises:
        if not self.source.matches(source):
            raise Error("path request belongs to a different connection")
        if request not in self.pending and len(self.pending) >= 64:
            raise Error("too many pending path requests")
        self.pending.add(request)

    def cancel(mut self, source: PathSource, request: UInt64) raises -> Bool:
        if not self.source.matches(source) or request not in self.pending:
            return False
        self.pending.remove(request)
        return True

    def route(mut self, source: PathSource, message: PathMessage) raises -> Bool:
        if not self.source.matches(source) or message.request not in self.pending:
            return False
        if message.kind != PATH_BEGIN and message.kind != PATH_CHUNK and message.kind != PATH_GIT:
            return False
        if (message.kind == PATH_BEGIN and Bool(message.error)) or (message.kind == PATH_CHUNK and message.done):
            self.pending.remove(message.request)
        return True


def _command(source: PathSource, request: UInt64, kind: UInt8) raises -> PathCommand:
    var message = PathMessage(kind, request)
    return PathCommand(source, request, encode_path_message(message))


struct PathService(Movable):
    var source: PathSource
    var next_request: UInt64
    var pending: Optional[UInt64]
    var listing: Optional[PathListing]
    var insert_pane: Optional[UInt64]
    var start_dir: Optional[String]
    var budget: Int

    def __init__(out self, budget: Int = PATH_RETAINED_BYTES):
        self.source = PathSource(0, 0, 0)
        self.next_request = 1
        self.pending = None
        self.listing = None
        self.insert_pane = None
        self.start_dir = None
        self.budget = budget

    def set_directory(mut self, source: PathSource, host: String, dir: String, pane: Optional[UInt64] = None) raises -> List[PathCommand]:
        var commands = List[PathCommand]()
        if self.listing and self.source.matches(source) and self.listing.value().covers(host, dir):
            return commands^
        var message = PathMessage(PATH_REQUEST, self.next_request)
        message.pane = pane
        message.dir = dir.copy()
        var frame = encode_path_message(message)
        var replacement = PathListing(host, dir, self.next_request)
        if replacement.retained_bytes > self.budget:
            raise Error("path listing directory exceeds retained byte budget")
        if self.pending:
            commands.append(_command(self.source, self.pending.value(), PATH_CANCEL))
        commands.append(PathCommand(source, self.next_request, frame^))
        self.listing = replacement^
        self.pending = self.next_request
        self.next_request += 1
        self.source = source
        return commands^

    def apply(mut self, source: PathSource, var message: PathMessage) raises -> PathApplied:
        if not self.source.matches(source) or not self.pending or self.pending.value() != message.request or not self.listing:
            return PathApplied(False, None)
        if message.kind != PATH_BEGIN and message.kind != PATH_CHUNK and message.kind != PATH_GIT:
            return PathApplied(False, None)
        ref listing = self.listing.value()
        if listing.additional(message) > self.budget - listing.retained_bytes:
            var cancel = _command(self.source, self.pending.value(), PATH_CANCEL)
            listing.fail_budget()
            self.pending = None
            return PathApplied(True, cancel^)
        var finished = (message.kind == PATH_BEGIN and Bool(message.error)) or (message.kind == PATH_CHUNK and message.done)
        var changed = listing.apply(message^, self.budget)
        if finished:
            self.pending = None
        return PathApplied(changed, None)

    def close(mut self) raises -> Optional[PathCommand]:
        var result: Optional[PathCommand] = None
        if self.pending:
            result = _command(self.source, self.pending.value(), PATH_CANCEL)
        self.pending = None
        self.listing = None
        self.insert_pane = None
        self.start_dir = None
        return result^

    def opened(mut self, source: PathSource, pane: UInt64, start: Optional[String] = None) raises -> Optional[PathCommand]:
        var cancel = self.close()
        self.source = source
        self.insert_pane = pane
        self.start_dir = start.copy()
        return cancel^
