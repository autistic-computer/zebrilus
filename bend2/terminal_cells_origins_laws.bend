import Base
import ./terminal.bend as Terminal
import ./terminal_cells.bend as Cells
import ./terminal_cells_origins.bend as Origins

law full_mutation_has_exact_source_indices:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Cells.mutate(cells, changed, viewport, patch) == Origins.resolve(Origins.mutate(Origins.indices(Cells.length(cells), True{}), Origins.indices(Cells.length(changed), False{}), viewport, patch), Origins.Sources{cells, changed}) : List<&2, Cells.Cell>}

law complete_patch_has_exact_source_indices_and_rollback:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Cells.apply(cells, changed, viewport, patch) == Origins.resolve_outcome(Origins.plan(Cells.length(cells), Cells.length(changed), viewport, patch), Origins.Sources{cells, changed}) : Cells.Outcome}

law every_planned_source_index_exists:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  Origins.ValidOutcome(Origins.plan(Cells.length(cells), Cells.length(changed), viewport, patch), Origins.Sources{cells, changed})

law every_planned_source_index_passes_numeric_bounds:
  for +cells: List<&2, Cells.Cell>
  for +changed: List<&2, Cells.Cell>
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  {Origins.all_in_bounds(Origins.outcome_origins(Origins.plan(Cells.length(cells), Cells.length(changed), viewport, patch)), Cells.length(cells), Cells.length(changed), True{}) == True{} : Bool}
