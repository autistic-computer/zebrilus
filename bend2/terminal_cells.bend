import Base
import ./terminal.bend as Terminal

type Cell is Data:
  Cell{glyph: U32, style_flags: U32}

type Cursor is Data:
  Cursor{retained: List<&2, Cell>, changed: List<&2, Cell>, position: Nat, reversed: List<&2, Cell>}

type Outcome is Data:
  Outcome{code: U32, cells: List<&2, Cell>}

type CellPair is Data:
  CellPair{before: Cell, after: Cell}

def prepend_reversed(reversed: List<&2, Cell>, cells: List<&2, Cell>) -> List<&2, Cell>:
  match reversed:
    case Nil{}:
      cells
    case head <> tail:
      prepend_reversed(tail, head <> cells)

def reversed(cells: List<&2, Cell>) -> List<&2, Cell>:
  prepend_reversed(cells, Nil{})

def append(left: List<&2, Cell>, right: List<&2, Cell>) -> List<&2, Cell>:
  prepend_reversed(reversed(left), right)

def take_reversed(cells: List<&2, Cell>, count: Nat, acc: List<&2, Cell>) -> List<&2, Cell>:
  match cells count:
    case _ 0n:
      acc
    case Nil{} 1n+_:
      acc
    case head <> tail 1n+remaining:
      take_reversed(tail, remaining, head <> acc)

def take(cells: List<&2, Cell>, count: Nat) -> List<&2, Cell>:
  reversed(take_reversed(cells, count, Nil{}))

def drop(cells: List<&2, Cell>, count: Nat) -> List<&2, Cell>:
  match cells count:
    case _ 0n:
      cells
    case Nil{} 1n+_:
      Nil{}
    case _ <> tail 1n+remaining:
      drop(tail, remaining)

def length_fold(cells: List<&2, Cell>, count: Nat) -> Nat:
  match cells:
    case Nil{}:
      count
    case _ <> tail:
      length_fold(tail, 1n+count)

def length(cells: List<&2, Cell>) -> Nat:
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
      clear(remaining, Cursor{old, changes, 1n+position, Cell{0, 0} <> out})

def finish(cursor: Cursor) -> List<&2, Cell>:
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

def apply_spans(spans: List<&2, Terminal.Span>, +columns: U32, cursor: Cursor) -> List<&2, Cell>:
  match spans:
    case Nil{}:
      finish(cursor)
    case head <> tail:
      apply_spans(tail, columns, apply_span(cursor, head, columns))

def shifted_nonzero(+cells: List<&2, Cell>, +total: Nat, +amount: Nat, positive: Bool) -> List<&2, Cell>:
  match positive:
    case True{}:
      append(take(cells, amount), take(cells, Nat.sub(total, amount)))
    case False{}:
      append(drop(cells, amount), drop(cells, Nat.sub(total, amount)))

def shifted(+cells: List<&2, Cell>, +columns: U32, rows: U32, +shift: U32, positive: Bool, zero: Bool) -> List<&2, Cell>:
  match zero:
    case True{}:
      cells
    case False{}:
      shifted_nonzero(cells, U32.to_nat(U32.mul(columns, rows)), U32.to_nat(U32.mul(columns, shift)), positive)

def mutate_rows(cells: List<&2, Cell>, changed: List<&2, Cell>, +columns: U32, rows: U32, +shift: U32, positive: Bool, spans: List<&2, Terminal.Span>) -> List<&2, Cell>:
  apply_spans(spans, columns, Cursor{shifted(cells, columns, rows, shift, positive, U32.is_eq(shift, 0)), changed, 0n, Nil{}})

def mutate(cells: List<&2, Cell>, changed: List<&2, Cell>, viewport: Terminal.Viewport, patch: Terminal.Patch) -> List<&2, Cell>:
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{+columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, positive, +shift, _, spans}:
      mutate_rows(cells, changed, columns, rows, shift, positive, spans)

def lengths_valid(cells: List<&2, Cell>, changed: List<&2, Cell>, viewport: Terminal.Viewport, patch: Terminal.Patch) -> Bool:
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, count, _}:
      Nat.is_eq(length(cells), U32.to_nat(U32.mul(columns, rows))) && Nat.is_eq(length(changed), U32.to_nat(count))

def apply_lengths(+cells: List<&2, Cell>, changed: List<&2, Cell>, viewport: Terminal.Viewport, patch: Terminal.Patch, valid: Bool) -> Outcome:
  match valid:
    case False{}:
      Outcome{4, cells}
    case True{}:
      Outcome{0, mutate(cells, changed, viewport, patch)}

def apply_checked(+cells: List<&2, Cell>, +changed: List<&2, Cell>, +viewport: Terminal.Viewport, +patch: Terminal.Patch, code: U32, accepted: Bool) -> Outcome:
  match accepted:
    case False{}:
      Outcome{code, cells}
    case True{}:
      apply_lengths(cells, changed, viewport, patch, lengths_valid(cells, changed, viewport, patch))

def apply(+cells: List<&2, Cell>, +changed: List<&2, Cell>, +viewport: Terminal.Viewport, +patch: Terminal.Patch) -> Outcome:
  +code = Terminal.status(viewport, patch)
  apply_checked(cells, changed, viewport, patch, code, U32.is_eq(code, 0))

def outcome_cells(outcome: Outcome) -> List<&2, Cell>:
  match outcome:
    case Outcome{_, cells}:
      cells

def before(pairs: List<&2, CellPair>, tail: List<&2, Cell>) -> List<&2, Cell>:
  match pairs:
    case Nil{}:
      tail
    case CellPair{old, _} <> rest:
      old <> before(rest, tail)

def after(pairs: List<&2, CellPair>, tail: List<&2, Cell>) -> List<&2, Cell>:
  match pairs:
    case Nil{}:
      tail
    case CellPair{_, new} <> rest:
      new <> after(rest, tail)

def pair_length(pairs: List<&2, CellPair>) -> Nat:
  match pairs:
    case Nil{}:
      0n
    case _ <> rest:
      1n+pair_length(rest)

def prefixed(prefix: List<&2, Cell>, tail: List<&2, Cell>) -> List<&2, Cell>:
  match prefix:
    case Nil{}:
      tail
    case head <> rest:
      head <> prefixed(rest, tail)

def cleared(prefix: List<&2, Cell>, tail: List<&2, Cell>) -> List<&2, Cell>:
  match prefix:
    case Nil{}:
      tail
    case _ <> rest:
      Cell{0, 0} <> cleared(rest, tail)

def prefix_length(prefix: List<&2, Cell>) -> Nat:
  match prefix:
    case Nil{}:
      0n
    case _ <> rest:
      1n+prefix_length(rest)
