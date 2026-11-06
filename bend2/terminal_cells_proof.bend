import Base
import ./terminal.bend as Terminal
import ./terminal_cells.bend as Cells
import ./terminal_cells_laws.bend as Laws

def Laws.copying_unmodified_cells_preserves_the_grid(count, cursor):
  match count cursor:
    case 0n _:
      {==}
    case 1n+_ Cells.Cursor{Nil{}, _, _, _}:
      {==}
    case 1n+remaining Cells.Cursor{head <> tail, changes, position, out}:
      Laws.copying_unmodified_cells_preserves_the_grid(remaining, Cells.Cursor{tail, changes, 1n+position, head <> out})

def Laws.replacement_preserves_cells_outside_the_span(pairs, old_tail, changed_tail, position, prefix_reversed):
  match pairs:
    case Nil{}:
      {==}
    case Cells.CellPair{old, new} <> rest:
      Laws.replacement_preserves_cells_outside_the_span(rest, old_tail, changed_tail, 1n+position, new <> prefix_reversed)

def Laws.clearing_preserves_cells_outside_the_tail(cleared, old_tail, changes, position, prefix_reversed):
  match cleared:
    case Nil{}:
      {==}
    case _ <> rest:
      Laws.clearing_preserves_cells_outside_the_tail(rest, old_tail, changes, 1n+position, Cells.Cell{0, 0} <> prefix_reversed)

def Laws.rejected_patch_preserves_all_cells(cells, changed, viewport, patch, e):
  %Equal.sym(Bool, U32.is_eq(Terminal.status(viewport, patch), 0), False{}, e) :
    {Cells.outcome_cells(Cells.apply_checked(cells, changed, viewport, patch, Terminal.status(viewport, patch), _)) == cells : List<&2, Cells.Cell>}
  {==}

law invalid_payload_case:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +code: U32
  for +accepted: Bool
  for e: {Cells.lengths_valid(cells, changed, viewport, patch) == False{} : Bool}
  {Cells.outcome_cells(Cells.apply_checked(cells, changed, viewport, patch, code, accepted)) == cells : List<&2, Cells.Cell>}

def invalid_payload_case(cells, changed, viewport, patch, code, accepted, e):
  match accepted:
    case False{}:
      {==}
    case True{}:
      %Equal.sym(Bool, Cells.lengths_valid(cells, changed, viewport, patch), False{}, e) :
        {Cells.outcome_cells(Cells.apply_lengths(cells, changed, viewport, patch, _)) == cells : List<&2, Cells.Cell>}
      {==}

def Laws.invalid_payload_preserves_all_cells(cells, changed, viewport, patch, e):
  invalid_payload_case(cells, changed, viewport, patch, Terminal.status(viewport, patch), U32.is_eq(Terminal.status(viewport, patch), 0), e)

def Laws.empty_patch_preserves_all_cells(cells, changed, columns, rows, positive):
  {==}
