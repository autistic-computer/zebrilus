from std.sys import size_of
from .path_protocol import PathMessage, PathEntry, PathMark, PATH_REQUEST, PATH_CANCEL, PATH_OPEN
from .path_service import PathSource, PathRegistry


comptime PATH_CLOSE: UInt8 = 6
comptime PATH_MAILBOX_BYTES: Int = 16 * 1024 * 1024
comptime PATH_MAILBOX_MESSAGES: Int = 4096


def path_message_bytes(message: PathMessage) -> Int:
    var bytes = 2 * size_of[PathMessage]()
    if message.dir:
        bytes += 2 * message.dir.value().byte_length() + 32
    if message.error:
        bytes += 2 * message.error.value().byte_length() + 32
    if message.root:
        ref root = message.root.value()
        bytes += 2 * (root.root.byte_length() + root.display_root.byte_length()) + 64
        if root.cwd:
            bytes += 2 * root.cwd.value().byte_length() + 32
    bytes += 2 * len(message.entries) * size_of[PathEntry]()
    for entry in message.entries:
        bytes += 2 * entry.rel.byte_length() + 32
    bytes += 2 * len(message.marks) * size_of[PathMark]()
    for mark in message.marks:
        bytes += 2 * mark.rel.byte_length() + 32
    return bytes


@fieldwise_init
struct PathDelivery(Copyable, Movable):
    var source: PathSource
    var message: PathMessage


struct PathInbox(Copyable, Movable):
    var events: List[PathDelivery]
    var bytes: Int

    def __init__(out self):
        self.events = List[PathDelivery]()
        self.bytes = 0

    def append(mut self, var delivery: PathDelivery) raises:
        var bytes = path_message_bytes(delivery.message) + 2 * size_of[PathDelivery]()
        if len(self.events) >= PATH_MAILBOX_MESSAGES or bytes > PATH_MAILBOX_BYTES - self.bytes:
            raise Error("path listing mailbox exceeds its retained limit")
        self.events.append(delivery^)
        self.bytes += bytes

    def merge(mut self, var other: Self) raises:
        if len(other.events) > PATH_MAILBOX_MESSAGES - len(self.events) or other.bytes > PATH_MAILBOX_BYTES - self.bytes:
            raise Error("path listing mailbox exceeds its retained limit")
        self.bytes += other.bytes
        var events = List[PathDelivery]()
        swap(events, other.events)
        self.events.extend(events^)

    def take(mut self) -> Self:
        var result = Self()
        swap(result.events, self.events)
        result.bytes = self.bytes
        self.bytes = 0
        return result^

    def reset(mut self, source: PathSource) raises:
        self.events.clear()
        self.bytes = 0
        self.append(PathDelivery(source, PathMessage(PATH_CLOSE)))


struct PathMailbox(Copyable, Movable):
    var registry: PathRegistry
    var inbox: PathInbox

    def __init__(out self, source: PathSource = PathSource(0, 0, 0)):
        self.registry = PathRegistry(source)
        self.inbox = PathInbox()

    def connected(mut self, server: UInt64, client: UInt64) raises:
        var old = self.registry.source
        var source = PathSource(server, client, self.registry.source.epoch)
        self.registry.reset(source)
        if old.server != 0 or old.client != 0:
            self.inbox.reset(old)
        else:
            self.inbox = PathInbox()

    def close_picker(mut self) raises:
        self.inbox.append(PathDelivery(self.registry.source, PathMessage(PATH_CLOSE)))

    def send(mut self, source: PathSource, message: PathMessage) raises -> Bool:
        if not self.registry.source.matches(source):
            return False
        if message.kind == PATH_REQUEST:
            self.registry.register(source, message.request)
            return True
        if message.kind == PATH_CANCEL:
            return self.registry.cancel(source, message.request)
        raise Error("unsupported outgoing path message")

    def route(mut self, var message: PathMessage) raises -> Bool:
        if message.kind != PATH_OPEN and not self.registry.route(self.registry.source, message):
            return False
        self.inbox.append(PathDelivery(self.registry.source, message^))
        return True
