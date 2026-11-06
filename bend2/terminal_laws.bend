import Base
import ./core.bend as Core
import ./terminal.bend as Terminal

law rejected_patch_is_atomic:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == False{} : Bool}
  {Terminal.outcome_viewport(Terminal.transition(viewport, patch)) == viewport : Terminal.Viewport}

law accepted_patch_generations:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  {Terminal.current_generation(Terminal.outcome_viewport(Terminal.transition(viewport, patch))) == Terminal.patch_generation(patch) : Core.StableId} &
  {Terminal.current_view_generation(Terminal.outcome_viewport(Terminal.transition(viewport, patch))) == Terminal.patch_view_generation(patch) : Core.StableId}

law accepted_patch_bases:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for +e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  {Terminal.current_generation(viewport) == Terminal.patch_base(patch) : Core.StableId} &
  {Terminal.current_view_generation(viewport) == Terminal.patch_base_view(patch) : Core.StableId}

law accepted_spans_are_bounded:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  Terminal.BoundedPatch(viewport, patch)

law accepted_scroll_replaces_exposed_rows:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  Terminal.ExposedPatch(viewport, patch)

law accepted_dictionary_is_bounded:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  Terminal.DictionaryLimits(viewport, patch)
