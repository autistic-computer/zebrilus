from .backend import AsyncBackend
from .state import AppModel
from .palette_files import (
    PaletteFiles,
    FilesEffect,
    FILES_INSERT,
    FILES_COPY_CLOSE,
    FILES_COPIED,
)
from .palette_path_worker import PalettePathWorker
from .path_service import PathSource, PathCommand
from .path_protocol import PathMessage, PATH_OPEN, PATH_BEGIN
from .path_mailbox import PathDelivery, PATH_CLOSE
from .path_context import FilesPlace, start_folder, listing_pane, path_inserts
from .path_listing import PathListing
from .path_rows import PathRow
from .path_text import start_dir, insert_path_text
from .input_protocol import input_paste


struct PaletteFilesRuntime(Movable):
    var files: PaletteFiles
    var worker: Optional[PalettePathWorker]
    var pick_ready: Bool
    var clipboard: Optional[String]
    var notice: Optional[String]
    var close_ready: Bool
    var error: String

    def __init__(out self):
        self.files = PaletteFiles()
        self.worker = None
        self.pick_ready = False
        self.clipboard = None
        self.notice = None
        self.close_ready = False
        self.error = String()

    def send(
        mut self, mut backend: AsyncBackend, var commands: List[PathCommand]
    ) raises:
        for command in commands:
            if not backend.path(
                command.copy()
            ) and self.files.service.pending == Optional[UInt64](
                command.request
            ):
                self.fail_request(
                    String(self.files.host_name, " is not connected")
                )

    def fail_request(mut self, error: String) raises:
        if self.files.service.listing:
            var failure = PathMessage(
                PATH_BEGIN,
                self.files.service.pending.value() if self.files.service.pending else UInt64(
                    0
                ),
            )
            failure.error = error.copy()
            if self.files.service.pending:
                _ = self.files.service.apply(
                    self.files.service.source, failure^
                )
            self.files.ranking.refresh(True)
        self.error = error

    def open(
        mut self,
        mut backend: AsyncBackend,
        model: AppModel,
        source: PathSource,
        place: FilesPlace,
        host_name: String,
        here: Bool,
        pane: Optional[UInt64],
        home: Optional[String],
        start: Optional[String],
        epoch: UInt64,
    ) raises -> String:
        var directory = start_dir(start, home) if start else start_folder(
            place, here, model.snapshot, pane, home
        )
        var cancel = self.files.open(
            source, place, host_name, here, directory, pane, epoch
        )
        if cancel:
            _ = backend.path(cancel.take())
        self.pick_ready = False
        self.clipboard = None
        self.notice = None
        self.close_ready = False
        self.error = String()
        return directory^

    def close(mut self, mut backend: AsyncBackend) raises:
        var cancel = self.files.close()
        if cancel:
            _ = backend.path(cancel.take())
        self.pick_ready = False
        self.close_ready = False

    def rest(
        mut self, mut backend: AsyncBackend, model: AppModel, text: String
    ) raises:
        var commands = self.files.set_rest(
            text, listing_pane(model.snapshot, self.files.service.insert_pane)
        )
        self.send(backend, commands^)

    def apply(
        mut self, mut backend: AsyncBackend, var delivery: PathDelivery
    ) raises -> Bool:
        if not self.files.active or not self.files.service.source.matches(
            delivery.source
        ):
            return False
        if delivery.message.kind == PATH_CLOSE:
            var cancel = self.files.service.retire_pending()
            if cancel:
                _ = backend.path(cancel.take())
            return False
        if delivery.message.kind == PATH_OPEN:
            return False
        var message = PathMessage()
        swap(message, delivery.message)
        var applied = self.files.apply(delivery.source, message^)
        if applied.cancel:
            _ = backend.path(applied.cancel.take())
        return applied.changed

    def observe_source(mut self, mut backend: AsyncBackend) raises:
        if (
            not self.files.active
            or not backend.path_source
            or self.files.service.source.matches(backend.path_source.value())
        ):
            return
        var cancel = self.files.service.retire_pending()
        if cancel:
            _ = backend.path(cancel.take())
        self.files.service.source = backend.path_source.value()
        var rows = List[PathRow]()
        swap(rows, self.files.ranking.rows)
        var selected = self.files.ranking.selected
        var pending = self.files.ranking.pending
        var wanted = self.files.ranking.wanted
        self.files.ranking.reset(self.files.service.source)
        self.files.ranking.rows = rows^
        self.files.ranking.selected = selected
        self.files.ranking.pending = pending
        self.files.ranking.wanted = wanted
        if self.files.service.listing:
            self.files.ranking.refresh(True)

    def tick(mut self) raises -> Bool:
        var changed = False
        if self.worker:
            var result = self.worker.value().collect()
            if result:
                var landed = self.files.complete(result.take())
                changed = landed.landed
                self.pick_ready = self.pick_ready or landed.picked
                if self.files.ranking.error:
                    self.error = self.files.ranking.error.value().copy()
        if self.files.active and (
            not self.worker or not self.worker.value().busy
        ):
            var job = self.files.prepare()
            if job:
                if not self.worker:
                    self.worker = PalettePathWorker()
                var owned = job.take()
                if not self.worker.value().submit(owned^):
                    raise Error(
                        "file rank worker rejected its only pending job"
                    )
        return changed

    def busy(self) -> Bool:
        return Bool(self.worker) and self.worker.value().busy

    def inserts(self, model: AppModel) raises -> Bool:
        return path_inserts(model.snapshot, self.files.service.insert_pane)

    def effect(
        mut self,
        mut backend: AsyncBackend,
        model: AppModel,
        effect: FilesEffect,
    ) raises:
        if effect.kind == FILES_INSERT:
            if effect.pane and effect.root and effect.entry:
                ref root = effect.root.value()
                ref entry = effect.entry.value()
                var text = insert_path_text(
                    root.root, entry.rel, entry.kind, root.cwd, root.insert
                )
                if not self.files.here:
                    var copied = self.files.copy_text()
                    text = String(copied.value(), " ") if copied else Optional[
                        String
                    ](None)
                if text:
                    backend.input(
                        effect.pane.value(),
                        input_paste(effect.pane.value(), text.value()),
                        self.files.insert_epoch,
                        True,
                        (
                            self.files.service.source.server,
                            self.files.service.source.client,
                        ),
                    )
            self.close_ready = True
        elif effect.kind == FILES_COPY_CLOSE or effect.kind == FILES_COPIED:
            self.clipboard = effect.text.copy()
            self.notice = String("Copied ", effect.text)
            self.close_ready = effect.kind == FILES_COPY_CLOSE
