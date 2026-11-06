import Base
import ./core.bend as Core
import ./terminal.bend as Terminal
import ./terminal_laws.bend as Laws
import ./id_proof.bend as Id

law status_sound:
  for +generation: Bool
  for +dictionary: Bool
  for +dimensions: Bool
  for +counts: Bool
  for +rows: Bool
  for e: {U32.is_eq(Terminal.status_gate(generation, dictionary, dimensions, counts, rows), 0) == True{} : Bool}
  {generation && (dictionary && (dimensions && (counts && rows))) == True{} : Bool}

def status_sound(generation, dictionary, dimensions, counts, rows, e):
  match generation dictionary dimensions counts rows:
    case False{} _ _ _ _:
      Id.absurd({generation && (dictionary && (dimensions && (counts && rows))) == True{} : Bool}, Id.false_is_not_true(e))
    case True{} False{} _ _ _:
      Id.absurd({generation && (dictionary && (dimensions && (counts && rows))) == True{} : Bool}, Id.false_is_not_true(e))
    case True{} True{} False{} _ _:
      Id.absurd({generation && (dictionary && (dimensions && (counts && rows))) == True{} : Bool}, Id.false_is_not_true(e))
    case True{} True{} True{} False{} _:
      Id.absurd({generation && (dictionary && (dimensions && (counts && rows))) == True{} : Bool}, Id.false_is_not_true(e))
    case True{} True{} True{} True{} False{}:
      Id.absurd({False{} == True{} : Bool}, Id.false_is_not_true(e))
    case True{} True{} True{} True{} True{}:
      {==}

law checks_sound:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {U32.is_eq(Terminal.status(viewport, patch), 0) == True{} : Bool}
  {Terminal.checks(viewport, patch) == True{} : Bool}

def checks_sound(viewport, patch, e):
  status_sound(Terminal.generation_matches(viewport, patch), Terminal.dictionary_matches(viewport, patch), Terminal.grid_matches(viewport, patch), Terminal.dictionary_valid(viewport, patch), Terminal.rows_valid(viewport, patch), e)

law generation_sound:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {Terminal.checks(viewport, patch) == True{} : Bool}
  {Terminal.generation_matches(viewport, patch) == True{} : Bool}

def generation_sound(viewport, patch, e):
  Id.and_left(Terminal.generation_matches(viewport, patch), Terminal.dictionary_matches(viewport, patch) && (Terminal.grid_matches(viewport, patch) && (Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch))), e)

law dictionary_sound:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {Terminal.checks(viewport, patch) == True{} : Bool}
  {Terminal.dictionary_valid(viewport, patch) == True{} : Bool}

def dictionary_sound(viewport, patch, e):
  e1 = Id.and_right(Terminal.generation_matches(viewport, patch), Terminal.dictionary_matches(viewport, patch) && (Terminal.grid_matches(viewport, patch) && (Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch))), e)
  e2 = Id.and_right(Terminal.dictionary_matches(viewport, patch), Terminal.grid_matches(viewport, patch) && (Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch)), e1)
  e3 = Id.and_right(Terminal.grid_matches(viewport, patch), Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch), e2)
  Id.and_left(Terminal.dictionary_valid(viewport, patch), Terminal.rows_valid(viewport, patch), e3)

law rows_sound:
  for +viewport: Terminal.Viewport
  for +patch: Terminal.Patch
  for e: {Terminal.checks(viewport, patch) == True{} : Bool}
  {Terminal.rows_valid(viewport, patch) == True{} : Bool}

def rows_sound(viewport, patch, e):
  e1 = Id.and_right(Terminal.generation_matches(viewport, patch), Terminal.dictionary_matches(viewport, patch) && (Terminal.grid_matches(viewport, patch) && (Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch))), e)
  e2 = Id.and_right(Terminal.dictionary_matches(viewport, patch), Terminal.grid_matches(viewport, patch) && (Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch)), e1)
  e3 = Id.and_right(Terminal.grid_matches(viewport, patch), Terminal.dictionary_valid(viewport, patch) && Terminal.rows_valid(viewport, patch), e2)
  Id.and_right(Terminal.dictionary_valid(viewport, patch), Terminal.rows_valid(viewport, patch), e3)

law row_parts:
  for +spans: List<&2, Terminal.Span>
  for +rows: U32
  for +columns: U32
  for +shift: U32
  for +positive: Bool
  for +cells: U32
  for +eligible: Bool
  for e: {Terminal.rows_gate(spans, rows, columns, shift, positive, cells, eligible) == True{} : Bool}
  {Terminal.spans_valid(spans, None{}, rows, columns, cells) && Terminal.exposed_valid(spans, rows, columns, shift, positive) == True{} : Bool}

def row_parts(spans, rows, columns, shift, positive, cells, eligible, e):
  match eligible:
    case False{}:
      Id.absurd({Terminal.spans_valid(spans, None{}, rows, columns, cells) && Terminal.exposed_valid(spans, rows, columns, shift, positive) == True{} : Bool}, Id.false_is_not_true(e))
    case True{}:
      e

law prepend_spans:
  for +row: U32
  for +start: U32
  for +length: U32
  for +clear: Bool
  for +tail: List<&2, Terminal.Span>
  for +previous: Maybe<&2, U32>
  for +rows: U32
  for +columns: U32
  for +cells: U32
  for +valid: Bool
  for proof: {valid && Terminal.span_valid(Terminal.Span{row, start, length, clear}, previous, rows, columns, cells) == True{} : Bool} & Terminal.SpanRanges(tail, rows, columns)
  {valid == True{} : Bool} & Terminal.SpanRanges(Terminal.Span{row, start, length, clear} <> tail, rows, columns)

def prepend_spans(row, start, length, clear, tail, previous, rows, columns, cells, valid, proof):
  match proof:
    case Tuple{acc, tail_ranges}:
      +a = U32.is_lt(row, rows)
      +b = Terminal.follows(previous, row)
      +c = U32.is_le(start, columns)
      +d = U32.is_le(length, U32.sub(columns, start))
      +f = U32.is_le(length, cells)
      +acc_proof = acc
      +head = Id.and_right(valid, a && (b && (c && (d && f))), acc_proof)
      +h2 = Id.and_right(a, b && (c && (d && f)), head)
      +h3 = Id.and_right(b, c && (d && f), h2)
      h4 = Id.and_right(c, d && f, h3)
      (Id.and_left(valid, a && (b && (c && (d && f))), acc_proof),
        (Id.and_left(a, b && (c && (d && f)), head),
          (Id.and_left(c, d && f, h3), (Id.and_left(d, f, h4), tail_ranges))))

law spans_fold_sound:
  for +spans: List<&2, Terminal.Span>
  for +previous: Maybe<&2, U32>
  for +rows: U32
  for +columns: U32
  for +cells: U32
  for +valid: Bool
  for e: {Terminal.spans_fold(spans, previous, rows, columns, cells, valid) == True{} : Bool}
  {valid == True{} : Bool} & Terminal.SpanRanges(spans, rows, columns)

def spans_fold_sound(spans, previous, rows, columns, cells, valid, e):
  match spans:
    case Nil{}:
      (Id.and_left(valid, U32.is_eq(cells, 0), e), Unit{})
    case Terminal.Span{row, start, length, clear} <> tail:
      prepend_spans(row, start, length, clear, tail, previous, rows, columns, cells, valid,
        spans_fold_sound(tail, Some{row}, rows, columns, U32.sub(cells, length), valid && Terminal.span_valid(Terminal.Span{row, start, length, clear}, previous, rows, columns, cells), e))

law spans_sound:
  for +spans: List<&2, Terminal.Span>
  for +previous: Maybe<&2, U32>
  for +rows: U32
  for +columns: U32
  for +cells: U32
  for e: {Terminal.spans_valid(spans, previous, rows, columns, cells) == True{} : Bool}
  Terminal.SpanRanges(spans, rows, columns)

def spans_sound(spans, previous, rows, columns, cells, e):
  Pair.snd({True{} == True{} : Bool}, Terminal.SpanRanges(spans, rows, columns), spans_fold_sound(spans, previous, rows, columns, cells, True{}, e))

law covers_case:
  for +length: U32
  for +columns: U32
  for +clear: Bool
  for +same: Bool
  for eq: {Core.u32_equal(length, columns) == same : Bool}
  for e: {same || clear == True{} : Bool}
  Either<&1, &1, {length == columns : U32}, {clear == True{} : Bool}>

def covers_case(length, columns, clear, same, eq, e):
  match same:
    case True{}:
      Inl{Id.u32_sound(length, columns, eq)}
    case False{}:
      Inr{e}

law prepend_edge:
  for +actual: U32
  for +start: U32
  for +length: U32
  for +clear: Bool
  for +tail: List<&2, Terminal.Span>
  for +row: U32
  for +remaining: Nat
  for +columns: U32
  for +descending: Bool
  for +valid: Bool
  for proof: {valid && Terminal.edge_span(Terminal.Span{actual, start, length, clear}, row, columns) == True{} : Bool} & Terminal.CoveredEdge(tail, Terminal.next_row(row, descending), remaining, columns, descending)
  {valid == True{} : Bool} & Terminal.CoveredEdge(Terminal.Span{actual, start, length, clear} <> tail, row, 1n+remaining, columns, descending)

def prepend_edge(actual, start, length, clear, tail, row, remaining, columns, descending, valid, proof):
  match proof:
    case Tuple{acc, tail_coverage}:
      +a = Core.u32_equal(actual, row)
      +b = Core.u32_equal(start, 0)
      +c = Core.u32_equal(length, columns)
      +acc_proof = acc
      +head = Id.and_right(valid, (a && b) && (c || clear), acc_proof)
      +position = Id.and_left(a && b, c || clear, head)
      (Id.and_left(valid, (a && b) && (c || clear), acc_proof),
        (Id.u32_sound(actual, row, Id.and_left(a, b, position)),
          (Id.u32_sound(start, 0, Id.and_right(a, b, position)),
            (covers_case(length, columns, clear, c, {==}, Id.and_right(a && b, c || clear, head)), tail_coverage))))

law edge_fold_sound:
  for +spans: List<&2, Terminal.Span>
  for +row: U32
  for +count: Nat
  for +columns: U32
  for +descending: Bool
  for +valid: Bool
  for e: {Terminal.edge_fold(spans, row, count, columns, descending, valid) == True{} : Bool}
  {valid == True{} : Bool} & Terminal.CoveredEdge(spans, row, count, columns, descending)

def edge_fold_sound(spans, row, count, columns, descending, valid, e):
  match spans count:
    case _ 0n:
      (e, Unit{})
    case Nil{} 1n+_:
      Id.absurd({valid == True{} : Bool} & Empty, Id.false_is_not_true(e))
    case Terminal.Span{actual, start, length, clear} <> tail 1n+remaining:
      prepend_edge(actual, start, length, clear, tail, row, remaining, columns, descending, valid,
        edge_fold_sound(tail, Terminal.next_row(row, descending), remaining, columns, descending, valid && Terminal.edge_span(Terminal.Span{actual, start, length, clear}, row, columns), e))

law edge_sound:
  for +spans: List<&2, Terminal.Span>
  for +row: U32
  for +count: Nat
  for +columns: U32
  for +descending: Bool
  for e: {Terminal.edge_valid(spans, row, count, columns, descending) == True{} : Bool}
  Terminal.CoveredEdge(spans, row, count, columns, descending)

def edge_sound(spans, row, count, columns, descending, e):
  Pair.snd({True{} == True{} : Bool}, Terminal.CoveredEdge(spans, row, count, columns, descending), edge_fold_sound(spans, row, count, columns, descending, True{}, e))

law exposure_sound:
  for +spans: List<&2, Terminal.Span>
  for +rows: U32
  for +columns: U32
  for +shift: U32
  for +positive: Bool
  for e: {Terminal.exposed_valid(spans, rows, columns, shift, positive) == True{} : Bool}
  Terminal.Exposure(spans, rows, columns, shift, positive)

def exposure_sound(spans, rows, columns, shift, positive, e):
  match positive:
    case True{}:
      edge_sound(spans, 0, U32.to_nat(shift), columns, False{}, e)
    case False{}:
      edge_sound(Terminal.reverse_spans(spans, Nil{}), U32.sub(rows, 1), U32.to_nat(shift), columns, True{}, e)

def Laws.rejected_patch_is_atomic(viewport, patch, e):
  %Equal.sym(Bool, U32.is_eq(Terminal.status(viewport, patch), 0), False{}, e) :
    {Terminal.outcome_viewport(Terminal.choose(viewport, patch, Terminal.status(viewport, patch), _)) == viewport : Terminal.Viewport}
  {==}

def Laws.accepted_patch_generations(viewport, patch, e):
  match viewport patch:
    case Terminal.Viewport{generation, view, grid, Terminal.Dictionary{de, ds, dg, db}} Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}:
      +current = {Terminal.Viewport{generation, view, grid, Terminal.Dictionary{de, ds, dg, db}} : Terminal.Viewport}
      +incoming = {Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans} : Terminal.Patch}
      %Equal.sym(Bool, U32.is_eq(Terminal.status(current, incoming), 0), True{}, e) :
        {Terminal.current_generation(Terminal.outcome_viewport(Terminal.choose(current, incoming, Terminal.status(current, incoming), _))) == next : Core.StableId} &
        {Terminal.current_view_generation(Terminal.outcome_viewport(Terminal.choose(current, incoming, Terminal.status(current, incoming), _))) == next_view : Core.StableId}
      ({==}, {==})

def Laws.accepted_patch_bases(viewport, patch, e):
  match viewport patch:
    case Terminal.Viewport{generation, view, grid, dictionary} Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}:
      +same = generation_sound(Terminal.Viewport{generation, view, grid, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, checks_sound(Terminal.Viewport{generation, view, grid, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, e))
      (Id.id_sound(generation, base, Id.and_left(Core.id_equal(generation, base), Core.id_equal(view, base_view), same)),
        Id.id_sound(view, base_view, Id.and_right(Core.id_equal(generation, base), Core.id_equal(view, base_view), same)))

def Laws.accepted_spans_are_bounded(viewport, patch, e):
  match viewport patch:
    case Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary} Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}:
      +valid = rows_sound(Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, checks_sound(Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, e))
      span_edge = row_parts(spans, rows, columns, shift, positive, cells, Terminal.rows_eligible(rows, columns, shift, positive), valid)
      spans_sound(spans, None{}, rows, columns, cells, Id.and_left(Terminal.spans_valid(spans, None{}, rows, columns, cells), Terminal.exposed_valid(spans, rows, columns, shift, positive), span_edge))

def Laws.accepted_scroll_replaces_exposed_rows(viewport, patch, e):
  match viewport patch:
    case Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary} Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}:
      valid = rows_sound(Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, checks_sound(Terminal.Viewport{generation, view, Terminal.Grid{columns, rows}, dictionary}, Terminal.Patch{base, base_view, next, next_view, epoch, pg, sb, styles, gb, graphemes, bytes, positive, shift, cells, spans}, e))
      span_edge = row_parts(spans, rows, columns, shift, positive, cells, Terminal.rows_eligible(rows, columns, shift, positive), valid)
      exposure_sound(spans, rows, columns, shift, positive, Id.and_right(Terminal.spans_valid(spans, None{}, rows, columns, cells), Terminal.exposed_valid(spans, rows, columns, shift, positive), span_edge))

def Laws.accepted_dictionary_is_bounded(viewport, patch, e):
  match viewport patch:
    case Terminal.Viewport{generation, view, grid, Terminal.Dictionary{epoch, styles, graphemes, bytes}} Terminal.Patch{base, base_view, next, next_view, pe, pg, sb, added_styles, gb, added_graphemes, added_bytes, positive, shift, cells, spans}:
      valid = dictionary_sound(Terminal.Viewport{generation, view, grid, Terminal.Dictionary{epoch, styles, graphemes, bytes}}, Terminal.Patch{base, base_view, next, next_view, pe, pg, sb, added_styles, gb, added_graphemes, added_bytes, positive, shift, cells, spans}, checks_sound(Terminal.Viewport{generation, view, grid, Terminal.Dictionary{epoch, styles, graphemes, bytes}}, Terminal.Patch{base, base_view, next, next_view, pe, pg, sb, added_styles, gb, added_graphemes, added_bytes, positive, shift, cells, spans}, e))
      +a = U32.is_le(styles, 65536)
      +b = U32.is_le(added_styles, U32.sub(65536, styles))
      +c = U32.is_le(graphemes, 2147483648)
      +d = U32.is_le(added_graphemes, U32.sub(2147483648, graphemes))
      +f = U32.is_le(added_bytes, U32.sub(4294967295, bytes))
      +limits = Id.and_right(Terminal.bases_match(styles, graphemes, sb, gb, Terminal.appends(added_styles, added_graphemes, added_bytes)), (a && b) && ((c && d) && f), valid)
      +styles_valid = Id.and_left(a && b, (c && d) && f, limits)
      +rest = Id.and_right(a && b, (c && d) && f, limits)
      +graphemes_valid = Id.and_left(c && d, f, rest)
      (Id.and_left(a, b, styles_valid), (Id.and_right(a, b, styles_valid), (Id.and_left(c, d, graphemes_valid), (Id.and_right(c, d, graphemes_valid), Id.and_right(c && d, f, rest)))))
