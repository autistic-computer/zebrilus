import Base
import ./core.bend as Core
import ./replay.bend as Replay
import ./tree_data.bend as Tree
import ./tree_list.bend as Lists
import ./tree_sort.bend as Sort

type Reader is Data:
  Reader{words: List<&2, U32>, remaining: U32}

type Parsed<-A: Data> is Data:
  Parsed{value: A, reader: Reader}

def Parser(-A: Data) -> Type:
  Reader -> Maybe<&2, Parsed<A>>

def Parser.pure(-A: Data, value: A) -> Parser(A):
  reader => Some{Parsed{value, reader}}

def Parser.failed(-A: Data) -> Parser(A):
  _ => None{}

def Parser.bound(-A: Data, -B: Data, parsed: Maybe<&2, Parsed<A>>, next: A -> Parser(B)) -> Maybe<&2, Parsed<B>>:
  match parsed:
    case None{}:
      None{}
    case Some{Parsed{value, reader}}:
      next(value)(reader)

def Parser.bind(-A: Data, -B: Data, parser: Parser(A), next: A -> Parser(B)) -> Parser(B):
  reader => Parser.bound(A, B, parser(reader), next)

def word(reader: Reader) -> Maybe<&2, Parsed<U32>>:
  match reader:
    case Reader{Nil{}, _}:
      None{}
    case Reader{head <> tail, remaining}:
      Some{Parsed{head, Reader{tail, U32.sub(remaining, 1)}}}

def remaining(reader: Reader) -> U32:
  match reader:
    case Reader{_, remaining}:
      remaining

def accepted_value(-A: Data, value: A, reader: Reader, accepted: Bool) -> Maybe<&2, Parsed<A>>:
  match accepted:
    case False{}:
      None{}
    case True{}:
      Some{Parsed{value, reader}}

def bounded_count(parsed: Maybe<&2, Parsed<U32>>, minimum: U32) -> Maybe<&2, Parsed<U32>>:
  match parsed:
    case None{}:
      None{}
    case Some{Parsed{+count, +reader}}:
      accepted_value(U32, count, reader, U32.is_le(count, U32.div(remaining(reader), minimum)))

def count(minimum: U32) -> Parser(U32):
  reader => bounded_count(word(reader), minimum)

def identifier() -> Parser(Core.StableId):
  do Parser<Core.StableId>:
    high : U32 <- word
    low : U32 <- word
    return Core.Id{high, low}

def optional_tag(zero: Bool, one: Bool) -> Parser(Maybe<&2, Core.StableId>):
  match zero one:
    case True{} _:
      Parser.pure(Maybe<&2, Core.StableId>, None{})
    case False{} True{}:
      do Parser<Maybe<&2, Core.StableId>>:
        id : Core.StableId <- identifier()
        return Some{id}
    case False{} False{}:
      Parser.failed(Maybe<&2, Core.StableId>)

def optional_id() -> Parser(Maybe<&2, Core.StableId>):
  do Parser<Maybe<&2, Core.StableId>>:
    +tag : U32 <- word
    optional_tag(U32.is_eq(tag, 0), U32.is_eq(tag, 1))

type VectorState<-A: Data> is Data:
  VectorState{reader: Reader, reversed: List<&2, A>}

def vector_step(-A: Data, parsed: Maybe<&2, Parsed<A>>, reversed: List<&2, A>) -> Maybe<&2, VectorState<A>>:
  match parsed:
    case None{}:
      None{}
    case Some{Parsed{value, reader}}:
      Some{VectorState{reader, value <> reversed}}

def vector_loop(~A: Data, ~element: Parser(A), count: Nat, state: Maybe<&2, VectorState<A>>) -> Maybe<&2, Parsed<List<&2, A>>>:
  match count state:
    case _ None{}:
      None{}
    case 0n Some{VectorState{reader, reversed}}:
      Some{Parsed{Lists.reverse(A, reversed), reader}}
    case 1n+rest Some{VectorState{reader, reversed}}:
      vector_loop(~A, ~element, rest, vector_step(A, element(reader), reversed))

def vector_count(~A: Data, ~element: Parser(A), count: U32) -> Parser(List<&2, A>):
  reader => vector_loop(~A, ~element, U32.to_nat(count), Some{VectorState{reader, Nil{}}})

def vector(~A: Data, ~element: Parser(A), minimum: U32) -> Parser(List<&2, A>):
  do Parser<List<&2, A>>:
    size : U32 <- count(minimum)
    vector_count(~A, ~element, size)

def pane() -> Parser(Tree.PaneRecord):
  do Parser<Tree.PaneRecord>:
    id : Core.StableId <- identifier()
    title : U32 <- word
    state : U32 <- word
    presentation : U32 <- word
    status : U32 <- word
    summary : U32 <- word
    return Tree.PaneRecord{id, title, state, presentation, status, summary}

def pane_entry() -> Parser(Sort.Tagged<Tree.PaneRecord>):
  do Parser<Sort.Tagged<Tree.PaneRecord>>:
    high : U32 <- word
    low : U32 <- word
    value : Tree.PaneRecord <- pane()
    return Sort.Tagged{Sort.SortKey{0, high, low}, value}

def follows(previous: Maybe<&2, Core.StableId>, id: Core.StableId) -> Bool:
  match previous:
    case None{}:
      True{}
    case Some{before}:
      Cmp.is_lt(Replay.sequence_cmp(before, id))

def panes_ordered(panes: List<&2, Sort.Tagged<Tree.PaneRecord>>, previous: Maybe<&2, Core.StableId>, valid: Bool) -> Bool:
  match panes:
    case Nil{}:
      valid
    case Sort.Tagged{Sort.SortKey{_, +high, +low}, _} <> tail:
      panes_ordered(tail, Some{Core.Id{high, low}}, valid && follows(previous, Core.Id{high, low}))

def require(-A: Data, value: A, valid: Bool) -> Parser(A):
  match valid:
    case False{}:
      Parser.failed(A)
    case True{}:
      Parser.pure(A, value)

def window() -> Parser(Sort.Tagged<Tree.WindowRecord>):
  do Parser<Sort.Tagged<Tree.WindowRecord>>:
    high : U32 <- word
    low : U32 <- word
    index : U32 <- word
    name : U32 <- word
    active : Core.StableId <- identifier()
    layout : U32 <- word
    flags : U32 <- word
    presentation : U32 <- word
    +panes : List<&2, Sort.Tagged<Tree.PaneRecord>> <- vector(~(Sort.Tagged<Tree.PaneRecord>), ~pane_entry(), 9)
    require(Sort.Tagged<Tree.WindowRecord>, Sort.Tagged{Sort.SortKey{index, high, low}, Tree.WindowRecord{name, active, layout, flags, presentation, panes}}, panes_ordered(panes, None{}, True{}))

def session() -> Parser(Sort.Tagged<Tree.SessionRecord>):
  do Parser<Sort.Tagged<Tree.SessionRecord>>:
    high : U32 <- word
    low : U32 <- word
    name : U32 <- word
    active : Core.StableId <- identifier()
    viewers : U32 <- word
    windows : List<&2, Sort.Tagged<Tree.WindowRecord>> <- vector(~(Sort.Tagged<Tree.WindowRecord>), ~window(), 10)
    return Sort.Tagged{Sort.SortKey{0, high, low}, Tree.SessionRecord{name, active, viewers, windows}}

def snapshot() -> Parser(Tree.TreeSnapshot):
  do Parser<Tree.TreeSnapshot>:
    generation : Core.StableId <- identifier()
    focused : Maybe<&2, Core.StableId> <- optional_id()
    sessions : List<&2, Sort.Tagged<Tree.SessionRecord>> <- vector(~(Sort.Tagged<Tree.SessionRecord>), ~session(), 7)
    return Tree.TreeSnapshot{generation, focused, sessions}

def operation_tag(tag: U32) -> Parser(Tree.Op):
  match tag:
    case 0:
      do Parser<Tree.Op>:
        value : Sort.Tagged<Tree.SessionRecord> <- session()
        return Tree.UpsertSession{value}
    case 1:
      do Parser<Tree.Op>:
        id : Core.StableId <- identifier()
        return Tree.RemoveSession{id}
    case 2:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        name : U32 <- word
        return Tree.SessionName{session, name}
    case 3:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        return Tree.SessionActiveWindow{session, window}
    case 4:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        viewers : U32 <- word
        return Tree.SessionViewers{session, viewers}
    case 5:
      do Parser<Tree.Op>:
        id : Core.StableId <- identifier()
        value : Sort.Tagged<Tree.WindowRecord> <- window()
        return Tree.UpsertWindow{id, value}
    case 6:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        return Tree.RemoveWindow{session, window}
    case 7:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        name : U32 <- word
        return Tree.WindowName{session, window, name}
    case 8:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        return Tree.WindowActivePane{session, window, pane}
    case 9:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        layout : U32 <- word
        return Tree.WindowLayout{session, window, layout}
    case 10:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        index : U32 <- word
        flags : U32 <- word
        return Tree.WindowFlags{session, window, index, flags}
    case 11:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        presentation : U32 <- word
        return Tree.WindowPresentation{session, window, presentation}
    case 12:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Tree.PaneRecord <- pane()
        return Tree.UpsertPane{session, window, pane}
    case 13:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        return Tree.RemovePane{session, window, pane}
    case 14:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        value : U32 <- word
        return Tree.PaneTitle{session, window, pane, value}
    case 15:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        value : U32 <- word
        return Tree.PaneState{session, window, pane, value}
    case 16:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        value : U32 <- word
        return Tree.PanePresentation{session, window, pane, value}
    case 17:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        value : U32 <- word
        return Tree.PaneStatus{session, window, pane, value}
    case 18:
      do Parser<Tree.Op>:
        session : Core.StableId <- identifier()
        window : Core.StableId <- identifier()
        pane : Core.StableId <- identifier()
        value : U32 <- word
        return Tree.PaneSummary{session, window, pane, value}
    case _:
      Parser.failed(Tree.Op)

def operation() -> Parser(Tree.Op):
  do Parser<Tree.Op>:
    tag : U32 <- word
    operation_tag(tag)

def operations() -> Parser(List<&2, Tree.Op>):
  vector(~Tree.Op, ~operation(), 3)

def delta() -> Parser(Tree.Delta):
  do Parser<Tree.Delta>:
    base : Core.StableId <- identifier()
    version : Core.StableId <- identifier()
    ops : List<&2, Tree.Op> <- operations()
    return Tree.Delta{base, version, ops}

def view() -> Parser(Tree.ClientView):
  do Parser<Tree.ClientView>:
    focused : Maybe<&2, Core.StableId> <- optional_id()
    ops : List<&2, Tree.Op> <- operations()
    return Tree.ClientView{focused, ops}

def completed(-A: Data, value: A, valid: Bool) -> Maybe<&2, A>:
  match valid:
    case False{}:
      None{}
    case True{}:
      Some{value}

def complete(-A: Data, parsed: Maybe<&2, Parsed<A>>) -> Maybe<&2, A>:
  match parsed:
    case None{}:
      None{}
    case Some{Parsed{value, Reader{Nil{}, remaining}}}:
      completed(A, value, U32.is_eq(remaining, 0))
    case _:
      None{}
