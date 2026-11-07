from .path_protocol import PathEntry, PathRoot, PathMessage, MAX_PATH_TEXT
from .path_service import PathService, PathSource, PathCommand, PathApplied
from .path_rank_job import (
    PathRanking,
    PathRankJob,
    PathRankResult,
    PathRankLanding,
)
from .path_context import FilesPlace
from .path_text import (
    FilesPath,
    split_rest,
    parent_dir,
    path_last_name,
    plain_path,
)
from .path_rows import PathRow

comptime FILES_UNHANDLED: UInt8 = 0
comptime FILES_STAY: UInt8 = 1
comptime FILES_SET_REST: UInt8 = 2
comptime FILES_INSERT: UInt8 = 3
comptime FILES_COPY_CLOSE: UInt8 = 4
comptime FILES_COPIED: UInt8 = 5

comptime FILES_ENTER: UInt8 = 0
comptime FILES_TAB: UInt8 = 1
comptime FILES_SHIFT_TAB: UInt8 = 2
comptime FILES_LEFT: UInt8 = 3
comptime FILES_RIGHT: UInt8 = 4
comptime FILES_UP: UInt8 = 5
comptime FILES_DOWN: UInt8 = 6
comptime FILES_FIRST: UInt8 = 7
comptime FILES_LAST: UInt8 = 8
comptime FILES_BACKSPACE: UInt8 = 9
comptime FILES_COPY: UInt8 = 10


@fieldwise_init
struct FilesHint(Copyable, Movable):
    var key: String
    var label: String


struct FilesEffect(Copyable, Movable):
    var kind: UInt8
    var text: String
    var pane: Optional[UInt64]
    var root: Optional[PathRoot]
    var entry: Optional[PathEntry]

    def __init__(out self, kind: UInt8 = FILES_UNHANDLED, text: String = ""):
        self.kind = kind
        self.text = text
        self.pane = None
        self.root = None
        self.entry = None


struct PaletteFiles(Movable):
    var service: PathService
    var ranking: PathRanking
    var active: Bool
    var host: UInt32
    var host_name: String
    var here: Bool
    var place: Optional[FilesPlace]
    var start: String
    var path: FilesPath
    var insert_epoch: UInt64
    var scroll: Int

    def __init__(out self):
        self.service = PathService()
        self.ranking = PathRanking()
        self.active = False
        self.host = 0
        self.host_name = String()
        self.here = False
        self.place = None
        self.start = String("~/")
        self.path = split_rest("", self.start)
        self.insert_epoch = 0
        self.scroll = 0

    def open(
        mut self,
        source: PathSource,
        place: FilesPlace,
        host_name: String,
        here: Bool,
        start: String,
        insert_pane: Optional[UInt64],
        epoch: UInt64,
    ) raises -> Optional[PathCommand]:
        if (
            start.byte_length() > MAX_PATH_TEXT
            or host_name.byte_length() > MAX_PATH_TEXT
        ):
            raise Error("file level context exceeds its text limit")
        var cancel = self.service.close()
        self.ranking.reset(source)
        self.service.source = source
        self.service.insert_pane = insert_pane
        self.active = True
        self.host = place.host
        self.host_name = host_name
        self.here = here
        self.place = place.copy()
        self.start = start
        self.path = split_rest("", start)
        self.insert_epoch = epoch
        self.scroll = 0
        return cancel^

    def close(mut self) raises -> Optional[PathCommand]:
        var cancel = self.service.close()
        self.ranking.reset(self.service.source)
        self.active = False
        self.place = None
        return cancel^

    def set_rest(
        mut self, rest: String, listing_pane: Optional[UInt64]
    ) raises -> List[PathCommand]:
        if not self.active or rest.byte_length() > 65536:
            raise Error("file level is closed or its query exceeds 64 KiB")
        var next = split_rest(rest, self.start)
        if next.dir.byte_length() > MAX_PATH_TEXT:
            raise Error("file level directory exceeds its protocol limit")
        if (
            next.dir == self.path.dir
            and next.typed == self.path.typed
            and next.filter == self.path.filter
            and self.service.listing
        ):
            return List[PathCommand]()
        var moved = next.dir != self.path.dir
        var replacement = (
            not self.service.listing
            or not self.service.listing.value().covers(self.host_name, next.dir)
        )
        var commands = self.service.set_directory(
            self.service.source, self.host_name, next.dir, listing_pane
        )
        self.path = next^
        if replacement:
            self.ranking.replace_listing(not moved)
        else:
            self.ranking.refresh(not moved)
        return commands^

    def apply(
        mut self, source: PathSource, var message: PathMessage
    ) raises -> PathApplied:
        var applied = self.service.apply(source, message^)
        if applied.changed:
            self.ranking.refresh(True)
        return applied^

    def prepare(mut self) raises -> Optional[PathRankJob]:
        if not self.active or not self.service.listing:
            return None
        return self.ranking.prepare(
            self.service.listing.value(), self.path.dir, self.path.filter
        )

    def complete(mut self, var result: PathRankResult) -> PathRankLanding:
        var landed = self.ranking.complete(result^, self.service.listing)
        if landed.landed:
            self.scroll = landed.scroll
        return landed

    def selected_row(self) -> Optional[PathRow]:
        if self.ranking.selected < 0 or self.ranking.selected >= len(
            self.ranking.rows
        ):
            return None
        return self.ranking.rows[self.ranking.selected].copy()

    def open_directory(self) -> Optional[String]:
        var row = self.selected_row()
        if not row or row.value().entry.kind != 1:
            return None
        return String(self.path.typed, row.value().name, "/")

    def absolute_directory(self) -> Optional[String]:
        if (
            not self.service.listing
            or not self.service.listing.value().root
            or not self.path.dir.startswith(self.service.listing.value().dir)
        ):
            return None
        ref listing = self.service.listing.value()
        var under = String(
            from_utf8_lossy=self.path.dir.as_bytes()[
                listing.dir.byte_length() :
            ]
        )
        return String(listing.root.value().root.rstrip("/"), "/", under)

    def up(mut self) -> Optional[String]:
        var root = self.absolute_directory()
        var parent = parent_dir(self.path.dir, root)
        if not parent:
            return None
        self.ranking.pending = path_last_name(
            root.value() if root else self.path.dir
        )
        return parent^

    def picked_folder(self) -> Optional[String]:
        if not self.service.listing or not self.service.listing.value().root:
            return None
        ref root = self.service.listing.value().root.value()
        var rel = String()
        var row = self.selected_row()
        if row:
            rel = String(row.value().entry.rel.rstrip("/"))
            if row.value().entry.kind == 0:
                var slash = -1
                for index in range(rel.byte_length()):
                    if rel.as_bytes()[index] == 47:
                        slash = index
                rel = (
                    String(from_utf8_lossy=rel.as_bytes()[:slash]) if slash
                    >= 0 else String()
                )
        return root.display_root.copy() if rel.byte_length() == 0 else String(
            root.display_root.rstrip("/"), "/", rel
        )

    def copy_text(self) -> Optional[String]:
        var row = self.selected_row()
        if (
            not row
            or not self.service.listing
            or not self.service.listing.value().root
        ):
            return None
        return plain_path(
            self.service.listing.value().root.value().root,
            row.value().entry.rel,
            None if self.here else Optional[String](self.host_name),
        )

    def pick(self, open: Bool, inserts: Bool) -> FilesEffect:
        if open:
            var rest = self.open_directory()
            return FilesEffect(
                FILES_SET_REST, rest.value()
            ) if rest else FilesEffect()
        var row = self.selected_row()
        if (
            not row
            or not self.service.listing
            or not self.service.listing.value().root
        ):
            return FilesEffect()
        var effect = FilesEffect(FILES_INSERT if inserts else FILES_COPY_CLOSE)
        effect.pane = self.service.insert_pane
        effect.entry = row.take().entry.copy()
        effect.root = self.service.listing.value().root.copy()
        if not inserts:
            var text = self.copy_text()
            if not text:
                return FilesEffect()
            effect.text = text.take()
        return effect^

    def key(mut self, key: UInt8, inserts: Bool) -> FilesEffect:
        if key == FILES_UP or key == FILES_DOWN:
            self.scroll = self.ranking.navigate(-1 if key == FILES_UP else 1)
            return FilesEffect(FILES_STAY)
        if key == FILES_FIRST or key == FILES_LAST:
            self.scroll = self.ranking.select_edge(key == FILES_LAST)
            return FilesEffect(FILES_STAY)
        if key == FILES_TAB or key == FILES_RIGHT:
            var effect = self.pick(True, inserts)
            return (
                FilesEffect(FILES_STAY) if effect.kind == FILES_UNHANDLED
                and key == FILES_TAB else effect^
            )
        if (
            key == FILES_SHIFT_TAB
            or key == FILES_LEFT
            or key == FILES_BACKSPACE
        ):
            if key != FILES_SHIFT_TAB and (
                self.path.filter.byte_length() != 0
                or (
                    key == FILES_BACKSPACE
                    and self.path.typed.byte_length() == 0
                )
            ):
                return FilesEffect()
            var parent = self.up()
            return FilesEffect(
                FILES_SET_REST, parent.take()
            ) if parent else FilesEffect(
                FILES_STAY if key == FILES_SHIFT_TAB else FILES_UNHANDLED
            )
        if key == FILES_ENTER:
            return self.pick(False, inserts)
        if key == FILES_COPY:
            var text = self.copy_text()
            return FilesEffect(
                FILES_COPIED, text.take()
            ) if text else FilesEffect()
        return FilesEffect()

    def empty_message(self) -> String:
        if self.ranking.error:
            return self.ranking.error.value().copy()
        if not self.service.listing:
            return String("Listing…")
        ref listing = self.service.listing.value()
        if listing.error:
            return listing.error.value().copy()
        if not listing.done or self.ranking.active:
            return String("Listing…")
        return String(
            "This folder is empty" if listing.is_empty() else "No matches"
        )

    def hints(self, inserts: Bool) -> List[FilesHint]:
        var values: List[FilesHint] = [
            FilesHint("tab right", "open"),
            FilesHint("shift-tab left", "parent"),
            FilesHint("enter", "insert" if inserts else "copy"),
        ]
        if inserts:
            values.append(FilesHint("alt-enter", "copy"))
        return values^
