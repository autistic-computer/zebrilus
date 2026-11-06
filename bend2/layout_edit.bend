import Base
import ./core.bend as Core

def complement_ratio(bits: U32) -> U32:
  match bits:
    case U32{word}:
      F32.bits(F32.sub(1.0, F32{word}))

def new_split(existing: Core.StableId, pane: Core.StableId, split: Core.StableId, axis: Core.Axis, ratio: U32, before: Bool) -> Core.Layout:
  match before:
    case True{}:
      Core.Split{split, axis, ratio, Core.Pane{pane}, Core.Pane{existing}}
    case False{}:
      Core.Split{split, axis, complement_ratio(ratio), Core.Pane{existing}, Core.Pane{pane}}

def insert_leaf(existing: Core.StableId, pane: Core.StableId, split: Core.StableId, axis: Core.Axis, ratio: U32, before: Bool, matched: Bool) -> Core.LayoutEdit:
  match matched:
    case True{}:
      Core.Edit{True{}, new_split(existing, pane, split, axis, ratio, before)}
    case False{}:
      Core.Edit{False{}, Core.Pane{existing}}

def insert_split(first: Core.Layout, second: Core.Layout, id: Core.StableId, axis: Core.Axis, ratio: U32, left_edit: Core.LayoutEdit, right_edit: Core.LayoutEdit) -> Core.LayoutEdit:
  match left_edit right_edit:
    case Core.Edit{True{}, left} _:
      Core.Edit{True{}, Core.Split{id, axis, ratio, left, second}}
    case Core.Edit{False{}, _} Core.Edit{changed, right}:
      Core.Edit{changed, Core.Split{id, axis, ratio, first, right}}

def insert(node: Core.Layout, +target: Core.StableId, +pane: Core.StableId, +split: Core.StableId, +axis: Core.Axis, +ratio: U32, +before: Bool) -> Core.LayoutEdit:
  match node:
    case Core.Empty{}:
      Core.Edit{False{}, Core.Empty{}}
    case Core.Pane{+existing}:
      insert_leaf(existing, pane, split, axis, ratio, before, Core.id_equal(existing, target))
    case Core.Split{id, old_axis, old_ratio, +first, +second}:
      left right = insert(first, target, pane, split, axis, ratio, before) insert(second, target, pane, split, axis, ratio, before)
      insert_split(first, second, id, old_axis, old_ratio, left, right)

def finish_join(edit: Core.LayoutEdit) -> Maybe<&2, Core.Layout>:
  match edit:
    case Core.Edit{True{}, node}:
      Some{node}
    case Core.Edit{False{}, _}:
      None{}

def insert_removed(removed: Core.LayoutEdit, source: Core.StableId, target: Core.StableId, split: Core.StableId, axis: Core.Axis, ratio: U32, before: Bool) -> Maybe<&2, Core.Layout>:
  match removed:
    case Core.Edit{False{}, _}:
      None{}
    case Core.Edit{True{}, node}:
      finish_join(insert(node, target, source, split, axis, ratio, before))

def join_distinct(node: Core.Layout, +source: Core.StableId, target: Core.StableId, split: Core.StableId, axis: Core.Axis, ratio: U32, before: Bool, same: Bool) -> Maybe<&2, Core.Layout>:
  match same:
    case True{}:
      None{}
    case False{}:
      insert_removed(Core.remove_leaf(node, source), source, target, split, axis, ratio, before)

def joined_layout(node: Core.Layout, +source: Core.StableId, +target: Core.StableId, split: Core.StableId, axis: Core.Axis, ratio: U32, before: Bool) -> Maybe<&2, Core.Layout>:
  join_distinct(node, source, target, split, axis, ratio, before, Core.id_equal(source, target))
