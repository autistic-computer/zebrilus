import Base
import ./terminal.bend as Terminal
import ./terminal_cells.bend as Cells

law copying_unmodified_cells_preserves_the_grid:
  for +count: Nat
  for +cursor: Cells.Cursor
  {Cells.finish(Cells.preserve(count, cursor)) == Cells.finish(cursor) : List<&2, Cells.Cell>}

law replacement_preserves_cells_outside_the_span:
  for +pairs: List<&2, Cells.CellPair>
  for +old_tail: List<&2, Cells.Cell>
  for changed_tail: List<&2, Cells.Cell>
  for position: Nat
  for +prefix_reversed: List<&2, Cells.Cell>
  {Cells.finish(Cells.replace(Cells.pair_length(pairs), Cells.Cursor{Cells.before(pairs, old_tail), Cells.after(pairs, changed_tail), position, prefix_reversed})) == Cells.prepend_reversed(prefix_reversed, Cells.after(pairs, old_tail)) : List<&2, Cells.Cell>}

law clearing_preserves_cells_outside_the_tail:
  for +cleared: List<&2, Cells.Cell>
  for +old_tail: List<&2, Cells.Cell>
  for changes: List<&2, Cells.Cell>
  for position: Nat
  for +prefix_reversed: List<&2, Cells.Cell>
  {Cells.finish(Cells.clear(Cells.prefix_length(cleared), Cells.Cursor{Cells.prefixed(cleared, old_tail), changes, position, prefix_reversed})) == Cells.prepend_reversed(prefix_reversed, Cells.cleared(cleared, old_tail)) : List<&2, Cells.Cell>}

law rejected_patch_preserves_all_cells:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == False{} : Bool}
  {Cells.outcome_cells(Cells.apply(cells, changed, viewport, patch)) == cells : List<&2, Cells.Cell>}

law invalid_payload_preserves_all_cells:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {Cells.lengths_valid(cells, changed, viewport, patch) == False{} : Bool}
  {Cells.outcome_cells(Cells.apply(cells, changed, viewport, patch)) == cells : List<&2, Cells.Cell>}

law empty_patch_preserves_all_cells:
  for +cells: List<&2, Cells.Cell>
  for changed: List<&2, Cells.Cell>
  for columns: U32
  for rows: U32
  for positive: Bool
  {Cells.mutate_rows(cells, changed, columns, rows, 0, positive, Nil{}) == cells : List<&2, Cells.Cell>}
