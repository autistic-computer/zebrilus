from std.atomic import Atomic
from std.memory import OwnedPointer
from mjui.sys.dispatch import (
    DispatchRef,
    dispatch_async,
    dispatch_global_queue,
    dispatch_semaphore,
    dispatch_signal,
    dispatch_wait,
    dispatch_release,
)
from .path_rank import PathRanker
from .path_rank_job import PathRankJob, PathRankResult, run_path_rank


struct _PathWork(Movable):
    var ranker: PathRanker
    var request: Optional[PathRankJob]
    var result: Optional[PathRankResult]
    var done: Atomic[Int64]
    var semaphore: DispatchRef

    def __init__(out self) raises:
        self.ranker = PathRanker()
        self.request = None
        self.result = None
        self.done = Atomic[Int64](1)
        self.semaphore = dispatch_semaphore()


def _run_path_work(context: MutOpaquePointer[MutUntrackedOrigin]) abi("C"):
    var pointer = context.unsafe_bitcast[_PathWork]()
    ref work = pointer[]
    if work.request:
        var request = work.request.take()
        work.result = run_path_rank(work.ranker, request^)
    work.done.store(1)
    dispatch_signal(work.semaphore)


struct PalettePathWorker(Movable):
    var work: OwnedPointer[_PathWork]
    var busy: Bool

    def __init__(out self) raises:
        self.work = OwnedPointer(_PathWork())
        self.busy = False

    def __deinit__(deinit self):
        if self.busy:
            dispatch_wait(self.work[].semaphore)
        dispatch_release(self.work[].semaphore)

    def submit(mut self, var job: PathRankJob) -> Bool:
        if self.busy:
            return False
        self.work[].request = job^
        self.work[].result = None
        self.work[].done.store(0)
        self.busy = True
        dispatch_async(
            dispatch_global_queue(),
            self.work.ptr().unsafe_bitcast[NoneType](),
            _run_path_work,
        )
        return True

    def collect(mut self, wait: Bool = False) -> Optional[PathRankResult]:
        if not self.busy or (not wait and self.work[].done.load() == 0):
            return None
        dispatch_wait(self.work[].semaphore)
        self.busy = False
        return self.work[].result.take()
