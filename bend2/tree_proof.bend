import Base
import ./core.bend as Core
import ./id_proof.bend as Id
import ./tree_data.bend as Tree
import ./tree_edit.bend as Edit
import ./tree_lookup.bend as Lookup
import ./tree_reducer.bend as Reducer
import ./tree_sort.bend as Sort
import ./tree_sort_proof.bend as SortProof
import ./tree_laws.bend as Laws
import ./tree_fields_proof.bend as Fields
import ./tree_lookup_proof.bend as LookupProof

def SomeForbidden(-A: Data, value: Maybe<&2, A>) -> Type:
  match value:
    case None{}:
      Unit
    case Some{_}:
      Empty

law none_is_not_some:
  for -A: Data
  for +value: A
  for e: {None{} == Some{value} : Maybe<&2, A>}
  Empty

def none_is_not_some(A, value, e):
  %e : SomeForbidden(A, _)
  Unit{}

law finished_rollback:
  for +snapshot: Tree.TreeSnapshot
  for +result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>
  for +error: Tree.TreeError
  for e: {Reducer.outcome_error(Reducer.finished(snapshot, result)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.finished(snapshot, result)) == snapshot : Tree.TreeSnapshot}

def finished_rollback(snapshot, result, error, e):
  match result:
    case Fail{_}:
      {==}
    case Done{_}:
      Id.absurd({Reducer.outcome_snapshot(Reducer.finished(snapshot, result)) == snapshot : Tree.TreeSnapshot}, none_is_not_some(Tree.TreeError, error, e))

def Laws.empty_overlay_preserves_exact_snapshot_order(snapshot):
  {==}

def Laws.rejected_overlay_is_atomic(snapshot, ops, error, e):
  match ops:
    case Nil{}:
      Id.absurd({snapshot == snapshot : Tree.TreeSnapshot}, none_is_not_some(Tree.TreeError, error, e))
    case head <> tail:
      finished_rollback(snapshot, Reducer.canonical_result(Reducer.batch(head <> tail, Done{snapshot})), error, e)

law versioned_error:
  for +outcome: Tree.Outcome
  for +version: Core.StableId
  {Reducer.outcome_error(Reducer.versioned(outcome, version)) == Reducer.outcome_error(outcome) : Maybe<&2, Tree.TreeError>}

def versioned_error(outcome, version):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      {==}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      {==}

law versioned_rollback:
  for +outcome: Tree.Outcome
  for +version: Core.StableId
  for +error: Tree.TreeError
  for e: {Reducer.outcome_error(Reducer.versioned(outcome, version)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.versioned(outcome, version)) == Reducer.outcome_snapshot(outcome) : Tree.TreeSnapshot}

def versioned_rollback(outcome, version, error, e):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      {==}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      Id.absurd({Reducer.outcome_snapshot(Reducer.versioned(outcome, version)) == Reducer.outcome_snapshot(outcome) : Tree.TreeSnapshot}, none_is_not_some(Tree.TreeError, error, e))

law apply_base_rollback:
  for +snapshot: Tree.TreeSnapshot
  for +base: Core.StableId
  for +version: Core.StableId
  for +ops: List<&2, Tree.Op>
  for +same: Bool
  for +error: Tree.TreeError
  for +e: {Reducer.outcome_error(Reducer.apply_base(snapshot, base, version, ops, same)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.apply_base(snapshot, base, version, ops, same)) == snapshot : Tree.TreeSnapshot}

def apply_base_rollback(snapshot, base, version, ops, same, error, e):
  match same:
    case False{}:
      {==}
    case True{}:
      +outcome = Reducer.overlay(snapshot, ops)
      prior = Equal.trans(Maybe<&2, Tree.TreeError>, Reducer.outcome_error(outcome), Reducer.outcome_error(Reducer.versioned(outcome, version)), Some{error}, Equal.sym(Maybe<&2, Tree.TreeError>, Reducer.outcome_error(Reducer.versioned(outcome, version)), Reducer.outcome_error(outcome), versioned_error(outcome, version)), e)
      Equal.trans(Tree.TreeSnapshot, Reducer.outcome_snapshot(Reducer.versioned(outcome, version)), Reducer.outcome_snapshot(outcome), snapshot, versioned_rollback(outcome, version, error, e), Laws.rejected_overlay_is_atomic(snapshot, ops, error, prior))

def Laws.rejected_delta_is_atomic(snapshot, delta, error, e):
  match delta:
    case Tree.Delta{base, version, ops}:
      apply_base_rollback(snapshot, base, version, ops, Core.id_equal(Reducer.generation(snapshot), base), error, e)

law focus_error:
  for +outcome: Tree.Outcome
  for +focus: Maybe<&2, Core.StableId>
  {Reducer.outcome_error(Reducer.with_focus(outcome, focus)) == Reducer.outcome_error(outcome) : Maybe<&2, Tree.TreeError>}

def focus_error(outcome, focus):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      {==}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      {==}

law focus_rollback:
  for +outcome: Tree.Outcome
  for +focus: Maybe<&2, Core.StableId>
  for +error: Tree.TreeError
  for e: {Reducer.outcome_error(Reducer.with_focus(outcome, focus)) == Some{error} : Maybe<&2, Tree.TreeError>}
  {Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus)) == Reducer.outcome_snapshot(outcome) : Tree.TreeSnapshot}

def focus_rollback(outcome, focus, error, e):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      {==}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      Id.absurd({Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus)) == Reducer.outcome_snapshot(outcome) : Tree.TreeSnapshot}, none_is_not_some(Tree.TreeError, error, e))

def Laws.rejected_view_is_atomic(snapshot, view, error, e):
  match view:
    case Tree.ClientView{focus, ops}:
      +outcome = Reducer.overlay(snapshot, ops)
      prior = Equal.trans(Maybe<&2, Tree.TreeError>, Reducer.outcome_error(outcome), Reducer.outcome_error(Reducer.with_focus(outcome, focus)), Some{error}, Equal.sym(Maybe<&2, Tree.TreeError>, Reducer.outcome_error(Reducer.with_focus(outcome, focus)), Reducer.outcome_error(outcome), focus_error(outcome, focus)), e)
      Equal.trans(Tree.TreeSnapshot, Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus)), Reducer.outcome_snapshot(outcome), snapshot, focus_rollback(outcome, focus, error, e), Laws.rejected_overlay_is_atomic(snapshot, ops, error, prior))

law changed_session_result_header:
  for +snapshot: Tree.TreeSnapshot
  for +zipper: Lookup.Zipper<Tree.SessionRecord>
  for +result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>
  {Reducer.result_header(Edit.changed_session_result(snapshot, zipper, result), Reducer.header(snapshot)) == Reducer.header(snapshot) : Reducer.Header}

def changed_session_result_header(snapshot, zipper, result):
  match snapshot result:
    case Tree.TreeSnapshot{_, _, _} Fail{_}:
      {==}
    case Tree.TreeSnapshot{_, _, _} Done{_}:
      {==}

law changed_session_header:
  for +snapshot: Tree.TreeSnapshot
  for +target: Core.StableId
  for +edit: Edit.SessionEdit
  for +found: Maybe<&2, Lookup.Zipper<Tree.SessionRecord>>
  {Reducer.result_header(Edit.changed_session(snapshot, target, edit, found), Reducer.header(snapshot)) == Reducer.header(snapshot) : Reducer.Header}

def changed_session_header(snapshot, target, edit, found):
  match found:
    case None{}:
      {==}
    case Some{zipper}:
      changed_session_result_header(snapshot, zipper, Edit.session_edit(Lookup.zipper_entry(Tree.SessionRecord, zipper), edit))

law root_edit_header:
  for +snapshot: Tree.TreeSnapshot
  for +edit: Edit.RootEdit
  {Reducer.result_header(Edit.root_edit(snapshot, edit), Reducer.header(snapshot)) == Reducer.header(snapshot) : Reducer.Header}

def root_edit_header(snapshot, edit):
  match snapshot edit:
    case Tree.TreeSnapshot{_, _, _} Edit.InsertSession{_}:
      {==}
    case Tree.TreeSnapshot{_, _, _} Edit.DeleteSession{_}:
      {==}
    case Tree.TreeSnapshot{generation, focused, sessions} Edit.ChangeSession{target, edit}:
      changed_session_header(Tree.TreeSnapshot{generation, focused, sessions}, target, edit, Lookup.locate(Tree.SessionRecord, sessions, target))

law step_header:
  for +snapshot: Tree.TreeSnapshot
  for +op: Tree.Op
  {Reducer.result_header(Edit.step(snapshot, op), Reducer.header(snapshot)) == Reducer.header(snapshot) : Reducer.Header}

def step_header(snapshot, op):
  root_edit_header(snapshot, Edit.lower(op))

law step_header_at:
  for +snapshot: Tree.TreeSnapshot
  for +op: Tree.Op
  for +expected: Reducer.Header
  for +e: {Reducer.header(snapshot) == expected : Reducer.Header}
  {Reducer.result_header(Edit.step(snapshot, op), expected) == expected : Reducer.Header}

def step_header_at(snapshot, op, expected, e):
  +step = Edit.step(snapshot, op)
  adjusted = Equal.cong(Reducer.Header, Reducer.Header, h => Reducer.result_header(step, h), Reducer.header(snapshot), expected, e)
  Equal.trans(Reducer.Header, Reducer.result_header(step, expected), Reducer.result_header(step, Reducer.header(snapshot)), expected, Equal.sym(Reducer.Header, Reducer.result_header(step, Reducer.header(snapshot)), Reducer.result_header(step, expected), adjusted), Equal.trans(Reducer.Header, Reducer.result_header(step, Reducer.header(snapshot)), Reducer.header(snapshot), expected, step_header(snapshot, op), e))

law batch_header:
  for +ops: List<&2, Tree.Op>
  for +current: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>
  for +expected: Reducer.Header
  for e: {Reducer.result_header(current, expected) == expected : Reducer.Header}
  {Reducer.result_header(Reducer.batch(ops, current), expected) == expected : Reducer.Header}

def batch_header(ops, current, expected, e):
  match ops current:
    case _ Fail{_}:
      {==}
    case Nil{} Done{_}:
      e
    case head <> tail Done{snapshot}:
      batch_header(tail, Edit.step(snapshot, head), expected, step_header_at(snapshot, head, expected, e))

law canonical_result_header:
  for +result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>
  for +expected: Reducer.Header
  {Reducer.result_header(Reducer.canonical_result(result), expected) == Reducer.result_header(result, expected) : Reducer.Header}

def canonical_result_header(result, expected):
  match result:
    case Fail{_}:
      {==}
    case Done{Tree.TreeSnapshot{_, _, _}}:
      {==}

law finished_header:
  for +snapshot: Tree.TreeSnapshot
  for +result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>
  for e: {Reducer.result_header(result, Reducer.header(snapshot)) == Reducer.header(snapshot) : Reducer.Header}
  {Reducer.header(Reducer.outcome_snapshot(Reducer.finished(snapshot, result))) == Reducer.header(snapshot) : Reducer.Header}

def finished_header(snapshot, result, e):
  match result:
    case Fail{_}:
      {==}
    case Done{_}:
      e

def Laws.overlay_preserves_generation_and_focus(snapshot, ops):
  match ops:
    case Nil{}:
      {==}
    case head <> tail:
      +result = Reducer.batch(head <> tail, Done{snapshot})
      stable = Equal.trans(Reducer.Header, Reducer.result_header(Reducer.canonical_result(result), Reducer.header(snapshot)), Reducer.result_header(result, Reducer.header(snapshot)), Reducer.header(snapshot), canonical_result_header(result, Reducer.header(snapshot)), batch_header(head <> tail, Done{snapshot}, Reducer.header(snapshot), {==}))
      finished_header(snapshot, Reducer.canonical_result(result), stable)

law versioned_generation:
  for +outcome: Tree.Outcome
  for +version: Core.StableId
  for e: {Reducer.accepted(Reducer.versioned(outcome, version)) == True{} : Bool}
  {Reducer.generation(Reducer.outcome_snapshot(Reducer.versioned(outcome, version))) == version : Core.StableId}

def versioned_generation(outcome, version, e):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      Id.absurd({Reducer.generation(Reducer.outcome_snapshot(Reducer.versioned(outcome, version))) == version : Core.StableId}, Id.false_is_not_true(e))
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      {==}

law apply_base_generation:
  for +snapshot: Tree.TreeSnapshot
  for +base: Core.StableId
  for +version: Core.StableId
  for +ops: List<&2, Tree.Op>
  for +same: Bool
  for e: {Reducer.accepted(Reducer.apply_base(snapshot, base, version, ops, same)) == True{} : Bool}
  {Reducer.generation(Reducer.outcome_snapshot(Reducer.apply_base(snapshot, base, version, ops, same))) == version : Core.StableId}

def apply_base_generation(snapshot, base, version, ops, same, e):
  match same:
    case False{}:
      Id.absurd({Reducer.generation(snapshot) == version : Core.StableId}, Id.false_is_not_true(e))
    case True{}:
      versioned_generation(Reducer.overlay(snapshot, ops), version, e)

def Laws.accepted_delta_uses_exact_target_generation(snapshot, base, version, ops, e):
  apply_base_generation(snapshot, base, version, ops, Core.id_equal(Reducer.generation(snapshot), base), e)

law apply_base_alignment:
  for +snapshot: Tree.TreeSnapshot
  for +base: Core.StableId
  for +version: Core.StableId
  for +ops: List<&2, Tree.Op>
  for +same: Bool
  for aligned: {Core.id_equal(Reducer.generation(snapshot), base) == same : Bool}
  for e: {Reducer.accepted(Reducer.apply_base(snapshot, base, version, ops, same)) == True{} : Bool}
  {Reducer.generation(snapshot) == base : Core.StableId}

def apply_base_alignment(snapshot, base, version, ops, same, aligned, e):
  match same:
    case False{}:
      Id.absurd({Reducer.generation(snapshot) == base : Core.StableId}, Id.false_is_not_true(e))
    case True{}:
      Id.id_sound(Reducer.generation(snapshot), base, aligned)

def Laws.accepted_delta_requires_exact_base_generation(snapshot, base, version, ops, e):
  apply_base_alignment(snapshot, base, version, ops, Core.id_equal(Reducer.generation(snapshot), base), {==}, e)

def header_generation(header: Reducer.Header) -> Core.StableId:
  match header:
    case Reducer.Header{generation, _}:
      generation

law headers_imply_generation:
  for +left: Tree.TreeSnapshot
  for +right: Tree.TreeSnapshot
  for e: {Reducer.header(left) == Reducer.header(right) : Reducer.Header}
  {Reducer.generation(left) == Reducer.generation(right) : Core.StableId}

def headers_imply_generation(left, right, e):
  match left right:
    case Tree.TreeSnapshot{lg, lf, _} Tree.TreeSnapshot{rg, rf, _}:
      Equal.cong(Reducer.Header, Core.StableId, h => header_generation(h), Reducer.Header{lg, lf}, Reducer.Header{rg, rf}, e)

law focus_preserves_generation:
  for +outcome: Tree.Outcome
  for +focus: Maybe<&2, Core.StableId>
  {Reducer.generation(Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus))) == Reducer.generation(Reducer.outcome_snapshot(outcome)) : Core.StableId}

def focus_preserves_generation(outcome, focus):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      {==}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      {==}

def Laws.client_view_preserves_generation(snapshot, view):
  match view:
    case Tree.ClientView{focus, ops}:
      +outcome = Reducer.overlay(snapshot, ops)
      Equal.trans(Core.StableId, Reducer.generation(Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus))), Reducer.generation(Reducer.outcome_snapshot(outcome)), Reducer.generation(snapshot), focus_preserves_generation(outcome, focus), headers_imply_generation(Reducer.outcome_snapshot(outcome), snapshot, Laws.overlay_preserves_generation_and_focus(snapshot, ops)))

law focus_success:
  for +outcome: Tree.Outcome
  for +focus: Maybe<&2, Core.StableId>
  for e: {Reducer.accepted(Reducer.with_focus(outcome, focus)) == True{} : Bool}
  {Reducer.focused(Reducer.outcome_snapshot(Reducer.with_focus(outcome, focus))) == focus : Maybe<&2, Core.StableId>}

def focus_success(outcome, focus, e):
  match outcome:
    case Tree.Outcome{Some{_}, _}:
      Id.absurd({Reducer.focused(Reducer.outcome_snapshot(outcome)) == focus : Maybe<&2, Core.StableId>}, Id.false_is_not_true(e))
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, _, _}}:
      {==}

def Laws.accepted_view_uses_exact_focus(snapshot, focus, ops, e):
  focus_success(Reducer.overlay(snapshot, ops), focus, e)

def Laws.windows_have_actual_lexicographic_bit_order(windows):
  SortProof.radix_ordered(Tree.WindowRecord, 96n, 95, windows)

def Laws.equal_window_sort_keys_preserve_complete_value_order(windows, key):
  SortProof.radix_stable(Tree.WindowRecord, 96n, 95, windows, key)

def Laws.sessions_have_actual_lexicographic_id_bit_order(sessions):
  SortProof.radix_ordered(Tree.SessionRecord, 64n, 63, sessions)

def Laws.equal_session_sort_keys_preserve_complete_value_order(sessions, key):
  SortProof.radix_stable(Tree.SessionRecord, 64n, 63, sessions, key)

def Laws.failed_batch_stops_before_later_operations(ops, error):
  match ops:
    case Nil{}:
      {==}
    case _ <> _:
      {==}
