import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_list.bend as Lists
import ./tree_sort.bend as Sort
import ./tree_word_io.bend as WordIO

type Builder is Data:
  Builder{count: U32, reversed: List<&2, U32>}

def put(builder: Builder, value: U32) -> Builder:
  match builder:
    case Builder{count, reversed}:
      Builder{U32.add(count, 1), value <> reversed}

def identifier(builder: Builder, id: Core.StableId) -> Builder:
  match id:
    case Core.Id{high, low}:
      put(put(builder, high), low)

def optional_id(builder: Builder, id: Maybe<&2, Core.StableId>) -> Builder:
  match id:
    case None{}:
      put(builder, 0)
    case Some{value}:
      identifier(put(builder, 1), value)

def count_fold(-A: Data, values: List<&2, A>, count: U32) -> U32:
  match values:
    case Nil{}:
      count
    case _ <> tail:
      count_fold(A, tail, U32.add(count, 1))

def count(-A: Data, values: List<&2, A>) -> U32:
  count_fold(A, values, 0)

def pane_body(builder: Builder, pane: Tree.PaneRecord) -> Builder:
  match pane:
    case Tree.PaneRecord{id, title, state, presentation, status, summary}:
      put(put(put(put(put(identifier(builder, id), title), state), presentation), status), summary)

def pane(builder: Builder, entry: Sort.Tagged<Tree.PaneRecord>) -> Builder:
  match entry:
    case Sort.Tagged{Sort.SortKey{_, high, low}, body}:
      pane_body(identifier(builder, Core.Id{high, low}), body)

def panes(values: List<&2, Sort.Tagged<Tree.PaneRecord>>, builder: Builder) -> Builder:
  match values:
    case Nil{}:
      builder
    case head <> tail:
      panes(tail, pane(builder, head))

def window(builder: Builder, entry: Sort.Tagged<Tree.WindowRecord>) -> Builder:
  match entry:
    case Sort.Tagged{Sort.SortKey{index, high, low}, Tree.WindowRecord{name, active, layout, flags, presentation, +children}}:
      panes(children, put(put(put(put(identifier(put(put(identifier(builder, Core.Id{high, low}), index), name), active), layout), flags), presentation), count(Sort.Tagged<Tree.PaneRecord>, children)))

def windows(values: List<&2, Sort.Tagged<Tree.WindowRecord>>, builder: Builder) -> Builder:
  match values:
    case Nil{}:
      builder
    case head <> tail:
      windows(tail, window(builder, head))

def session(builder: Builder, entry: Sort.Tagged<Tree.SessionRecord>) -> Builder:
  match entry:
    case Sort.Tagged{Sort.SortKey{_, high, low}, Tree.SessionRecord{name, active, viewers, +children}}:
      windows(children, put(put(identifier(put(identifier(builder, Core.Id{high, low}), name), active), viewers), count(Sort.Tagged<Tree.WindowRecord>, children)))

def sessions(values: List<&2, Sort.Tagged<Tree.SessionRecord>>, builder: Builder) -> Builder:
  match values:
    case Nil{}:
      builder
    case head <> tail:
      sessions(tail, session(builder, head))

def snapshot(builder: Builder, value: Tree.TreeSnapshot) -> Builder:
  match value:
    case Tree.TreeSnapshot{generation, focused, +children}:
      sessions(children, put(optional_id(identifier(builder, generation), focused), count(Sort.Tagged<Tree.SessionRecord>, children)))

def status(error: Maybe<&2, Tree.TreeError>) -> U32:
  match error:
    case None{}:
      0
    case Some{Tree.BaseMismatch{_, _}}:
      1
    case Some{Tree.MissingSession{_}}:
      2
    case Some{Tree.MissingWindow{_}}:
      3
    case Some{Tree.MissingPane{_}}:
      4

def error(builder: Builder, error: Maybe<&2, Tree.TreeError>) -> Builder:
  match error:
    case None{}:
      put(builder, 0)
    case Some{Tree.BaseMismatch{expected, received}}:
      identifier(identifier(put(builder, 1), expected), received)
    case Some{Tree.MissingSession{id}}:
      identifier(put(builder, 2), id)
    case Some{Tree.MissingWindow{id}}:
      identifier(put(builder, 3), id)
    case Some{Tree.MissingPane{id}}:
      identifier(put(builder, 4), id)

def packet(value: Tree.Outcome) -> Builder:
  match value:
    case Tree.Outcome{failure, state}:
      snapshot(error(Builder{0, Nil{}}, failure), state)

def words(builder: Builder) -> WordIO.Words:
  match builder:
    case Builder{count, reversed}:
      WordIO.Words{count, Lists.reverse(U32, reversed)}
