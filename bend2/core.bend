import Base

type StableId is Data:
  Id{high: U32, low: U32}

def bits_equal(n: Nat, a: Word(n), b: Word(n)) -> Bool:
  match n:
    case 0n:
      True{}
    case 1n+p:
      match a b:
        case WCon{x, xs} WCon{y, ys}:
          Bool.not(Bool.xor(x, y)) && bits_equal(p, xs, ys)

def u32_equal(a: U32, b: U32) -> Bool:
  match a b:
    case U32{x} U32{y}:
      bits_equal(32n, x, y)

def id_equal(a: StableId, b: StableId) -> Bool:
  match a b:
    case Id{ah, al} Id{bh, bl}:
      u32_equal(ah, bh) && u32_equal(al, bl)

type Axis is Data:
  Horizontal{}
  Vertical{}

type Layout is Data:
  Empty{}
  Pane{id: StableId}
  Split{id: StableId, axis: Axis, ratio_bits: U32, first: Layout, second: Layout}

type LayoutShape is Data:
  NoShape{}
  LeafShape{}
  SplitShape{id: StableId, axis: Axis, ratio_bits: U32, first: LayoutShape, second: LayoutShape}

def shape(node: Layout) -> LayoutShape:
  match node:
    case Empty{}:
      NoShape{}
    case Pane{_}:
      LeafShape{}
    case Split{id, axis, ratio, first, second}:
      a b = shape(first) shape(second)
      SplitShape{id, axis, ratio, a, b}

def pane_count(node: Layout) -> Nat:
  match node:
    case Empty{}:
      0n
    case Pane{_}:
      1n
    case Split{_, _, _, first, second}:
      a b = pane_count(first) pane_count(second)
      Nat.add(a, b)

def append_ids(a: List<&2, StableId>, b: List<&2, StableId>) -> List<&2, StableId>:
  match a:
    case Nil{}:
      b
    case h <> t:
      h <> append_ids(t, b)

def panes(node: Layout) -> List<&2, StableId>:
  match node:
    case Empty{}:
      Nil{}
    case Pane{id}:
      [id]
    case Split{_, _, _, first, second}:
      a b = panes(first) panes(second)
      append_ids(a, b)

def contains(node: Layout, +target: StableId) -> Bool:
  match node:
    case Empty{}:
      False{}
    case Pane{id}:
      id_equal(id, target)
    case Split{_, _, _, first, second}:
      a b = contains(first, target) contains(second, target)
      a || b

def swap_id.choose(+pane: StableId, +source: StableId, target: StableId, source_match: Bool, target_match: Bool) -> StableId:
  match source_match target_match:
    case True{} _:
      target
    case False{} True{}:
      source
    case False{} False{}:
      pane

def swap_id(+pane: StableId, +source: StableId, +target: StableId) -> StableId:
  swap_id.choose(pane, source, target, id_equal(pane, source), id_equal(pane, target))

def swapped_layout(node: Layout, +source: StableId, +target: StableId) -> Layout:
  match node:
    case Empty{}:
      Empty{}
    case Pane{id}:
      Pane{swap_id(id, source, target)}
    case Split{id, axis, ratio, first, second}:
      a b = swapped_layout(first, source, target) swapped_layout(second, source, target)
      Split{id, axis, ratio, a, b}

type FrameStatus is Data:
  Valid{}
  Truncated{}
  FrameTooLarge{}
  LengthMismatch{}
  UnsupportedLane{}
  VersionMismatch{}
  UnsupportedFlags{}

def frame_gate(short_prefix: Bool, too_large: Bool, wrong_length: Bool, short_header: Bool, bad_lane: Bool, bad_version: Bool, bad_flags: Bool) -> FrameStatus:
  match short_prefix too_large wrong_length short_header bad_lane bad_version bad_flags:
    case True{} _ _ _ _ _ _:
      Truncated{}
    case False{} True{} _ _ _ _ _:
      FrameTooLarge{}
    case False{} False{} True{} _ _ _ _:
      LengthMismatch{}
    case False{} False{} False{} True{} _ _ _:
      Truncated{}
    case False{} False{} False{} False{} True{} _ _:
      UnsupportedLane{}
    case False{} False{} False{} False{} False{} True{} _:
      VersionMismatch{}
    case False{} False{} False{} False{} False{} False{} True{}:
      UnsupportedFlags{}
    case False{} False{} False{} False{} False{} False{} False{}:
      Valid{}

def validate_envelope(+total: U32, +declared: U32, lane: U32, flags: U32, version: U32) -> FrameStatus:
  frame_gate((total < 4 : U32), (declared > 67108864 : U32),
    U32.is_ne((total - 4 : U32), declared), (declared < 4 : U32),
    (lane > 1 : U32), U32.is_ne(version, 118), U32.is_ne(flags, 0))

def status_code(status: FrameStatus) -> U32:
  match status:
    case Valid{}:
      0
    case Truncated{}:
      1
    case FrameTooLarge{}:
      2
    case LengthMismatch{}:
      3
    case UnsupportedLane{}:
      4
    case VersionMismatch{}:
      5
    case UnsupportedFlags{}:
      6

def accepted(status: FrameStatus) -> Bool:
  match status:
    case Valid{}:
      True{}
    case _:
      False{}

type AgentPhase is Data:
  PhaseStarting{}
  PhaseReady{}
  PhaseRunning{}
  PhaseAwaitingPermission{}
  PhaseFailed{}

type Attention is Data:
  Idle{}
  Working{}
  NeedsInput{}
  AttentionFailed{}

def attention(phase: AgentPhase, permission: Bool) -> Attention:
  match phase permission:
    case _ True{}:
      NeedsInput{}
    case PhaseRunning{} False{}:
      Working{}
    case PhaseAwaitingPermission{} False{}:
      Working{}
    case PhaseFailed{} False{}:
      AttentionFailed{}
    case _ False{}:
      Idle{}

type AttentionEdge is Data:
  NoEdge{}
  RequestEdge{}
  DoneEdge{}
  FailedEdge{}

def attention_edge(previous: Attention, current: Attention) -> AttentionEdge:
  match previous current:
    case NeedsInput{} NeedsInput{}:
      NoEdge{}
    case _ NeedsInput{}:
      RequestEdge{}
    case Working{} Idle{}:
      DoneEdge{}
    case AttentionFailed{} AttentionFailed{}:
      NoEdge{}
    case _ AttentionFailed{}:
      FailedEdge{}
    case _ _:
      NoEdge{}


type LayoutEdit is Data:
  Edit{changed: Bool, node: Layout}

def is_leaf(node: Layout, target: StableId) -> Bool:
  match node:
    case Pane{id}:
      id_equal(id, target)
    case _:
      False{}

def remove_split.choose(first: Layout, second: Layout, id: StableId, axis: Axis, ratio: U32, left_direct: Bool, right_direct: Bool, left_edit: LayoutEdit, right_edit: LayoutEdit) -> LayoutEdit:
  match left_direct right_direct left_edit right_edit:
    case True{} _ _ _:
      Edit{True{}, second}
    case False{} True{} _ _:
      Edit{True{}, first}
    case False{} False{} Edit{True{}, left} _:
      Edit{True{}, Split{id, axis, ratio, left, second}}
    case False{} False{} Edit{False{}, _} Edit{True{}, right}:
      Edit{True{}, Split{id, axis, ratio, first, right}}
    case False{} False{} Edit{False{}, _} Edit{False{}, _}:
      Edit{False{}, Split{id, axis, ratio, first, second}}

def remove_leaf(node: Layout, +target: StableId) -> LayoutEdit:
  match node:
    case Empty{}:
      Edit{False{}, Empty{}}
    case Pane{id}:
      Edit{False{}, Pane{id}}
    case Split{id, axis, ratio, +first, +second}:
      left right = remove_leaf(first, target) remove_leaf(second, target)
      remove_split.choose(first, second, id, axis, ratio, is_leaf(first, target), is_leaf(second, target), left, right)
