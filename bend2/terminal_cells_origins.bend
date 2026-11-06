import Base
import ./terminal.bend as Terminal
import ./terminal_cells.bend as Cells

type Origin is Data:
  Retained{index: Nat}
  Incoming{index: Nat}
  Cleared{}

type Cursor is Data:
  Cursor{retained: List<&2, Origin>, changed: List<&2, Origin>, position: Nat, reversed: List<&2, Origin>}

type Outcome is Data:
  Outcome{code: U32, origins: List<&2, Origin>}

type Sources is Data:
  Sources{retained: List<&2, Cells.Cell>, changed: List<&2, Cells.Cell>}

def prepend_reversed(reversed: List<&2, Origin>, cells: List<&2, Origin>) -> List<&2, Origin>:
  match reversed:
    case Nil{}:
      cells
    case head <> tail:
      prepend_reversed(tail, head <> cells)

def reversed(cells: List<&2, Origin>) -> List<&2, Origin>:
  prepend_reversed(cells, Nil{})

def append(left: List<&2, Origin>, right: List<&2, Origin>) -> List<&2, Origin>:
  prepend_reversed(reversed(left), right)

def take_reversed(cells: List<&2, Origin>, count: Nat, acc: List<&2, Origin>) -> List<&2, Origin>:
  match cells count:
    case _ 0n:
      acc
    case Nil{} 1n+_:
      acc
    case head <> tail 1n+remaining:
      take_reversed(tail, remaining, head <> acc)

def take(cells: List<&2, Origin>, count: Nat) -> List<&2, Origin>:
  reversed(take_reversed(cells, count, Nil{}))

def drop(cells: List<&2, Origin>, count: Nat) -> List<&2, Origin>:
  match cells count:
    case _ 0n:
      cells
    case Nil{} 1n+_:
      Nil{}
    case _ <> tail 1n+remaining:
      drop(tail, remaining)

def length_fold(cells: List<&2, Origin>, count: Nat) -> Nat:
  match cells:
    case Nil{}:
      count
    case _ <> tail:
      length_fold(tail, 1n+count)

def length(cells: List<&2, Origin>) -> Nat:
  length_fold(cells, 0n)

def preserve(count: Nat, cursor: Cursor) -> Cursor:
  match count cursor:
    case 0n rest:
      rest
    case 1n+_ Cursor{Nil{}, changes, position, out}:
      Cursor{Nil{}, changes, position, out}
    case 1n+remaining Cursor{head <> tail, changes, position, out}:
      preserve(remaining, Cursor{tail, changes, 1n+position, head <> out})

def replace(count: Nat, cursor: Cursor) -> Cursor:
  match count cursor:
    case 0n rest:
      rest
    case 1n+_ Cursor{Nil{}, changes, position, out}:
      Cursor{Nil{}, changes, position, out}
    case 1n+_ Cursor{old, Nil{}, position, out}:
      Cursor{old, Nil{}, position, out}
    case 1n+remaining Cursor{_ <> old, head <> changes, position, out}:
      replace(remaining, Cursor{old, changes, 1n+position, head <> out})

def clear(count: Nat, cursor: Cursor) -> Cursor:
  match count cursor:
    case 0n rest:
      rest
    case 1n+_ Cursor{Nil{}, changes, position, out}:
      Cursor{Nil{}, changes, position, out}
    case 1n+remaining Cursor{_ <> old, changes, position, out}:
      clear(remaining, Cursor{old, changes, 1n+position, Cleared{} <> out})

def finish(cursor: Cursor) -> List<&2, Origin>:
  match cursor:
    case Cursor{retained, _, _, out}:
      prepend_reversed(out, retained)

def position(cursor: Cursor) -> Nat:
  match cursor:
    case Cursor{_, _, position, _}:
      position

def clear_tail(cursor: Cursor, columns: U32, start: U32, length: U32, clearing: Bool) -> Cursor:
  match clearing:
    case False{}:
      cursor
    case True{}:
      clear(U32.to_nat(U32.sub(U32.sub(columns, start), length)), cursor)

def apply_span(+cursor: Cursor, span: Terminal.Span, +columns: U32) -> Cursor:
  match span:
    case Terminal.Span{row, +start, +length, clearing}:
      clear_tail(replace(U32.to_nat(length), preserve(Nat.sub(U32.to_nat(U32.add(U32.mul(row, columns), start)), position(cursor)), cursor)), columns, start, length, clearing)

def apply_spans(spans: List<&2, Terminal.Span>, +columns: U32, cursor: Cursor) -> List<&2, Origin>:
  match spans:
    case Nil{}:
      finish(cursor)
    case head <> tail:
      apply_spans(tail, columns, apply_span(cursor, head, columns))

def shifted_nonzero(+cells: List<&2, Origin>, +total: Nat, +amount: Nat, positive: Bool) -> List<&2, Origin>:
  match positive:
    case True{}:
      append(take(cells, amount), take(cells, Nat.sub(total, amount)))
    case False{}:
      append(drop(cells, amount), drop(cells, Nat.sub(total, amount)))

def shifted(+cells: List<&2, Origin>, +columns: U32, rows: U32, +shift: U32, positive: Bool, zero: Bool) -> List<&2, Origin>:
  match zero:
    case True{}:
      cells
    case False{}:
      shifted_nonzero(cells, U32.to_nat(U32.mul(columns, rows)), U32.to_nat(U32.mul(columns, shift)), positive)

def mutate_rows(cells: List<&2, Origin>, changed: List<&2, Origin>, +columns: U32, rows: U32, +shift: U32, positive: Bool, spans: List<&2, Terminal.Span>) -> List<&2, Origin>:
  apply_spans(spans, columns, Cursor{shifted(cells, columns, rows, shift, positive, U32.is_eq(shift, 0)), changed, 0n, Nil{}})

def mutate(cells: List<&2, Origin>, changed: List<&2, Origin>, viewport: Terminal.Viewport, patch: Terminal.Patch) -> List<&2, Origin>:
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{+columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, positive, +shift, _, spans}:
      mutate_rows(cells, changed, columns, rows, shift, positive, spans)

def indexed(index: Nat, retained: Bool) -> Origin:
  match retained:
    case True{}:
      Retained{index}
    case False{}:
      Incoming{index}

def enumerated(count: Nat, +start: Nat, +retained: Bool) -> List<&2, Origin>:
  match count:
    case 0n:
      Nil{}
    case 1n+remaining:
      indexed(start, retained) <> enumerated(remaining, 1n+start, retained)

def enumerate_fold(count: Nat, +start: Nat, +retained: Bool, acc: List<&2, Origin>) -> List<&2, Origin>:
  match count:
    case 0n:
      reversed(acc)
    case 1n+remaining:
      enumerate_fold(remaining, 1n+start, retained, indexed(start, retained) <> acc)

def indices(count: Nat, retained: Bool) -> List<&2, Origin>:
  enumerate_fold(count, 0n, retained, Nil{})

def lookup(cells: List<&2, Cells.Cell>, index: Nat) -> Cells.Cell:
  match cells index:
    case Nil{} _:
      Cells.Cell{0, 0}
    case head <> _ 0n:
      head
    case _ <> tail 1n+remaining:
      lookup(tail, remaining)

def origin(origin: Origin, sources: Sources) -> Cells.Cell:
  match origin sources:
    case Retained{index} Sources{retained, _}:
      lookup(retained, index)
    case Incoming{index} Sources{_, changed}:
      lookup(changed, index)
    case Cleared{} _:
      Cells.Cell{0, 0}

def resolved(origins: List<&2, Origin>, +sources: Sources) -> List<&2, Cells.Cell>:
  match origins:
    case Nil{}:
      Nil{}
    case head <> tail:
      origin(head, sources) <> resolved(tail, sources)

def resolve_fold(origins: List<&2, Origin>, +sources: Sources, acc: List<&2, Cells.Cell>) -> List<&2, Cells.Cell>:
  match origins:
    case Nil{}:
      Cells.reversed(acc)
    case head <> tail:
      resolve_fold(tail, sources, origin(head, sources) <> acc)

def resolve(origins: List<&2, Origin>, sources: Sources) -> List<&2, Cells.Cell>:
  resolve_fold(origins, sources, Nil{})

def resolved_cursor(cursor: Cursor, +sources: Sources) -> Cells.Cursor:
  match cursor:
    case Cursor{retained, changed, position, out}:
      Cells.Cursor{resolved(retained, sources), resolved(changed, sources), position, resolved(out, sources)}

def lengths_valid(retained: Nat, changed: Nat, viewport: Terminal.Viewport, patch: Terminal.Patch) -> Bool:
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, count, _}:
      Nat.is_eq(retained, U32.to_nat(U32.mul(columns, rows))) && Nat.is_eq(changed, U32.to_nat(count))

def apply_lengths(+retained: Nat, +changed: Nat, viewport: Terminal.Viewport, patch: Terminal.Patch, valid: Bool) -> Outcome:
  match valid:
    case False{}:
      Outcome{4, indices(retained, True{})}
    case True{}:
      Outcome{0, mutate(indices(retained, True{}), indices(changed, False{}), viewport, patch)}

def apply_checked(+retained: Nat, +changed: Nat, +viewport: Terminal.Viewport, +patch: Terminal.Patch, code: U32, accepted: Bool) -> Outcome:
  match accepted:
    case False{}:
      Outcome{code, indices(retained, True{})}
    case True{}:
      apply_lengths(retained, changed, viewport, patch, lengths_valid(retained, changed, viewport, patch))

def plan(retained: Nat, changed: Nat, +viewport: Terminal.Viewport, +patch: Terminal.Patch) -> Outcome:
  +code = Terminal.status(viewport, patch)
  apply_checked(retained, changed, viewport, patch, code, U32.is_eq(code, 0))

def resolve_outcome(outcome: Outcome, sources: Sources) -> Cells.Outcome:
  match outcome:
    case Outcome{code, origins}:
      Cells.Outcome{code, resolve(origins, sources)}

def in_bounds(origin: Origin, retained: Nat, changed: Nat) -> Bool:
  match origin:
    case Retained{index}:
      Nat.is_lt(index, retained)
    case Incoming{index}:
      Nat.is_lt(index, changed)
    case Cleared{}:
      True{}

def all_in_bounds(origins: List<&2, Origin>, +retained: Nat, +changed: Nat, valid: Bool) -> Bool:
  match origins:
    case Nil{}:
      valid
    case head <> tail:
      all_in_bounds(tail, retained, changed, valid && in_bounds(head, retained, changed))

def cell_count(cells: List<&2, Cells.Cell>) -> Nat:
  match cells:
    case Nil{}:
      0n
    case _ <> tail:
      1n+cell_count(tail)

def source_list(cells: List<&2, Cells.Cell>, other: List<&2, Cells.Cell>, retained: Bool) -> Sources:
  match retained:
    case True{}:
      Sources{cells, other}
    case False{}:
      Sources{other, cells}

def prepended_source(head: Cells.Cell, sources: Sources, retained: Bool) -> Sources:
  match sources retained:
    case Sources{old, changed} True{}:
      Sources{head <> old, changed}
    case Sources{old, changed} False{}:
      Sources{old, head <> changed}

def bumped(origin: Origin, retained: Bool) -> Origin:
  match origin retained:
    case Retained{index} True{}:
      Retained{1n+index}
    case Incoming{index} False{}:
      Incoming{1n+index}
    case rest _:
      rest

def bumped_list(origins: List<&2, Origin>, +retained: Bool) -> List<&2, Origin>:
  match origins:
    case Nil{}:
      Nil{}
    case head <> tail:
      bumped(head, retained) <> bumped_list(tail, retained)

type BoundsPair<-A: Data, -B: Data> is Data:
  BoundsPair{first: A, second: B}

def HasIndex(index: Nat, cells: List<&2, Cells.Cell>) -> Data:
  match index cells:
    case _ Nil{}:
      Empty
    case 0n _ <> _:
      Unit
    case 1n+remaining _ <> tail:
      HasIndex(remaining, tail)

def ValidOrigin(origin: Origin, sources: Sources) -> Data:
  match origin sources:
    case Retained{index} Sources{retained, _}:
      HasIndex(index, retained)
    case Incoming{index} Sources{_, changed}:
      HasIndex(index, changed)
    case Cleared{} _:
      Unit

def ValidOrigins(origins: List<&2, Origin>, +sources: Sources) -> Data:
  match origins:
    case Nil{}:
      Unit
    case head <> tail:
      BoundsPair<ValidOrigin(head, sources), ValidOrigins(tail, sources)>

def ValidCursor(cursor: Cursor, +sources: Sources) -> Data:
  match cursor:
    case Cursor{retained, changed, _, out}:
      BoundsPair<ValidOrigins(retained, sources), BoundsPair<ValidOrigins(changed, sources), ValidOrigins(out, sources)>>

def ValidOutcome(outcome: Outcome, sources: Sources) -> Data:
  match outcome:
    case Outcome{_, origins}:
      ValidOrigins(origins, sources)

def outcome_origins(outcome: Outcome) -> List<&2, Origin>:
  match outcome:
    case Outcome{_, origins}:
      origins

def retained_count(sources: Sources) -> Nat:
  match sources:
    case Sources{retained, _}:
      Cells.length(retained)

def incoming_count(sources: Sources) -> Nat:
  match sources:
    case Sources{_, changed}:
      Cells.length(changed)
