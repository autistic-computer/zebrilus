from std.memory import ArcPointer
from std.atomic import Atomic
from std.sys import size_of
from .path_service import PathSource
from .path_protocol import PathMark
from .path_listing import PathListing, PATH_RETAINED_BYTES
from .path_rank import PathRanker
from .path_rows import PathRow, path_rows
from .path_text import plain_path


struct _PathRankCancellation(Movable):
    var value: Atomic[Int64]

    def __init__(out self):
        self.value = Atomic[Int64](0)


@fieldwise_init
struct PathRankTicket(TrivialRegisterPassable):
    var source: PathSource
    var epoch: UInt64
    var serial: UInt64
    var generation: UInt64

    def matches(self, other: Self) -> Bool:
        return (
            self.source.matches(other.source)
            and self.epoch == other.epoch
            and self.serial == other.serial
            and self.generation == other.generation
        )


@fieldwise_init
struct PathRankJob(Movable):
    var ticket: PathRankTicket
    var keep: Bool
    var listing: PathListing
    var base: String
    var filter: String
    var cancellation: ArcPointer[_PathRankCancellation]
    var snapshot_bytes: Int

    def cancelled(self) -> Bool:
        return self.cancellation[].value.load() != 0


@fieldwise_init
struct PathRankResult(Movable):
    var ticket: PathRankTicket
    var keep: Bool
    var rows: List[PathRow]
    var error: Optional[String]
    var cancelled: Bool


@fieldwise_init
struct PathRankLanding(TrivialRegisterPassable):
    var accepted: Bool
    var landed: Bool
    var picked: Bool
    var scroll: Int


def _snapshot_cost(listing: PathListing, base: String, filter: String) -> Int:
    var bytes = (
        size_of[PathListing]()
        + len(listing.chunks) * 32
        + 2 * (base.byte_length() + filter.byte_length())
        + 128
    )
    for key in listing.marks.exact.keys():
        bytes += 128 + 2 * key.byte_length()
    for mark in listing.marks.directories:
        bytes += 2 * size_of[PathMark]() + 2 * mark.rel.byte_length() + 32
    return bytes


@inline(.never)
def run_path_rank(
    mut ranker: PathRanker, var job: PathRankJob
) -> PathRankResult:
    var rows = List[PathRow]()
    var error: Optional[String] = None
    if not job.cancelled():
        try:
            rows = path_rows(ranker, job.listing, job.base, job.filter)
        except caught:
            error = String(caught)
    var cancelled = job.cancelled()
    if cancelled:
        rows.clear()
        error = None
    return PathRankResult(job.ticket, job.keep, rows^, error^, cancelled)


struct PathRanking(Movable):
    var source: PathSource
    var epoch: UInt64
    var next_serial: UInt64
    var generation: UInt64
    var active: Optional[PathRankTicket]
    var cancellation: Optional[ArcPointer[_PathRankCancellation]]
    var dirty: Bool
    var keep: Bool
    var rows: List[PathRow]
    var selected: Int
    var pending: Optional[String]
    var wanted: Optional[String]
    var error: Optional[String]

    def __init__(out self, source: PathSource = PathSource(0, 0, 0)):
        self.source = source
        self.epoch = 0
        self.next_serial = 0
        self.generation = 0
        self.active = None
        self.cancellation = None
        self.dirty = False
        self.keep = True
        self.rows = List[PathRow]()
        self.selected = 0
        self.pending = None
        self.wanted = None
        self.error = None

    def reset(mut self, source: PathSource):
        if self.cancellation:
            self.cancellation.value()[].value.store(1)
        self.source = source
        self.epoch += 1
        self.generation = 0
        self.active = None
        self.cancellation = None
        self.dirty = False
        self.keep = True
        self.rows.clear()
        self.selected = 0
        self.pending = None
        self.wanted = None
        self.error = None

    def refresh(mut self, keep: Bool):
        self.dirty = True
        if not self.active:
            self.keep = keep

    def replace_listing(mut self, keep: Bool):
        self.generation += 1
        self.refresh(keep)

    @inline(.never)
    def prepare(
        mut self, listing: PathListing, dir: String, filter: String
    ) raises -> Optional[PathRankJob]:
        if self.active or not self.dirty:
            return None
        if dir.byte_length() > 4096 or filter.byte_length() > 65536:
            raise Error("path ranking input exceeds its text budget")
        var base = String(
            from_utf8_lossy=dir.as_bytes()[listing.dir.byte_length() :]
        ) if dir.startswith(listing.dir) else String()
        var bytes = _snapshot_cost(listing, base, filter)
        if bytes > PATH_RETAINED_BYTES:
            raise Error("path ranking snapshot exceeds its 16 MiB budget")
        var snapshot = PathListing()
        snapshot.chunks = listing.chunks.copy()
        snapshot.marks.exact = listing.marks.exact.copy()
        snapshot.marks.directories = listing.marks.directories.copy()
        var cancellation = ArcPointer(_PathRankCancellation())
        var ticket = PathRankTicket(
            self.source, self.epoch, self.next_serial, self.generation
        )
        var job = PathRankJob(
            ticket,
            self.keep,
            snapshot^,
            base^,
            filter.copy(),
            cancellation.copy(),
            bytes,
        )
        self.next_serial += 1
        self.active = ticket
        self.cancellation = cancellation^
        self.dirty = False
        return job^

    def navigate(mut self, direction: Int) -> Int:
        if len(self.rows) == 0:
            return self.selected
        if direction < 0:
            self.selected = (
                self.selected - 1 if self.selected > 0 else len(self.rows) - 1
            )
        else:
            self.selected = (self.selected + 1) % len(self.rows)
        return self.selected

    def select_edge(mut self, last: Bool) -> Int:
        self.selected = max(len(self.rows) - 1, 0) if last else 0
        return self.selected

    @inline(.never)
    def complete(
        mut self, var result: PathRankResult, listing: Optional[PathListing]
    ) -> PathRankLanding:
        if (
            not self.active
            or not self.active.value().matches(result.ticket)
            or not self.source.matches(result.ticket.source)
            or self.epoch != result.ticket.epoch
        ):
            return PathRankLanding(False, False, False, -1)
        self.active = None
        self.cancellation = None
        self.keep = True
        if result.cancelled:
            return PathRankLanding(True, False, False, -1)
        if result.error:
            self.error = result.error.take()
            self.rows.clear()
            self.selected = 0
            self.dirty = False
            return PathRankLanding(True, False, False, 0)
        if result.ticket.generation != self.generation:
            return PathRankLanding(True, False, False, -1)
        var kept: Optional[String] = None
        if (
            result.keep
            and self.selected >= 0
            and self.selected < len(self.rows)
        ):
            kept = self.rows[self.selected].name.copy()
        swap(self.rows, result.rows)
        var pending = -1
        var preserved = -1
        for index in range(len(self.rows)):
            if (
                self.pending
                and pending < 0
                and self.rows[index].name == self.pending.value()
            ):
                pending = index
            if kept and preserved < 0 and self.rows[index].name == kept.value():
                preserved = index
        if (
            pending >= 0
            or not listing
            or listing.value().done
            or listing.value().error
        ):
            self.pending = None
        self.selected = (
            pending if pending >= 0 else preserved if preserved >= 0 else 0
        )
        var picked = False
        if self.wanted and listing:
            if listing.value().root:
                ref root = listing.value().root.value()
                for index in range(len(self.rows)):
                    if (
                        plain_path(root.root, self.rows[index].entry.rel)
                        == self.wanted.value()
                    ):
                        self.wanted = None
                        self.selected = index
                        picked = True
                        break
            if listing.value().done or listing.value().error:
                self.wanted = None
        self.error = None
        return PathRankLanding(True, True, picked, self.selected)
