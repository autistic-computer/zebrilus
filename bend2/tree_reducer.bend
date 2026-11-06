import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_edit.bend as Edit
import ./tree_list.bend as Lists
import ./tree_sort.bend as Sort

type Header is Data:
  Header{generation: Core.StableId, focused: Maybe<&2, Core.StableId>}

def header(snapshot: Tree.TreeSnapshot) -> Header:
  match snapshot:
    case Tree.TreeSnapshot{generation, focused, _}:
      Header{generation, focused}

def result_header(result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>, fallback: Header) -> Header:
  match result:
    case Fail{_}:
      fallback
    case Done{snapshot}:
      header(snapshot)

def canonical_windows(windows: List<&2, Sort.Tagged<Tree.WindowRecord>>) -> List<&2, Sort.Tagged<Tree.WindowRecord>>:
  Sort.radix(Tree.WindowRecord, 96n, 95, windows)

def canonical_children(sessions: List<&2, Sort.Tagged<Tree.SessionRecord>>, acc: List<&2, Sort.Tagged<Tree.SessionRecord>>) -> List<&2, Sort.Tagged<Tree.SessionRecord>>:
  match sessions:
    case Nil{}:
      Lists.reverse(Sort.Tagged<Tree.SessionRecord>, acc)
    case Sort.Tagged{key, Tree.SessionRecord{name, active, viewers, windows}} <> tail:
      canonical_children(tail, Sort.Tagged{key, Tree.SessionRecord{name, active, viewers, canonical_windows(windows)}} <> acc)

def canonical_sessions(sessions: List<&2, Sort.Tagged<Tree.SessionRecord>>) -> List<&2, Sort.Tagged<Tree.SessionRecord>>:
  Sort.radix(Tree.SessionRecord, 64n, 63, sessions)

def canonical(snapshot: Tree.TreeSnapshot) -> Tree.TreeSnapshot:
  match snapshot:
    case Tree.TreeSnapshot{generation, focused, sessions}:
      Tree.TreeSnapshot{generation, focused, canonical_sessions(canonical_children(sessions, Nil{}))}

def batch(ops: List<&2, Tree.Op>, current: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  match ops current:
    case _ Fail{error}:
      Fail{error}
    case Nil{} Done{snapshot}:
      Done{snapshot}
    case head <> tail Done{snapshot}:
      batch(tail, Edit.step(snapshot, head))

def canonical_result(result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  match result:
    case Fail{error}:
      Fail{error}
    case Done{snapshot}:
      Done{canonical(snapshot)}

def finished(original: Tree.TreeSnapshot, result: Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>) -> Tree.Outcome:
  match result:
    case Fail{error}:
      Tree.Outcome{Some{error}, original}
    case Done{snapshot}:
      Tree.Outcome{None{}, snapshot}

def overlay(+snapshot: Tree.TreeSnapshot, ops: List<&2, Tree.Op>) -> Tree.Outcome:
  match ops:
    case Nil{}:
      Tree.Outcome{None{}, snapshot}
    case head <> tail:
      finished(snapshot, canonical_result(batch(head <> tail, Done{snapshot})))

def generation(snapshot: Tree.TreeSnapshot) -> Core.StableId:
  match snapshot:
    case Tree.TreeSnapshot{generation, _, _}:
      generation

def focused(snapshot: Tree.TreeSnapshot) -> Maybe<&2, Core.StableId>:
  match snapshot:
    case Tree.TreeSnapshot{_, focused, _}:
      focused

def versioned(outcome: Tree.Outcome, version: Core.StableId) -> Tree.Outcome:
  match outcome:
    case Tree.Outcome{Some{error}, snapshot}:
      Tree.Outcome{Some{error}, snapshot}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{_, focused, sessions}}:
      Tree.Outcome{None{}, Tree.TreeSnapshot{version, focused, sessions}}

def apply_base(+snapshot: Tree.TreeSnapshot, base: Core.StableId, version: Core.StableId, ops: List<&2, Tree.Op>, same: Bool) -> Tree.Outcome:
  match same:
    case False{}:
      Tree.Outcome{Some{Tree.BaseMismatch{generation(snapshot), base}}, snapshot}
    case True{}:
      versioned(overlay(snapshot, ops), version)

def apply(+snapshot: Tree.TreeSnapshot, delta: Tree.Delta) -> Tree.Outcome:
  match delta:
    case Tree.Delta{+base, version, ops}:
      apply_base(snapshot, base, version, ops, Core.id_equal(generation(snapshot), base))

def with_focus(outcome: Tree.Outcome, focus: Maybe<&2, Core.StableId>) -> Tree.Outcome:
  match outcome:
    case Tree.Outcome{Some{error}, snapshot}:
      Tree.Outcome{Some{error}, snapshot}
    case Tree.Outcome{None{}, Tree.TreeSnapshot{generation, _, sessions}}:
      Tree.Outcome{None{}, Tree.TreeSnapshot{generation, focus, sessions}}

def client_view(snapshot: Tree.TreeSnapshot, view: Tree.ClientView) -> Tree.Outcome:
  match view:
    case Tree.ClientView{focus, ops}:
      with_focus(overlay(snapshot, ops), focus)

def outcome_snapshot(outcome: Tree.Outcome) -> Tree.TreeSnapshot:
  match outcome:
    case Tree.Outcome{_, snapshot}:
      snapshot

def outcome_error(outcome: Tree.Outcome) -> Maybe<&2, Tree.TreeError>:
  match outcome:
    case Tree.Outcome{error, _}:
      error

def accepted(outcome: Tree.Outcome) -> Bool:
  match outcome:
    case Tree.Outcome{None{}, _}:
      True{}
    case Tree.Outcome{Some{_}, _}:
      False{}
