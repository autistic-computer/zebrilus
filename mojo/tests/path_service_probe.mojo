from zzmojo.path_protocol import PathMessage, PathEntry, PathMark, PathRoot, PATH_BEGIN, PATH_CHUNK, PATH_GIT, PATH_REQUEST, PATH_CANCEL, decode_path_message
from zzmojo.path_listing import PathListing
from zzmojo.path_service import PathService, PathSource, PathRegistry


def main() raises:
    var source = PathSource(1, 2, 3)
    var stale = PathSource(1, 2, 4)
    var service = PathService()
    var requests = service.set_directory(source, "here", "~/", UInt64(7))
    if len(requests) != 1 or requests[0].request != 1 or not requests[0].source.matches(source):
        raise Error("path request lost captured connection identity")
    var decoded = decode_path_message(requests[0].frame.copy())
    if not decoded or decoded.value().kind != PATH_REQUEST or decoded.value().pane.value() != 7 or decoded.value().dir.value() != "~/":
        raise Error("path request changed its captured insertion pane or directory")
    var registry = PathRegistry(source)
    registry.register(source, 1)
    var begin = PathMessage(PATH_BEGIN, 1)
    begin.root = PathRoot("/home/me", "~/", String("/home/me"), 4)
    if registry.route(stale, begin) or not registry.route(source, begin):
        raise Error("path registry accepted a stale source or dropped a live begin")
    if service.apply(stale, begin.copy()).changed or not service.apply(source, begin.copy()).changed:
        raise Error("path service accepted a stale source or dropped a live begin")
    var chunk = PathMessage(PATH_CHUNK, 1)
    chunk.entries.append(PathEntry("sub/", 1, False))
    _ = service.apply(source, chunk^)
    var replacement = service.set_directory(source, "here", "~/sub/", UInt64(7))
    if len(replacement) != 2 or replacement[0].request != 1 or replacement[1].request != 2:
        raise Error("incomplete path navigation failed to cancel before replacement")
    decoded = decode_path_message(replacement[0].frame.copy())
    if not decoded or decoded.value().kind != PATH_CANCEL:
        raise Error("replaced listing did not emit the source cancellation frame")
    if service.apply(source, begin.copy()).changed:
        raise Error("late completion changed a replacement listing")
    chunk = PathMessage(PATH_CHUNK, 2)
    chunk.done = True
    chunk.entries.append(PathEntry("nested/", 1, True))
    _ = service.apply(source, chunk^)
    if service.pending or len(service.set_directory(source, "here", "~/sub/nested/", UInt64(7))) != 0:
        raise Error("complete untruncated listing was not reused for a child directory")
    var git = PathMessage(PATH_GIT, 2)
    git.marks.append(PathMark("nested/", 0))
    if service.apply(source, git^).changed:
        raise Error("path service accepted git marks after the source finished routing")
    var finish = PathMessage(PATH_CHUNK, 1)
    finish.done = True
    if not registry.route(source, finish) or registry.route(source, begin) or registry.cancel(source, 1):
        raise Error("path registry retained a completed request")
    var changed_source = service.set_directory(stale, "here", "~/sub/nested/", UInt64(7))
    if len(changed_source) != 1 or changed_source[0].request != 3:
        raise Error("a new connection reused a previous connection's listing")
    var rejected = False
    try:
        _ = service.set_directory(stale, "here", String("x") * 4097, UInt64(7))
    except:
        rejected = True
    if not rejected or service.pending.value() != 3 or service.next_request != 4:
        raise Error("invalid path navigation partially changed the active request")
    var cancel = service.opened(source, 99, String("~/elsewhere/"))
    if not cancel or cancel.value().request != 3 or not cancel.value().source.matches(stale) or service.insert_pane.value() != 99 or service.start_dir.value() != "~/elsewhere/" or service.listing:
        raise Error("path picker notification lost source or insertion context")
    cancel = service.close()
    if cancel or service.insert_pane or service.start_dir:
        raise Error("path picker close retained insertion context")
    var bounded = PathService(2048)
    requests = bounded.set_directory(source, "here", "~/", UInt64(7))
    chunk = PathMessage(PATH_CHUNK, requests[0].request)
    chunk.entries.append(PathEntry(String("q") * 2000, 0, False))
    var overflow = bounded.apply(source, chunk^)
    if not overflow.changed or not overflow.cancel or bounded.pending or not bounded.listing.value().done or not bounded.listing.value().error or not bounded.listing.value().is_empty():
        raise Error("path budget overflow presented partial results or omitted cancellation")
    if bounded.listing.value().retained_bytes > bounded.budget:
        raise Error("path budget failure retained excessive state")
    registry.reset(stale)
    for request in range(64):
        registry.register(stale, UInt64(request))
    rejected = False
    try:
        registry.register(stale, 64)
    except:
        rejected = True
    if not rejected or len(registry.pending) != 64 or registry.cancel(source, 1):
        raise Error("path registry lost its bounded request/source policy")
    if not registry.cancel(stale, 1) or registry.cancel(stale, 1):
        raise Error("path cancellation was duplicated")
    registry.reset(source)
    if len(registry.pending) != 0:
        raise Error("connection reset retained old path requests")
    service.next_request = UInt64.MAX
    requests = service.set_directory(source, "here", "~/a/", UInt64(7))
    if requests[len(requests) - 1].request != UInt64.MAX:
        raise Error("path request counter changed before source wrap")
    requests = service.set_directory(source, "here", "~/b/", UInt64(7))
    if requests[len(requests) - 1].request != 0:
        raise Error("path request counter did not preserve unsigned source wrap")
    var listing = PathListing("here", "~/", UInt64(7))
    chunk = PathMessage(PATH_CHUNK, UInt64(7))
    chunk.entries.append(PathEntry("safe", 0, False))
    rejected = False
    try:
        _ = listing.apply(chunk^, listing.retained_bytes)
    except:
        rejected = True
    if not rejected or not listing.is_empty() or listing.done or listing.error:
        raise Error("rejected path update partially mutated retained state")
    var large = PathService()
    requests = large.set_directory(source, "here", "/fixture/", UInt64(7))
    chunk = PathMessage(PATH_CHUNK, requests[0].request)
    for index in range(50000):
        chunk.entries.append(PathEntry(String("entry-", index), 0, False))
    chunk.done = True
    _ = large.apply(source, chunk^)
    if large.pending or len(large.listing.value().chunks) != 1 or len(large.listing.value().chunks[0][].entries) != 50000 or large.listing.value().retained_bytes > large.budget:
        raise Error("bounded native-size listing lost entries or exceeded its retained budget")
    print("Path service: source identity, cancellation, completion, reuse, overflow and rollback passed")
