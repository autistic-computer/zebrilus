from zzmojo.path_protocol import PathMessage, PathEntry, PathRoot, PATH_REQUEST, PATH_BEGIN, PATH_CHUNK, PATH_CANCEL, PATH_OPEN
from zzmojo.path_service import PathSource
from zzmojo.path_mailbox import PathMailbox, PathInbox, PathDelivery, PATH_CLOSE, PATH_MAILBOX_MESSAGES


def main() raises:
    var source = PathSource(1, 2, 3)
    var stale = PathSource(1, 2, 4)
    var mailbox = PathMailbox(source)
    var request = PathMessage(PATH_REQUEST, 1)
    if mailbox.send(stale, request) or not mailbox.send(source, request):
        raise Error("path mailbox lost outgoing source ownership")
    var begin = PathMessage(PATH_BEGIN, 2)
    begin.root = PathRoot("/tmp", "/tmp/", None, 0)
    if mailbox.route(begin.copy()):
        raise Error("unsolicited native listing entered the path mailbox")
    begin.request = 1
    if not mailbox.route(begin^):
        raise Error("solicited native begin was lost")
    var chunk = PathMessage(PATH_CHUNK, 1)
    chunk.entries.append(PathEntry("entry", 0, False))
    if not mailbox.route(chunk^):
        raise Error("solicited native chunk was lost")
    mailbox.close_picker()
    var opened = PathMessage(PATH_OPEN)
    opened.pane = UInt64(7)
    opened.dir = String("~/src/")
    _ = mailbox.route(opened^)
    var delivered = mailbox.inbox.take()
    if len(delivered.events) != 4 or mailbox.inbox.bytes != 0 or len(mailbox.inbox.events) != 0:
        raise Error("path mailbox did not transfer event ownership")
    if delivered.events[0].message.kind != PATH_BEGIN or delivered.events[1].message.kind != PATH_CHUNK or delivered.events[2].message.kind != PATH_CLOSE or delivered.events[3].message.kind != PATH_OPEN:
        raise Error("path mailbox reordered replies and picker notifications")
    if not delivered.events[3].source.matches(source) or delivered.events[3].message.pane.value() != 7:
        raise Error("path picker notification lost its captured source/pane")
    var cancel = PathMessage(PATH_CANCEL, 1)
    if not mailbox.send(source, cancel) or mailbox.send(source, cancel):
        raise Error("path mailbox duplicated cancellation")
    chunk = PathMessage(PATH_CHUNK, 1)
    chunk.done = True
    if mailbox.route(chunk^):
        raise Error("a canceled native listing survived mailbox routing")
    _ = mailbox.send(source, request)
    mailbox.connected(4, 5)
    if len(mailbox.registry.pending) != 0 or mailbox.registry.source.server != 4 or mailbox.registry.source.client != 5 or mailbox.registry.source.epoch != 3 or mailbox.inbox.events[0].message.kind != PATH_CLOSE:
        raise Error("new native connection retained old path request state")
    var inbox = PathInbox()
    var large = PathMessage(PATH_CHUNK, 1)
    for _ in range(50000):
        large.entries.append(PathEntry(String("q") * 200, 0, False))
    var rejected = False
    try:
        inbox.append(PathDelivery(source, large^))
    except:
        rejected = True
    if not rejected or inbox.bytes != 0 or len(inbox.events) != 0:
        raise Error("oversized path mailbox append partially mutated state")
    for _ in range(PATH_MAILBOX_MESSAGES):
        inbox.append(PathDelivery(source, PathMessage(PATH_CLOSE)))
    var one = PathInbox()
    one.append(PathDelivery(source, PathMessage(PATH_CLOSE)))
    var prior = inbox.bytes
    rejected = False
    try:
        inbox.merge(one^)
    except:
        rejected = True
    if not rejected or len(inbox.events) != PATH_MAILBOX_MESSAGES or inbox.bytes != prior:
        raise Error("path mailbox merge lost its atomic count limit")
    print("Path mailbox: ordered provenance, solicited routing, cancellation and bounded atomic handoff passed")
