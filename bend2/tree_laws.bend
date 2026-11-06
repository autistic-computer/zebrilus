import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_reducer.bend as Reducer
import ./tree_sort.bend as Sort
import ./tree_fields_laws.bend as Fields
import ./tree_lookup_laws.bend as Lookup

law empty_overlay_preserves_exact_snapshot_order:
  for +snapshot: Tree.TreeSnapshot
  {Reducer.overlay(snapshot, Nil{}) == Tree.Outcome{None{}, snapshot} : Tree.Outcome}

law rejected_overlay_is_atomic:
  for +snapshot: Tree.TreeSnapshot
  for +ops: List<&2, Tree.Op>
  for +error: Tree.TreeError
  for e: {Reducer.outcome_error(Reducer.overlay(snapshot, ops)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.overlay(snapshot, ops)) == snapshot : Tree.TreeSnapshot}

law rejected_delta_is_atomic:
  for +snapshot: Tree.TreeSnapshot
  for +delta: Tree.Delta
  for +error: Tree.TreeError
  for e: {Reducer.outcome_error(Reducer.apply(snapshot, delta)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.apply(snapshot, delta)) == snapshot : Tree.TreeSnapshot}

law rejected_view_is_atomic:
  for +snapshot: Tree.TreeSnapshot
  for +view: Tree.ClientView
  for +error: Tree.TreeError
  for +e: {Reducer.outcome_error(Reducer.client_view(snapshot, view)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.client_view(snapshot, view)) == snapshot : Tree.TreeSnapshot}

law overlay_preserves_generation_and_focus:
  for +snapshot: Tree.TreeSnapshot
  for +ops: List<&2, Tree.Op>
  {Reducer.header(Reducer.outcome_snapshot(Reducer.overlay(snapshot, ops))) == Reducer.header(snapshot) : Reducer.Header}

law accepted_delta_uses_exact_target_generation:
  for +snapshot: Tree.TreeSnapshot
  for +base: Core.StableId
  for +version: Core.StableId
  for +ops: List<&2, Tree.Op>
  for e: {Reducer.accepted(Reducer.apply(snapshot, Tree.Delta{base, version, ops})) == True{} : Bool}
  {Reducer.generation(Reducer.outcome_snapshot(Reducer.apply(snapshot, Tree.Delta{base, version, ops}))) == version : Core.StableId}

law accepted_delta_requires_exact_base_generation:
  for +snapshot: Tree.TreeSnapshot
  for +base: Core.StableId
  for +version: Core.StableId
  for +ops: List<&2, Tree.Op>
  for e: {Reducer.accepted(Reducer.apply(snapshot, Tree.Delta{base, version, ops})) == True{} : Bool}
  {Reducer.generation(snapshot) == base : Core.StableId}

law client_view_preserves_generation:
  for +snapshot: Tree.TreeSnapshot
  for +view: Tree.ClientView
  {Reducer.generation(Reducer.outcome_snapshot(Reducer.client_view(snapshot, view))) == Reducer.generation(snapshot) : Core.StableId}

law accepted_view_uses_exact_focus:
  for +snapshot: Tree.TreeSnapshot
  for +focus: Maybe<&2, Core.StableId>
  for +ops: List<&2, Tree.Op>
  for e: {Reducer.accepted(Reducer.client_view(snapshot, Tree.ClientView{focus, ops})) == True{} : Bool}
  {Reducer.focused(Reducer.outcome_snapshot(Reducer.client_view(snapshot, Tree.ClientView{focus, ops}))) == focus : Maybe<&2, Core.StableId>}

law windows_have_actual_lexicographic_bit_order:
  for +windows: List<&2, Sort.Tagged<Tree.WindowRecord>>
  Sort.Ordered(Tree.WindowRecord, 96n, 95, windows, Reducer.canonical_windows(windows))

law equal_window_sort_keys_preserve_complete_value_order:
  for +windows: List<&2, Sort.Tagged<Tree.WindowRecord>>
  for +key: Sort.SortKey
  {Sort.values_for_key(Tree.WindowRecord, Reducer.canonical_windows(windows), key) == Sort.values_for_key(Tree.WindowRecord, windows, key) : List<&2, Tree.WindowRecord>}

law sessions_have_actual_lexicographic_id_bit_order:
  for +sessions: List<&2, Sort.Tagged<Tree.SessionRecord>>
  Sort.Ordered(Tree.SessionRecord, 64n, 63, sessions, Reducer.canonical_sessions(sessions))

law equal_session_sort_keys_preserve_complete_value_order:
  for +sessions: List<&2, Sort.Tagged<Tree.SessionRecord>>
  for +key: Sort.SortKey
  {Sort.values_for_key(Tree.SessionRecord, Reducer.canonical_sessions(sessions), key) == Sort.values_for_key(Tree.SessionRecord, sessions, key) : List<&2, Tree.SessionRecord>}

law failed_batch_stops_before_later_operations:
  for ops: List<&2, Tree.Op>
  for +error: Tree.TreeError
  {Reducer.batch(ops, Fail{error}) == Fail{error} : Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>}
