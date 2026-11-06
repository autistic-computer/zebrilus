import Base
import ./id_proof.bend as Id
import ./terminal.bend as Terminal
import ./terminal_cells.bend as Cells
import ./terminal_cells_origins.bend as Origins
import ./terminal_cells_origins_laws.bend as Laws

law resolved_prepend:
  for +left: List<&2, Origins.Origin>
  for +right: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  {Origins.resolved(Origins.prepend_reversed(left, right), sources) == Cells.prepend_reversed(Origins.resolved(left, sources), Origins.resolved(right, sources)) : List<&2, Cells.Cell>}

def resolved_prepend(left, right, sources):
  match left:
    case Nil{}:
      {==}
    case head <> tail:
      resolved_prepend(tail, head <> right, sources)

law resolved_reverse:
  for +values: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  {Origins.resolved(Origins.reversed(values), sources) == Cells.reversed(Origins.resolved(values, sources)) : List<&2, Cells.Cell>}

def resolved_reverse(values, sources):
  resolved_prepend(values, Nil{}, sources)

law resolved_append:
  for +left: List<&2, Origins.Origin>
  for +right: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  {Origins.resolved(Origins.append(left, right), sources) == Cells.append(Origins.resolved(left, sources), Origins.resolved(right, sources)) : List<&2, Cells.Cell>}

def resolved_append(left, right, sources):
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.prepend_reversed(Origins.reversed(left), right), sources), Cells.prepend_reversed(Origins.resolved(Origins.reversed(left), sources), Origins.resolved(right, sources)), resolved_prepend(Origins.reversed(left), right, sources)) : {_ == Cells.append(Origins.resolved(left, sources), Origins.resolved(right, sources)) : List<&2, Cells.Cell>}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.reversed(left), sources), Cells.reversed(Origins.resolved(left, sources)), resolved_reverse(left, sources)) : {Cells.prepend_reversed(_, Origins.resolved(right, sources)) == Cells.append(Origins.resolved(left, sources), Origins.resolved(right, sources)) : List<&2, Cells.Cell>}
  {==}

law resolved_take_reversed:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +acc: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  {Origins.resolved(Origins.take_reversed(values, count, acc), sources) == Cells.take_reversed(Origins.resolved(values, sources), count, Origins.resolved(acc, sources)) : List<&2, Cells.Cell>}

def resolved_take_reversed(values, count, acc, sources):
  match values count:
    case _ 0n:
      {==}
    case Nil{} 1n+_:
      {==}
    case head <> tail 1n+remaining:
      resolved_take_reversed(tail, remaining, head <> acc, sources)

law resolved_take:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +sources: Origins.Sources
  {Origins.resolved(Origins.take(values, count), sources) == Cells.take(Origins.resolved(values, sources), count) : List<&2, Cells.Cell>}

def resolved_take(values, count, sources):
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.reversed(Origins.take_reversed(values, count, Nil{})), sources), Cells.reversed(Origins.resolved(Origins.take_reversed(values, count, Nil{}), sources)), resolved_reverse(Origins.take_reversed(values, count, Nil{}), sources)) : {_ == Cells.take(Origins.resolved(values, sources), count) : List<&2, Cells.Cell>}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.take_reversed(values, count, Nil{}), sources), Cells.take_reversed(Origins.resolved(values, sources), count, Nil{}), resolved_take_reversed(values, count, Nil{}, sources)) : {Cells.reversed(_) == Cells.take(Origins.resolved(values, sources), count) : List<&2, Cells.Cell>}
  {==}

law resolved_drop:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +sources: Origins.Sources
  {Origins.resolved(Origins.drop(values, count), sources) == Cells.drop(Origins.resolved(values, sources), count) : List<&2, Cells.Cell>}

def resolved_drop(values, count, sources):
  match values count:
    case _ 0n:
      {==}
    case Nil{} 1n+_:
      {==}
    case _ <> tail 1n+remaining:
      resolved_drop(tail, remaining, sources)

law resolved_preserve:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.preserve(count, cursor), sources) == Cells.preserve(count, Origins.resolved_cursor(cursor, sources)) : Cells.Cursor}

def resolved_preserve(count, cursor, sources):
  match count cursor:
    case 0n _:
      {==}
    case 1n+_ Origins.Cursor{Nil{}, _, _, _}:
      {==}
    case 1n+remaining Origins.Cursor{head <> tail, changed, position, out}:
      resolved_preserve(remaining, Origins.Cursor{tail, changed, 1n+position, head <> out}, sources)

law resolved_replace:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.replace(count, cursor), sources) == Cells.replace(count, Origins.resolved_cursor(cursor, sources)) : Cells.Cursor}

def resolved_replace(count, cursor, sources):
  match count cursor:
    case 0n _:
      {==}
    case 1n+_ Origins.Cursor{Nil{}, Nil{}, _, _}:
      {==}
    case 1n+_ Origins.Cursor{Nil{}, _ <> _, _, _}:
      {==}
    case 1n+_ Origins.Cursor{_ <> _, Nil{}, _, _}:
      {==}
    case 1n+remaining Origins.Cursor{_ <> old, head <> changed, position, out}:
      resolved_replace(remaining, Origins.Cursor{old, changed, 1n+position, head <> out}, sources)

law resolved_clear:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.clear(count, cursor), sources) == Cells.clear(count, Origins.resolved_cursor(cursor, sources)) : Cells.Cursor}

def resolved_clear(count, cursor, sources):
  match count cursor:
    case 0n _:
      {==}
    case 1n+_ Origins.Cursor{Nil{}, _, _, _}:
      {==}
    case 1n+remaining Origins.Cursor{_ <> old, changed, position, out}:
      resolved_clear(remaining, Origins.Cursor{old, changed, 1n+position, Origins.Cleared{} <> out}, sources)

law resolved_finish:
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  {Origins.resolved(Origins.finish(cursor), sources) == Cells.finish(Origins.resolved_cursor(cursor, sources)) : List<&2, Cells.Cell>}

def resolved_finish(cursor, sources):
  match cursor:
    case Origins.Cursor{retained, _, _, out}:
      resolved_prepend(out, retained, sources)

law resolved_clear_tail:
  for +cursor: Origins.Cursor
  for +columns: U32
  for +start: U32
  for +length: U32
  for +clearing: Bool
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.clear_tail(cursor, columns, start, length, clearing), sources) == Cells.clear_tail(Origins.resolved_cursor(cursor, sources), columns, start, length, clearing) : Cells.Cursor}

def resolved_clear_tail(cursor, columns, start, length, clearing, sources):
  match clearing:
    case False{}:
      {==}
    case True{}:
      resolved_clear(U32.to_nat(U32.sub(U32.sub(columns, start), length)), cursor, sources)

law resolved_edit:
  for +cursor: Origins.Cursor
  for +skip: Nat
  for +count: Nat
  for +columns: U32
  for +start: U32
  for +length: U32
  for +clearing: Bool
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.clear_tail(Origins.replace(count, Origins.preserve(skip, cursor)), columns, start, length, clearing), sources) == Cells.clear_tail(Cells.replace(count, Cells.preserve(skip, Origins.resolved_cursor(cursor, sources))), columns, start, length, clearing) : Cells.Cursor}

def resolved_edit(cursor, skip, count, columns, start, length, clearing, sources):
  +kept = Origins.preserve(skip, cursor)
  +edited = Origins.replace(count, kept)
  %Equal.sym(Cells.Cursor, Origins.resolved_cursor(Origins.clear_tail(edited, columns, start, length, clearing), sources), Cells.clear_tail(Origins.resolved_cursor(edited, sources), columns, start, length, clearing), resolved_clear_tail(edited, columns, start, length, clearing, sources)) : {_ == Cells.clear_tail(Cells.replace(count, Cells.preserve(skip, Origins.resolved_cursor(cursor, sources))), columns, start, length, clearing) : Cells.Cursor}
  %Equal.sym(Cells.Cursor, Origins.resolved_cursor(edited, sources), Cells.replace(count, Origins.resolved_cursor(kept, sources)), resolved_replace(count, kept, sources)) : {Cells.clear_tail(_, columns, start, length, clearing) == Cells.clear_tail(Cells.replace(count, Cells.preserve(skip, Origins.resolved_cursor(cursor, sources))), columns, start, length, clearing) : Cells.Cursor}
  %Equal.sym(Cells.Cursor, Origins.resolved_cursor(kept, sources), Cells.preserve(skip, Origins.resolved_cursor(cursor, sources)), resolved_preserve(skip, cursor, sources)) : {Cells.clear_tail(Cells.replace(count, _), columns, start, length, clearing) == Cells.clear_tail(Cells.replace(count, Cells.preserve(skip, Origins.resolved_cursor(cursor, sources))), columns, start, length, clearing) : Cells.Cursor}
  {==}

law resolved_span:
  for +cursor: Origins.Cursor
  for +span: Terminal.Span
  for +columns: U32
  for +sources: Origins.Sources
  {Origins.resolved_cursor(Origins.apply_span(cursor, span, columns), sources) == Cells.apply_span(Origins.resolved_cursor(cursor, sources), span, columns) : Cells.Cursor}

def resolved_span(cursor, span, columns, sources):
  match cursor span:
    case Origins.Cursor{retained, changed, position, out} Terminal.Span{row, start, length, clearing}:
      resolved_edit(Origins.Cursor{retained, changed, position, out}, Nat.sub(U32.to_nat(U32.add(U32.mul(row, columns), start)), position), U32.to_nat(length), columns, start, length, clearing, sources)

law resolved_spans:
  for +spans: List<&2, Terminal.Span>
  for +columns: U32
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  {Origins.resolved(Origins.apply_spans(spans, columns, cursor), sources) == Cells.apply_spans(spans, columns, Origins.resolved_cursor(cursor, sources)) : List<&2, Cells.Cell>}

def resolved_spans(spans, columns, cursor, sources):
  match spans:
    case Nil{}:
      resolved_finish(cursor, sources)
    case head <> tail:
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.apply_spans(tail, columns, Origins.apply_span(cursor, head, columns)), sources), Cells.apply_spans(tail, columns, Origins.resolved_cursor(Origins.apply_span(cursor, head, columns), sources)), resolved_spans(tail, columns, Origins.apply_span(cursor, head, columns), sources)) : {_ == Cells.apply_spans(head <> tail, columns, Origins.resolved_cursor(cursor, sources)) : List<&2, Cells.Cell>}
      %Equal.sym(Cells.Cursor, Origins.resolved_cursor(Origins.apply_span(cursor, head, columns), sources), Cells.apply_span(Origins.resolved_cursor(cursor, sources), head, columns), resolved_span(cursor, head, columns, sources)) : {Cells.apply_spans(tail, columns, _) == Cells.apply_spans(head <> tail, columns, Origins.resolved_cursor(cursor, sources)) : List<&2, Cells.Cell>}
      {==}

law resolved_shift_nonzero:
  for +values: List<&2, Origins.Origin>
  for +total: Nat
  for +amount: Nat
  for +positive: Bool
  for +sources: Origins.Sources
  {Origins.resolved(Origins.shifted_nonzero(values, total, amount, positive), sources) == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, positive) : List<&2, Cells.Cell>}

def resolved_shift_nonzero(values, total, amount, positive, sources):
  match positive:
    case True{}:
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.append(Origins.take(values, amount), Origins.take(values, Nat.sub(total, amount))), sources), Cells.append(Origins.resolved(Origins.take(values, amount), sources), Origins.resolved(Origins.take(values, Nat.sub(total, amount)), sources)), resolved_append(Origins.take(values, amount), Origins.take(values, Nat.sub(total, amount)), sources)) : {_ == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, True{}) : List<&2, Cells.Cell>}
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.take(values, amount), sources), Cells.take(Origins.resolved(values, sources), amount), resolved_take(values, amount, sources)) : {Cells.append(_, Origins.resolved(Origins.take(values, Nat.sub(total, amount)), sources)) == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, True{}) : List<&2, Cells.Cell>}
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.take(values, Nat.sub(total, amount)), sources), Cells.take(Origins.resolved(values, sources), Nat.sub(total, amount)), resolved_take(values, Nat.sub(total, amount), sources)) : {Cells.append(Cells.take(Origins.resolved(values, sources), amount), _) == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, True{}) : List<&2, Cells.Cell>}
      {==}
    case False{}:
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.append(Origins.drop(values, amount), Origins.drop(values, Nat.sub(total, amount))), sources), Cells.append(Origins.resolved(Origins.drop(values, amount), sources), Origins.resolved(Origins.drop(values, Nat.sub(total, amount)), sources)), resolved_append(Origins.drop(values, amount), Origins.drop(values, Nat.sub(total, amount)), sources)) : {_ == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, False{}) : List<&2, Cells.Cell>}
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.drop(values, amount), sources), Cells.drop(Origins.resolved(values, sources), amount), resolved_drop(values, amount, sources)) : {Cells.append(_, Origins.resolved(Origins.drop(values, Nat.sub(total, amount)), sources)) == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, False{}) : List<&2, Cells.Cell>}
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.drop(values, Nat.sub(total, amount)), sources), Cells.drop(Origins.resolved(values, sources), Nat.sub(total, amount)), resolved_drop(values, Nat.sub(total, amount), sources)) : {Cells.append(Cells.drop(Origins.resolved(values, sources), amount), _) == Cells.shifted_nonzero(Origins.resolved(values, sources), total, amount, False{}) : List<&2, Cells.Cell>}
      {==}

law resolved_shift:
  for +values: List<&2, Origins.Origin>
  for +columns: U32
  for +rows: U32
  for +shift: U32
  for +positive: Bool
  for +zero: Bool
  for +sources: Origins.Sources
  {Origins.resolved(Origins.shifted(values, columns, rows, shift, positive, zero), sources) == Cells.shifted(Origins.resolved(values, sources), columns, rows, shift, positive, zero) : List<&2, Cells.Cell>}

def resolved_shift(values, columns, rows, shift, positive, zero, sources):
  match zero:
    case True{}:
      {==}
    case False{}:
      resolved_shift_nonzero(values, U32.to_nat(U32.mul(columns, rows)), U32.to_nat(U32.mul(columns, shift)), positive, sources)

law resolved_mutate_rows:
  for +retained: List<&2, Origins.Origin>
  for +changed: List<&2, Origins.Origin>
  for +columns: U32
  for +rows: U32
  for +shift: U32
  for +positive: Bool
  for +spans: List<&2, Terminal.Span>
  for +sources: Origins.Sources
  {Origins.resolved(Origins.mutate_rows(retained, changed, columns, rows, shift, positive, spans), sources) == Cells.mutate_rows(Origins.resolved(retained, sources), Origins.resolved(changed, sources), columns, rows, shift, positive, spans) : List<&2, Cells.Cell>}

def resolved_mutate_rows(retained, changed, columns, rows, shift, positive, spans, sources):
  +shifted = Origins.shifted(retained, columns, rows, shift, positive, U32.is_eq(shift, 0))
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.apply_spans(spans, columns, Origins.Cursor{shifted, changed, 0n, Nil{}}), sources), Cells.apply_spans(spans, columns, Origins.resolved_cursor(Origins.Cursor{shifted, changed, 0n, Nil{}}, sources)), resolved_spans(spans, columns, Origins.Cursor{shifted, changed, 0n, Nil{}}, sources)) : {_ == Cells.mutate_rows(Origins.resolved(retained, sources), Origins.resolved(changed, sources), columns, rows, shift, positive, spans) : List<&2, Cells.Cell>}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(shifted, sources), Cells.shifted(Origins.resolved(retained, sources), columns, rows, shift, positive, U32.is_eq(shift, 0)), resolved_shift(retained, columns, rows, shift, positive, U32.is_eq(shift, 0), sources)) : {Cells.apply_spans(spans, columns, Cells.Cursor{_, Origins.resolved(changed, sources), 0n, Nil{}}) == Cells.mutate_rows(Origins.resolved(retained, sources), Origins.resolved(changed, sources), columns, rows, shift, positive, spans) : List<&2, Cells.Cell>}
  {==}

law resolved_mutate:
  for +retained: List<&2, Origins.Origin>
  for +changed: List<&2, Origins.Origin>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +sources: Origins.Sources
  {Origins.resolved(Origins.mutate(retained, changed, viewport, patch), sources) == Cells.mutate(Origins.resolved(retained, sources), Origins.resolved(changed, sources), viewport, patch) : List<&2, Cells.Cell>}

def resolved_mutate(retained, changed, viewport, patch, sources):
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, positive, shift, _, spans}:
      resolved_mutate_rows(retained, changed, columns, rows, shift, positive, spans, sources)

law enumerated_fold_spec:
  for +count: Nat
  for +start: Nat
  for +retained: Bool
  for +acc: List<&2, Origins.Origin>
  {Origins.enumerate_fold(count, start, retained, acc) == Origins.prepend_reversed(acc, Origins.enumerated(count, start, retained)) : List<&2, Origins.Origin>}

def enumerated_fold_spec(count, start, retained, acc):
  match count:
    case 0n:
      {==}
    case 1n+remaining:
      enumerated_fold_spec(remaining, 1n+start, retained, Origins.indexed(start, retained) <> acc)

law indices_spec:
  for +count: Nat
  for +retained: Bool
  {Origins.indices(count, retained) == Origins.enumerated(count, 0n, retained) : List<&2, Origins.Origin>}

def indices_spec(count, retained):
  enumerated_fold_spec(count, 0n, retained, Nil{})

law resolve_fold_spec:
  for +origins: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for +acc: List<&2, Cells.Cell>
  {Origins.resolve_fold(origins, sources, acc) == Cells.prepend_reversed(acc, Origins.resolved(origins, sources)) : List<&2, Cells.Cell>}

def resolve_fold_spec(origins, sources, acc):
  match origins:
    case Nil{}:
      {==}
    case head <> tail:
      resolve_fold_spec(tail, sources, Origins.origin(head, sources) <> acc)

law resolve_spec:
  for +origins: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  {Origins.resolve(origins, sources) == Origins.resolved(origins, sources) : List<&2, Cells.Cell>}

def resolve_spec(origins, sources):
  resolve_fold_spec(origins, sources, Nil{})

law bumped_origin_resolves:
  for +origin: Origins.Origin
  for +head: Cells.Cell
  for +sources: Origins.Sources
  for +retained: Bool
  {Origins.origin(Origins.bumped(origin, retained), Origins.prepended_source(head, sources, retained)) == Origins.origin(origin, sources) : Cells.Cell}

def bumped_origin_resolves(origin, head, sources, retained):
  match origin sources retained:
    case Origins.Retained{_} Origins.Sources{_, _} True{}:
      {==}
    case Origins.Retained{_} Origins.Sources{_, _} False{}:
      {==}
    case Origins.Incoming{_} Origins.Sources{_, _} True{}:
      {==}
    case Origins.Incoming{_} Origins.Sources{_, _} False{}:
      {==}
    case Origins.Cleared{} _ _:
      {==}

law bumped_resolves:
  for +origins: List<&2, Origins.Origin>
  for +head: Cells.Cell
  for +sources: Origins.Sources
  for +retained: Bool
  {Origins.resolved(Origins.bumped_list(origins, retained), Origins.prepended_source(head, sources, retained)) == Origins.resolved(origins, sources) : List<&2, Cells.Cell>}

def bumped_resolves(origins, head, sources, retained):
  match origins:
    case Nil{}:
      {==}
    case first <> tail:
      %Equal.sym(Cells.Cell, Origins.origin(Origins.bumped(first, retained), Origins.prepended_source(head, sources, retained)), Origins.origin(first, sources), bumped_origin_resolves(first, head, sources, retained)) : {_ <> Origins.resolved(Origins.bumped_list(tail, retained), Origins.prepended_source(head, sources, retained)) == Origins.origin(first, sources) <> Origins.resolved(tail, sources) : List<&2, Cells.Cell>}
      %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.bumped_list(tail, retained), Origins.prepended_source(head, sources, retained)), Origins.resolved(tail, sources), bumped_resolves(tail, head, sources, retained)) : {Origins.origin(first, sources) <> _ == Origins.origin(first, sources) <> Origins.resolved(tail, sources) : List<&2, Cells.Cell>}
      {==}

law bumped_enumerated:
  for +count: Nat
  for +start: Nat
  for +retained: Bool
  {Origins.bumped_list(Origins.enumerated(count, start, retained), retained) == Origins.enumerated(count, 1n+start, retained) : List<&2, Origins.Origin>}

def bumped_enumerated(count, start, retained):
  match count retained:
    case 0n _:
      {==}
    case 1n+remaining True{}:
      %bumped_enumerated(remaining, 1n+start, True{}) : {Origins.Retained{1n+start} <> Origins.bumped_list(Origins.enumerated(remaining, 1n+start, True{}), True{}) == Origins.Retained{1n+start} <> _ : List<&2, Origins.Origin>}
      {==}
    case 1n+remaining False{}:
      %bumped_enumerated(remaining, 1n+start, False{}) : {Origins.Incoming{1n+start} <> Origins.bumped_list(Origins.enumerated(remaining, 1n+start, False{}), False{}) == Origins.Incoming{1n+start} <> _ : List<&2, Origins.Origin>}
      {==}

law enumerated_successor_resolves:
  for +count: Nat
  for +head: Cells.Cell
  for +sources: Origins.Sources
  for +retained: Bool
  {Origins.resolved(Origins.enumerated(count, 1n, retained), Origins.prepended_source(head, sources, retained)) == Origins.resolved(Origins.enumerated(count, 0n, retained), sources) : List<&2, Cells.Cell>}

def enumerated_successor_resolves(count, head, sources, retained):
  %bumped_enumerated(count, 0n, retained) : {Origins.resolved(_, Origins.prepended_source(head, sources, retained)) == Origins.resolved(Origins.enumerated(count, 0n, retained), sources) : List<&2, Cells.Cell>}
  bumped_resolves(Origins.enumerated(count, 0n, retained), head, sources, retained)

law enumerated_cells_resolve:
  for +cells: List<&2, Cells.Cell>
  for +other: List<&2, Cells.Cell>
  for +retained: Bool
  {Origins.resolved(Origins.enumerated(Origins.cell_count(cells), 0n, retained), Origins.source_list(cells, other, retained)) == cells : List<&2, Cells.Cell>}

def enumerated_cells_resolve(cells, other, retained):
  match cells retained:
    case Nil{} _:
      {==}
    case head <> tail True{}:
      Equal.cong(List<&2, Cells.Cell>, List<&2, Cells.Cell>, value => head <> value, Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 1n, True{}), Origins.Sources{head <> tail, other}), tail,
        Equal.trans(List<&2, Cells.Cell>, Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 1n, True{}), Origins.Sources{head <> tail, other}), Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 0n, True{}), Origins.Sources{tail, other}), tail,
          enumerated_successor_resolves(Origins.cell_count(tail), head, Origins.Sources{tail, other}, True{}), enumerated_cells_resolve(tail, other, True{})))
    case head <> tail False{}:
      Equal.cong(List<&2, Cells.Cell>, List<&2, Cells.Cell>, value => head <> value, Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 1n, False{}), Origins.Sources{other, head <> tail}), tail,
        Equal.trans(List<&2, Cells.Cell>, Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 1n, False{}), Origins.Sources{other, head <> tail}), Origins.resolved(Origins.enumerated(Origins.cell_count(tail), 0n, False{}), Origins.Sources{other, tail}), tail,
          enumerated_successor_resolves(Origins.cell_count(tail), head, Origins.Sources{other, tail}, False{}), enumerated_cells_resolve(tail, other, False{})))

law add_right_successor:
  for +left: Nat
  for +right: Nat
  {Nat.add(left, 1n+right) == 1n+Nat.add(left, right) : Nat}

def add_right_successor(left, right):
  match left:
    case 0n:
      {==}
    case 1n+remaining:
      %add_right_successor(remaining, right) : {1n+Nat.add(remaining, 1n+right) == 1n+_ : Nat}
      {==}

law add_right_zero:
  for +value: Nat
  {Nat.add(value, 0n) == value : Nat}

def add_right_zero(value):
  match value:
    case 0n:
      {==}
    case 1n+remaining:
      %add_right_zero(remaining) : {1n+Nat.add(remaining, 0n) == 1n+_ : Nat}
      {==}

law cell_length_fold:
  for +cells: List<&2, Cells.Cell>
  for +count: Nat
  {Cells.length_fold(cells, count) == Nat.add(Origins.cell_count(cells), count) : Nat}

def cell_length_fold(cells, count):
  match cells:
    case Nil{}:
      {==}
    case _ <> tail:
      %Equal.sym(Nat, Cells.length_fold(tail, 1n+count), Nat.add(Origins.cell_count(tail), 1n+count), cell_length_fold(tail, 1n+count)) : {_ == 1n+Nat.add(Origins.cell_count(tail), count) : Nat}
      add_right_successor(Origins.cell_count(tail), count)

law cell_length:
  for +cells: List<&2, Cells.Cell>
  {Cells.length(cells) == Origins.cell_count(cells) : Nat}

def cell_length(cells):
  Equal.trans(Nat, Cells.length(cells), Nat.add(Origins.cell_count(cells), 0n), Origins.cell_count(cells), cell_length_fold(cells, 0n), add_right_zero(Origins.cell_count(cells)))

law indexed_cells_resolve:
  for +cells: List<&2, Cells.Cell>
  for +other: List<&2, Cells.Cell>
  for +retained: Bool
  {Origins.resolved(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained)) == cells : List<&2, Cells.Cell>}

def indexed_cells_resolve(cells, other, retained):
  %Equal.sym(List<&2, Origins.Origin>, Origins.indices(Cells.length(cells), retained), Origins.enumerated(Cells.length(cells), 0n, retained), indices_spec(Cells.length(cells), retained)) : {Origins.resolved(_, Origins.source_list(cells, other, retained)) == cells : List<&2, Cells.Cell>}
  %Equal.sym(Nat, Cells.length(cells), Origins.cell_count(cells), cell_length(cells)) : {Origins.resolved(Origins.enumerated(_, 0n, retained), Origins.source_list(cells, other, retained)) == cells : List<&2, Cells.Cell>}
  enumerated_cells_resolve(cells, other, retained)


law indexed_mutate:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Origins.resolved(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}) == Cells.mutate(cells, changed, viewport, patch) : List<&2, Cells.Cell>}

def indexed_mutate(cells, changed, viewport, patch):
  +retained = Origins.indices(Cells.length(cells), True{})
  +incoming = Origins.indices(Cells.length(changed), False{})
  +sources = {Origins.Sources{cells, changed} : Origins.Sources}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(Origins.mutate(retained, incoming, viewport, patch), sources), Cells.mutate(Origins.resolved(retained, sources), Origins.resolved(incoming, sources), viewport, patch), resolved_mutate(retained, incoming, viewport, patch, sources)) : {_ == Cells.mutate(cells, changed, viewport, patch) : List<&2, Cells.Cell>}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(retained, sources), cells, indexed_cells_resolve(cells, changed, True{})) : {Cells.mutate(_, Origins.resolved(incoming, sources), viewport, patch) == Cells.mutate(cells, changed, viewport, patch) : List<&2, Cells.Cell>}
  %Equal.sym(List<&2, Cells.Cell>, Origins.resolved(incoming, sources), changed, indexed_cells_resolve(changed, cells, False{})) : {Cells.mutate(cells, _, viewport, patch) == Cells.mutate(cells, changed, viewport, patch) : List<&2, Cells.Cell>}
  {==}

law indices_resolve:
  for +cells: List<&2, Cells.Cell>
  for +other: List<&2, Cells.Cell>
  for +retained: Bool
  {Origins.resolve(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained)) == cells : List<&2, Cells.Cell>}

def indices_resolve(cells, other, retained):
  Equal.trans(List<&2, Cells.Cell>, Origins.resolve(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained)), Origins.resolved(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained)), cells, resolve_spec(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained)), indexed_cells_resolve(cells, other, retained))

law indexed_mutate_resolve:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Origins.resolve(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}) == Cells.mutate(cells, changed, viewport, patch) : List<&2, Cells.Cell>}

def indexed_mutate_resolve(cells, changed, viewport, patch):
  Equal.trans(List<&2, Cells.Cell>, Origins.resolve(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}), Origins.resolved(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}), Cells.mutate(cells, changed, viewport, patch), resolve_spec(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}), indexed_mutate(cells, changed, viewport, patch))

def Laws.full_mutation_has_exact_source_indices(cells, changed, viewport, patch):
  Equal.sym(List<&2, Cells.Cell>, Origins.resolve(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}), Cells.mutate(cells, changed, viewport, patch), indexed_mutate_resolve(cells, changed, viewport, patch))

law lengths_refinement:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +valid: Bool
  {Cells.apply_lengths(cells, changed, viewport, patch, valid) == Origins.resolve_outcome(Origins.apply_lengths(Cells.length(cells), Cells.length(changed), viewport, patch, valid), Origins.Sources{cells, changed}) : Cells.Outcome}

def lengths_refinement(cells, changed, viewport, patch, valid):
  match valid:
    case False{}:
      Equal.cong(List<&2, Cells.Cell>, Cells.Outcome, value => Cells.Outcome{4, value}, cells, Origins.resolve(Origins.indices(Cells.length(cells), True{}), Origins.Sources{cells, changed}), Equal.sym(List<&2, Cells.Cell>, Origins.resolve(Origins.indices(Cells.length(cells), True{}), Origins.Sources{cells, changed}), cells, indices_resolve(cells, changed, True{})))
    case True{}:
      Equal.cong(List<&2, Cells.Cell>, Cells.Outcome, value => Cells.Outcome{0, value}, Cells.mutate(cells, changed, viewport, patch), Origins.resolve(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}), Laws.full_mutation_has_exact_source_indices(cells, changed, viewport, patch))

law lengths_valid_align:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Cells.lengths_valid(cells, changed, viewport, patch) == Origins.lengths_valid(Cells.length(cells), Cells.length(changed), viewport, patch) : Bool}

def lengths_valid_align(cells, changed, viewport, patch):
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{_, _}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, _, _}:
      {==}

law checked_refinement:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +code: U32
  for +accepted: Bool
  {Cells.apply_checked(cells, changed, viewport, patch, code, accepted) == Origins.resolve_outcome(Origins.apply_checked(Cells.length(cells), Cells.length(changed), viewport, patch, code, accepted), Origins.Sources{cells, changed}) : Cells.Outcome}

def checked_refinement(cells, changed, viewport, patch, code, accepted):
  match accepted:
    case False{}:
      Equal.cong(List<&2, Cells.Cell>, Cells.Outcome, value => Cells.Outcome{code, value}, cells, Origins.resolve(Origins.indices(Cells.length(cells), True{}), Origins.Sources{cells, changed}), Equal.sym(List<&2, Cells.Cell>, Origins.resolve(Origins.indices(Cells.length(cells), True{}), Origins.Sources{cells, changed}), cells, indices_resolve(cells, changed, True{})))
    case True{}:
      %lengths_valid_align(cells, changed, viewport, patch) : {Cells.apply_lengths(cells, changed, viewport, patch, Cells.lengths_valid(cells, changed, viewport, patch)) == Origins.resolve_outcome(Origins.apply_lengths(Cells.length(cells), Cells.length(changed), viewport, patch, _), Origins.Sources{cells, changed}) : Cells.Outcome}
      lengths_refinement(cells, changed, viewport, patch, Cells.lengths_valid(cells, changed, viewport, patch))

def Laws.complete_patch_has_exact_source_indices_and_rollback(cells, changed, viewport, patch):
  checked_refinement(cells, changed, viewport, patch, Terminal.status(viewport, patch), U32.is_eq(Terminal.status(viewport, patch), 0))

law bumped_origin_valid:
  for +origin: Origins.Origin
  for +head: Cells.Cell
  for +sources: Origins.Sources
  for +retained: Bool
  for e: Origins.ValidOrigin(origin, sources)
  Origins.ValidOrigin(Origins.bumped(origin, retained), Origins.prepended_source(head, sources, retained))

def bumped_origin_valid(origin, head, sources, retained, e):
  match origin sources retained:
    case Origins.Retained{_} Origins.Sources{_, _} True{}:
      e
    case Origins.Retained{_} Origins.Sources{_, _} False{}:
      e
    case Origins.Incoming{_} Origins.Sources{_, _} True{}:
      e
    case Origins.Incoming{_} Origins.Sources{_, _} False{}:
      e
    case Origins.Cleared{} _ _:
      e

law bumped_list_valid:
  for +origins: List<&2, Origins.Origin>
  for +head: Cells.Cell
  for +sources: Origins.Sources
  for +retained: Bool
  for e: Origins.ValidOrigins(origins, sources)
  Origins.ValidOrigins(Origins.bumped_list(origins, retained), Origins.prepended_source(head, sources, retained))

def bumped_list_valid(origins, head, sources, retained, e):
  match origins e:
    case Nil{} _:
      Unit{}
    case first <> tail Origins.BoundsPair{first_valid, tail_valid}:
      Origins.BoundsPair{bumped_origin_valid(first, head, sources, retained, first_valid), bumped_list_valid(tail, head, sources, retained, tail_valid)}

law enumerated_valid:
  for +cells: List<&2, Cells.Cell>
  for +other: List<&2, Cells.Cell>
  for +retained: Bool
  Origins.ValidOrigins(Origins.enumerated(Origins.cell_count(cells), 0n, retained), Origins.source_list(cells, other, retained))

def enumerated_valid(cells, other, retained):
  match cells retained:
    case Nil{} _:
      Unit{}
    case head <> tail True{}:
      %bumped_enumerated(Origins.cell_count(tail), 0n, True{}) : Origins.BoundsPair<Unit, Origins.ValidOrigins(_, Origins.Sources{head <> tail, other})>
      Origins.BoundsPair{Unit{}, bumped_list_valid(Origins.enumerated(Origins.cell_count(tail), 0n, True{}), head, Origins.Sources{tail, other}, True{}, enumerated_valid(tail, other, True{}))}
    case head <> tail False{}:
      %bumped_enumerated(Origins.cell_count(tail), 0n, False{}) : Origins.BoundsPair<Unit, Origins.ValidOrigins(_, Origins.Sources{other, head <> tail})>
      Origins.BoundsPair{Unit{}, bumped_list_valid(Origins.enumerated(Origins.cell_count(tail), 0n, False{}), head, Origins.Sources{other, tail}, False{}, enumerated_valid(tail, other, False{}))}

law indices_valid:
  for +cells: List<&2, Cells.Cell>
  for +other: List<&2, Cells.Cell>
  for +retained: Bool
  Origins.ValidOrigins(Origins.indices(Cells.length(cells), retained), Origins.source_list(cells, other, retained))

def indices_valid(cells, other, retained):
  %Equal.sym(List<&2, Origins.Origin>, Origins.indices(Cells.length(cells), retained), Origins.enumerated(Cells.length(cells), 0n, retained), indices_spec(Cells.length(cells), retained)) : Origins.ValidOrigins(_, Origins.source_list(cells, other, retained))
  %Equal.sym(Nat, Cells.length(cells), Origins.cell_count(cells), cell_length(cells)) : Origins.ValidOrigins(Origins.enumerated(_, 0n, retained), Origins.source_list(cells, other, retained))
  enumerated_valid(cells, other, retained)

law valid_prepend:
  for +left: List<&2, Origins.Origin>
  for +right: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for left_valid: Origins.ValidOrigins(left, sources)
  for right_valid: Origins.ValidOrigins(right, sources)
  Origins.ValidOrigins(Origins.prepend_reversed(left, right), sources)

def valid_prepend(left, right, sources, left_valid, right_valid):
  match left left_valid:
    case Nil{} _:
      right_valid
    case head <> tail Origins.BoundsPair{head_valid, tail_valid}:
      valid_prepend(tail, head <> right, sources, tail_valid, Origins.BoundsPair{head_valid, right_valid})

law valid_reverse:
  for +values: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for e: Origins.ValidOrigins(values, sources)
  Origins.ValidOrigins(Origins.reversed(values), sources)

def valid_reverse(values, sources, e):
  valid_prepend(values, Nil{}, sources, e, Unit{})

law valid_append:
  for +left: List<&2, Origins.Origin>
  for +right: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for left_valid: Origins.ValidOrigins(left, sources)
  for right_valid: Origins.ValidOrigins(right, sources)
  Origins.ValidOrigins(Origins.append(left, right), sources)

def valid_append(left, right, sources, left_valid, right_valid):
  valid_prepend(Origins.reversed(left), right, sources, valid_reverse(left, sources, left_valid), right_valid)

law valid_take_reversed:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +acc: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for values_valid: Origins.ValidOrigins(values, sources)
  for acc_valid: Origins.ValidOrigins(acc, sources)
  Origins.ValidOrigins(Origins.take_reversed(values, count, acc), sources)

def valid_take_reversed(values, count, acc, sources, values_valid, acc_valid):
  match values count values_valid:
    case _ 0n _:
      acc_valid
    case Nil{} 1n+_ _:
      acc_valid
    case head <> tail 1n+remaining Origins.BoundsPair{head_valid, tail_valid}:
      valid_take_reversed(tail, remaining, head <> acc, sources, tail_valid, Origins.BoundsPair{head_valid, acc_valid})

law valid_take:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +sources: Origins.Sources
  for e: Origins.ValidOrigins(values, sources)
  Origins.ValidOrigins(Origins.take(values, count), sources)

def valid_take(values, count, sources, e):
  valid_reverse(Origins.take_reversed(values, count, Nil{}), sources, valid_take_reversed(values, count, Nil{}, sources, e, Unit{}))

law valid_drop:
  for +values: List<&2, Origins.Origin>
  for +count: Nat
  for +sources: Origins.Sources
  for e: Origins.ValidOrigins(values, sources)
  Origins.ValidOrigins(Origins.drop(values, count), sources)

def valid_drop(values, count, sources, e):
  match values count e:
    case _ 0n rest:
      rest
    case Nil{} 1n+_ _:
      Unit{}
    case _ <> tail 1n+remaining Origins.BoundsPair{_, tail_valid}:
      valid_drop(tail, remaining, sources, tail_valid)

law valid_preserve:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidCursor(Origins.preserve(count, cursor), sources)

def valid_preserve(count, cursor, sources, e):
  match count cursor e:
    case 0n _ rest:
      rest
    case 1n+_ Origins.Cursor{Nil{}, _, _, _} rest:
      rest
    case 1n+remaining Origins.Cursor{head <> tail, changed, position, out} Origins.BoundsPair{Origins.BoundsPair{head_valid, tail_valid}, Origins.BoundsPair{changed_valid, out_valid}}:
      valid_preserve(remaining, Origins.Cursor{tail, changed, 1n+position, head <> out}, sources, Origins.BoundsPair{tail_valid, Origins.BoundsPair{changed_valid, Origins.BoundsPair{head_valid, out_valid}}})

law valid_replace:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidCursor(Origins.replace(count, cursor), sources)

def valid_replace(count, cursor, sources, e):
  match count cursor e:
    case 0n _ rest:
      rest
    case 1n+_ Origins.Cursor{Nil{}, Nil{}, _, _} rest:
      rest
    case 1n+_ Origins.Cursor{Nil{}, _ <> _, _, _} rest:
      rest
    case 1n+_ Origins.Cursor{_ <> _, Nil{}, _, _} rest:
      rest
    case 1n+remaining Origins.Cursor{_ <> old, head <> changed, position, out} Origins.BoundsPair{Origins.BoundsPair{_, old_valid}, Origins.BoundsPair{Origins.BoundsPair{head_valid, changed_valid}, out_valid}}:
      valid_replace(remaining, Origins.Cursor{old, changed, 1n+position, head <> out}, sources, Origins.BoundsPair{old_valid, Origins.BoundsPair{changed_valid, Origins.BoundsPair{head_valid, out_valid}}})

law valid_clear:
  for +count: Nat
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidCursor(Origins.clear(count, cursor), sources)

def valid_clear(count, cursor, sources, e):
  match count cursor e:
    case 0n _ rest:
      rest
    case 1n+_ Origins.Cursor{Nil{}, _, _, _} rest:
      rest
    case 1n+remaining Origins.Cursor{_ <> old, changed, position, out} Origins.BoundsPair{Origins.BoundsPair{_, old_valid}, Origins.BoundsPair{changed_valid, out_valid}}:
      valid_clear(remaining, Origins.Cursor{old, changed, 1n+position, Origins.Cleared{} <> out}, sources, Origins.BoundsPair{old_valid, Origins.BoundsPair{changed_valid, Origins.BoundsPair{Unit{}, out_valid}}})

law valid_finish:
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidOrigins(Origins.finish(cursor), sources)

def valid_finish(cursor, sources, e):
  match cursor e:
    case Origins.Cursor{retained, _, _, out} Origins.BoundsPair{retained_valid, Origins.BoundsPair{_, out_valid}}:
      valid_prepend(out, retained, sources, out_valid, retained_valid)

law valid_clear_tail:
  for +cursor: Origins.Cursor
  for +columns: U32
  for +start: U32
  for +length: U32
  for +clearing: Bool
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidCursor(Origins.clear_tail(cursor, columns, start, length, clearing), sources)

def valid_clear_tail(cursor, columns, start, length, clearing, sources, e):
  match clearing:
    case False{}:
      e
    case True{}:
      valid_clear(U32.to_nat(U32.sub(U32.sub(columns, start), length)), cursor, sources, e)

law valid_span:
  for +cursor: Origins.Cursor
  for +span: Terminal.Span
  for +columns: U32
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidCursor(Origins.apply_span(cursor, span, columns), sources)

def valid_span(cursor, span, columns, sources, e):
  match span:
    case Terminal.Span{row, start, length, clearing}:
      +skip = Nat.sub(U32.to_nat(U32.add(U32.mul(row, columns), start)), Origins.position(cursor))
      +kept = Origins.preserve(skip, cursor)
      valid_clear_tail(Origins.replace(U32.to_nat(length), kept), columns, start, length, clearing, sources,
        valid_replace(U32.to_nat(length), kept, sources, valid_preserve(skip, cursor, sources, e)))

law valid_spans:
  for +spans: List<&2, Terminal.Span>
  for +columns: U32
  for +cursor: Origins.Cursor
  for +sources: Origins.Sources
  for e: Origins.ValidCursor(cursor, sources)
  Origins.ValidOrigins(Origins.apply_spans(spans, columns, cursor), sources)

def valid_spans(spans, columns, cursor, sources, e):
  match spans:
    case Nil{}:
      valid_finish(cursor, sources, e)
    case head <> tail:
      valid_spans(tail, columns, Origins.apply_span(cursor, head, columns), sources, valid_span(cursor, head, columns, sources, e))

law valid_shift_nonzero:
  for +values: List<&2, Origins.Origin>
  for +total: Nat
  for +amount: Nat
  for +positive: Bool
  for +sources: Origins.Sources
  for +e: Origins.ValidOrigins(values, sources)
  Origins.ValidOrigins(Origins.shifted_nonzero(values, total, amount, positive), sources)

def valid_shift_nonzero(values, total, amount, positive, sources, e):
  match positive:
    case True{}:
      valid_append(Origins.take(values, amount), Origins.take(values, Nat.sub(total, amount)), sources, valid_take(values, amount, sources, e), valid_take(values, Nat.sub(total, amount), sources, e))
    case False{}:
      valid_append(Origins.drop(values, amount), Origins.drop(values, Nat.sub(total, amount)), sources, valid_drop(values, amount, sources, e), valid_drop(values, Nat.sub(total, amount), sources, e))

law valid_shift:
  for +values: List<&2, Origins.Origin>
  for +columns: U32
  for +rows: U32
  for +shift: U32
  for +positive: Bool
  for +zero: Bool
  for +sources: Origins.Sources
  for +e: Origins.ValidOrigins(values, sources)
  Origins.ValidOrigins(Origins.shifted(values, columns, rows, shift, positive, zero), sources)

def valid_shift(values, columns, rows, shift, positive, zero, sources, e):
  match zero:
    case True{}:
      e
    case False{}:
      valid_shift_nonzero(values, U32.to_nat(U32.mul(columns, rows)), U32.to_nat(U32.mul(columns, shift)), positive, sources, e)

law valid_mutate_rows:
  for +retained: List<&2, Origins.Origin>
  for +changed: List<&2, Origins.Origin>
  for +columns: U32
  for +rows: U32
  for +shift: U32
  for +positive: Bool
  for +spans: List<&2, Terminal.Span>
  for +sources: Origins.Sources
  for retained_valid: Origins.ValidOrigins(retained, sources)
  for changed_valid: Origins.ValidOrigins(changed, sources)
  Origins.ValidOrigins(Origins.mutate_rows(retained, changed, columns, rows, shift, positive, spans), sources)

def valid_mutate_rows(retained, changed, columns, rows, shift, positive, spans, sources, retained_valid, changed_valid):
  valid_spans(spans, columns, Origins.Cursor{Origins.shifted(retained, columns, rows, shift, positive, U32.is_eq(shift, 0)), changed, 0n, Nil{}}, sources,
    Origins.BoundsPair{valid_shift(retained, columns, rows, shift, positive, U32.is_eq(shift, 0), sources, retained_valid), Origins.BoundsPair{changed_valid, Unit{}}})

law valid_mutate:
  for +retained: List<&2, Origins.Origin>
  for +changed: List<&2, Origins.Origin>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +sources: Origins.Sources
  for retained_valid: Origins.ValidOrigins(retained, sources)
  for changed_valid: Origins.ValidOrigins(changed, sources)
  Origins.ValidOrigins(Origins.mutate(retained, changed, viewport, patch), sources)

def valid_mutate(retained, changed, viewport, patch, sources, retained_valid, changed_valid):
  match viewport patch:
    case Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _} Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, positive, shift, _, spans}:
      valid_mutate_rows(retained, changed, columns, rows, shift, positive, spans, sources, retained_valid, changed_valid)

law lengths_valid_origins:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +valid: Bool
  Origins.ValidOutcome(Origins.apply_lengths(Cells.length(cells), Cells.length(changed), viewport, patch, valid), Origins.Sources{cells, changed})

def lengths_valid_origins(cells, changed, viewport, patch, valid):
  match valid:
    case False{}:
      indices_valid(cells, changed, True{})
    case True{}:
      valid_mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch, Origins.Sources{cells, changed}, indices_valid(cells, changed, True{}), indices_valid(changed, cells, False{}))

law checked_valid_origins:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +code: U32
  for +accepted: Bool
  Origins.ValidOutcome(Origins.apply_checked(Cells.length(cells), Cells.length(changed), viewport, patch, code, accepted), Origins.Sources{cells, changed})

def checked_valid_origins(cells, changed, viewport, patch, code, accepted):
  match accepted:
    case False{}:
      indices_valid(cells, changed, True{})
    case True{}:
      lengths_valid_origins(cells, changed, viewport, patch, Origins.lengths_valid(Cells.length(cells), Cells.length(changed), viewport, patch))

def Laws.every_planned_source_index_exists(cells, changed, viewport, patch):
  checked_valid_origins(cells, changed, viewport, patch, Terminal.status(viewport, patch), U32.is_eq(Terminal.status(viewport, patch), 0))


law has_index_is_lt:
  for +index: Nat
  for +cells: List<&2, Cells.Cell>
  for e: Origins.HasIndex(index, cells)
  {Nat.is_lt(index, Origins.cell_count(cells)) == True{} : Bool}

def has_index_is_lt(index, cells, e):
  match index cells:
    case actual Nil{}:
      Id.absurd({Nat.is_lt(actual, 0n) == True{} : Bool}, e)
    case 0n _ <> _:
      {==}
    case 1n+remaining _ <> tail:
      has_index_is_lt(remaining, tail, e)

law valid_origin_bounds:
  for +origin: Origins.Origin
  for +sources: Origins.Sources
  for e: Origins.ValidOrigin(origin, sources)
  {Origins.in_bounds(origin, Origins.retained_count(sources), Origins.incoming_count(sources)) == True{} : Bool}

def valid_origin_bounds(origin, sources, e):
  match origin sources:
    case Origins.Retained{index} Origins.Sources{retained, _}:
      %Equal.sym(Nat, Cells.length(retained), Origins.cell_count(retained), cell_length(retained)) : {Nat.is_lt(index, _) == True{} : Bool}
      has_index_is_lt(index, retained, e)
    case Origins.Incoming{index} Origins.Sources{_, changed}:
      %Equal.sym(Nat, Cells.length(changed), Origins.cell_count(changed), cell_length(changed)) : {Nat.is_lt(index, _) == True{} : Bool}
      has_index_is_lt(index, changed, e)
    case Origins.Cleared{} _:
      {==}

law bool_and_true:
  for +value: Bool
  {value && True{} == value : Bool}

def bool_and_true(value):
  match value:
    case False{}:
      {==}
    case True{}:
      {==}

law valid_all_bounds_fold:
  for +origins: List<&2, Origins.Origin>
  for +sources: Origins.Sources
  for +valid: Bool
  for e: Origins.ValidOrigins(origins, sources)
  {Origins.all_in_bounds(origins, Origins.retained_count(sources), Origins.incoming_count(sources), valid) == valid : Bool}

def valid_all_bounds_fold(origins, sources, valid, e):
  match origins e:
    case Nil{} _:
      {==}
    case head <> tail Origins.BoundsPair{head_valid, tail_valid}:
      %Equal.sym(Bool, Origins.in_bounds(head, Origins.retained_count(sources), Origins.incoming_count(sources)), True{}, valid_origin_bounds(head, sources, head_valid)) : {Origins.all_in_bounds(tail, Origins.retained_count(sources), Origins.incoming_count(sources), valid && _) == valid : Bool}
      %Equal.sym(Bool, valid && True{}, valid, bool_and_true(valid)) : {Origins.all_in_bounds(tail, Origins.retained_count(sources), Origins.incoming_count(sources), _) == valid : Bool}
      valid_all_bounds_fold(tail, sources, valid, tail_valid)

law valid_outcome_bounds:
  for +outcome: Origins.Outcome
  for +sources: Origins.Sources
  for e: Origins.ValidOutcome(outcome, sources)
  {Origins.all_in_bounds(Origins.outcome_origins(outcome), Origins.retained_count(sources), Origins.incoming_count(sources), True{}) == True{} : Bool}

def valid_outcome_bounds(outcome, sources, e):
  match outcome:
    case Origins.Outcome{_, origins}:
      valid_all_bounds_fold(origins, sources, True{}, e)

def Laws.every_planned_source_index_passes_numeric_bounds(cells, changed, viewport, patch):
  valid_outcome_bounds(Origins.plan(Cells.length(cells), Cells.length(changed), viewport, patch), Origins.Sources{cells, changed}, Laws.every_planned_source_index_exists(cells, changed, viewport, patch))
