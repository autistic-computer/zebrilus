from zzmojo.path_service import PathSource
from zzmojo.path_protocol import PathMessage, PathEntry, PATH_CHUNK
from zzmojo.path_listing import PathListing
from zzmojo.path_rank import PathRanker
from zzmojo.path_rank_job import PathRanking, run_path_rank


def main() raises:
    var listing: Optional[PathListing] = PathListing("local", "/root/")
    var update = PathMessage(PATH_CHUNK)
    update.entries.append(PathEntry("alpha", 0, False))
    _ = listing.value().apply(update^)
    listing.value().marks.exact["alpha"] = 0
    var state = PathRanking(PathSource(1, 2, 3))
    var ranker = PathRanker()
    state.replace_listing(True)
    var job = state.prepare(listing.value(), "/root/", "")
    if not job or state.dirty or not state.active:
        raise Error("path rank was not prepared atomically")
    var appended = PathMessage(PATH_CHUNK)
    appended.entries.append(PathEntry("beta", 0, False))
    _ = listing.value().apply(appended^)
    listing.value().marks.exact["alpha"] = 1
    state.refresh(True)
    var result = run_path_rank(ranker, job.take())
    var landed = state.complete(result^, listing)
    if (
        not landed.landed
        or len(state.rows) != 1
        or state.rows[0].name != "alpha"
        or state.rows[0].mark != Optional[UInt8](0)
        or not state.dirty
    ):
        raise Error(
            "path rank did not preserve captured chunks and dirty refresh"
        )
    var next = state.prepare(listing.value(), "/root/", "")
    state.reset(PathSource(1, 2, 4))
    if not next.value().cancelled():
        raise Error("path rank source reset did not cancel captured work")
    var stale = run_path_rank(ranker, next.take())
    if (
        not stale.cancelled
        or state.complete(stale^, listing).accepted
        or len(state.rows) != 0
    ):
        raise Error("path rank stale completion changed a fresh source")
    state.replace_listing(True)
    var old = state.prepare(listing.value(), "/root/", "")
    state.replace_listing(False)
    var outdated = run_path_rank(ranker, old.take())
    var ignored = state.complete(outdated^, listing)
    if not ignored.accepted or ignored.landed or not state.dirty:
        raise Error("path rank generation gap was not rejected and requeued")
    var serial = state.next_serial
    var refused = False
    try:
        _ = state.prepare(listing.value(), "/root/", String("x") * 65537)
    except:
        refused = True
    if (
        not refused
        or state.active
        or not state.dirty
        or state.next_serial != serial
    ):
        raise Error("path rank oversized preparation was not atomic")
    print(
        "path rank snapshot, dirty requeue, cancellation and generation checks"
        " passed"
    )
