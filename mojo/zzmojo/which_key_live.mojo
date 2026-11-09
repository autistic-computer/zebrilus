from mjui.ui import Ui
from mjui.widgets.penna import PennaTheme
from .desktop_router_delivery import DesktopUiEffect
from .prefix_bindings import PrefixBinding, PrefixTable, MAX_PREFIX_TEXT
from .which_key import WhichKeyModel
from .which_key_sheet import WhichKeySheet, which_key_sheet
from .which_key_component import paint_which_key
from .which_key_render_plan import prepare_which_key
from .which_key_timing import WhichKeyChange, WhichKeyTiming, arm_table, timer_decision, TIMER_SHOW

comptime WHICH_KEY_KEY: UInt64 = 0x7600000000000000
comptime MAX_WHICH_KEY_DELAY_MS: UInt64 = 2000
comptime WHICH_KEY_INPUT: UInt8 = 0
comptime WHICH_KEY_TABLE: UInt8 = 1
comptime WHICH_KEY_ALL: UInt8 = 2
comptime WHICH_KEY_ROUTE: UInt8 = 3


@fieldwise_init
struct WhichKeyIntent(Copyable, Movable):
    var kind: UInt8
    var time: UInt64
    var eligible: Bool
    var held: Bool
    var armed: Bool
    var table: Optional[String]
    var repeat: Bool
    var delay: UInt64
    var bindings: List[PrefixBinding]
    var prefix: Optional[String]

    @staticmethod
    def input(eligible: Bool, held: Bool, armed: Bool) -> Self:
        return Self(WHICH_KEY_INPUT, 0, eligible, held, armed, None, False, 0, List[PrefixBinding](), None)

    @staticmethod
    def table_changed(time: UInt64, table: Optional[String], repeat: Bool, delay: UInt64) raises -> Self:
        if delay > MAX_WHICH_KEY_DELAY_MS:
            raise Error("which-key delay exceeds 2000 milliseconds")
        if table and table.value().byte_length() > MAX_PREFIX_TEXT:
            raise Error("which-key table name exceeds byte limit")
        return Self(WHICH_KEY_TABLE, time, False, False, False, table.copy(), repeat, delay, List[PrefixBinding](), None)

    @staticmethod
    def all_keys(effect: DesktopUiEffect) raises -> Self:
        if effect.kind != WHICH_KEY_ROUTE:
            raise Error("desktop effect is not a which-key intent")
        return Self(WHICH_KEY_ALL, 0, False, False, False, None, False, 0, effect.bindings.copy(), effect.canonical_prefix.copy())


@fieldwise_init
struct WhichKeyContext(Copyable, Movable):
    var table: Optional[String]
    var repeat: Bool
    var prefix: Optional[String]
    var blocked: Bool
    var top: Float32
    var bottom: Float32


@fieldwise_init
struct WhichKeyFrame(Copyable, Movable):
    var visible: Bool
    var wake: Optional[UInt64]
    var refusal: Optional[String]


struct LiveWhichKey(Movable):
    var timing: WhichKeyTiming
    var armed: UInt64
    var table: String
    var deadline: UInt64
    var sheet: Optional[WhichKeySheet]
    var model: WhichKeyModel
    var key: UInt64
    var refusal: Optional[String]

    def __init__(out self, key: UInt64 = WHICH_KEY_KEY) raises:
        self.timing = WhichKeyTiming(0, False, False)
        self.armed = 0
        self.table = String()
        self.deadline = 0
        self.sheet = None
        self.model = WhichKeyModel()
        self.key = key
        self.refusal = None

    def _build(mut self, tables: List[PrefixTable], table: String, prefix: Optional[String], all: Bool) -> Optional[WhichKeySheet]:
        try:
            return which_key_sheet(self.model, tables, table, prefix, all)
        except error:
            self.refusal = String(error)
            return None

    def _apply(mut self, change: WhichKeyChange, var sheet: Optional[WhichKeySheet]):
        if change.build:
            self.sheet = sheet^
        if not change.state.visible:
            self.sheet = None
        self.timing = change.state

    def apply(mut self, intent: WhichKeyIntent) raises:
        if intent.kind == WHICH_KEY_INPUT:
            self._apply(self.timing.input(intent.eligible, intent.held, intent.armed), None)
        elif intent.kind == WHICH_KEY_TABLE:
            var name: Optional[String] = None
            if intent.table:
                name = arm_table(Tuple(intent.table.value().copy(), intent.repeat))
            var change = self.timing.table_changed(Bool(name), intent.delay > 0)
            self._apply(change, None)
            if name and change.state.timer:
                var delay = intent.delay * 1_000_000
                self.armed = change.state.generation
                self.table = name.value().copy()
                self.deadline = intent.time + delay if intent.time <= UInt64.MAX - delay else UInt64.MAX
        elif intent.kind == WHICH_KEY_ALL:
            var all: List[PrefixTable] = [PrefixTable("prefix", intent.bindings.copy())]
            var sheet = self._build(all, "prefix", intent.prefix, True)
            self._apply(self.timing.show_all(Bool(sheet)), sheet^)
        else:
            raise Error("unknown which-key intent")

    def advance(mut self, tables: List[PrefixTable], context: WhichKeyContext, now: UInt64):
        if self.timing.timer and now >= self.deadline:
            var matches = context.table == Optional[String](self.table) and not context.repeat
            var blocked = matches and context.blocked
            var sheet: Optional[WhichKeySheet] = None
            if timer_decision(self.timing.generation, self.armed, matches, blocked) == TIMER_SHOW:
                sheet = self._build(tables, self.table.copy(), context.prefix, False)
            self._apply(self.timing.fire(self.armed, matches, blocked, Bool(sheet)), sheet^)
        self._apply(self.timing.render(context.blocked), None)

    def wake(self) -> Optional[UInt64]:
        return self.deadline if self.timing.timer else None

    def frame(mut self, mut ui: Ui, th: PennaTheme, tables: List[PrefixTable], context: WhichKeyContext) raises -> WhichKeyFrame:
        self.advance(tables, context, UInt64(max(ui.time, Float64(0)) * 1_000_000_000))
        if self.sheet:
            var painting = False
            try:
                var plan = prepare_which_key(ui, th, self.sheet.value(), context.top, context.bottom)
                painting = True
                _ = paint_which_key(ui, th, self.key, plan)
            except error:
                if painting:
                    raise error
                self.refusal = String(error)
                self._apply(self.timing.render(True), None)
        var wake = self.wake()
        if wake:
            ui.request_wake(Float64(wake.value()) / 1_000_000_000)
        var refusal = self.refusal.take() if self.refusal else None
        return WhichKeyFrame(self.timing.visible, wake, refusal^)
