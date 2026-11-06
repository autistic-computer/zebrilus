import Base
import ./core.bend as Core

type Grid is Data:
  Grid{columns: U32, rows: U32}

type Dictionary is Data:
  Dictionary{generation: U32, styles: U32, graphemes: U32, bytes: U32}

type Viewport is Data:
  Viewport{generation: Core.StableId, view_generation: Core.StableId, grid: Grid, dictionary: Dictionary}

type Span is Data:
  Span{row: U32, start: U32, length: U32, clear: Bool}

type Patch is Data:
  Patch{base: Core.StableId, base_view: Core.StableId, generation: Core.StableId, view_generation: Core.StableId, dictionary_generation: U32, grid: Grid, style_base: U32, styles: U32, grapheme_base: U32, graphemes: U32, bytes: U32, positive: Bool, shift: U32, cells: U32, spans: List<&2, Span>}

type Outcome is Data:
  Outcome{code: U32, viewport: Viewport}

def current_generation(viewport: Viewport) -> Core.StableId:
  match viewport:
    case Viewport{generation, _, _, _}:
      generation

def current_view_generation(viewport: Viewport) -> Core.StableId:
  match viewport:
    case Viewport{_, generation, _, _}:
      generation

def patch_generation(patch: Patch) -> Core.StableId:
  match patch:
    case Patch{_, _, generation, _, _, _, _, _, _, _, _, _, _, _, _}:
      generation

def patch_base(patch: Patch) -> Core.StableId:
  match patch:
    case Patch{base, _, _, _, _, _, _, _, _, _, _, _, _, _, _}:
      base

def patch_base_view(patch: Patch) -> Core.StableId:
  match patch:
    case Patch{_, base, _, _, _, _, _, _, _, _, _, _, _, _, _}:
      base

def patch_view_generation(patch: Patch) -> Core.StableId:
  match patch:
    case Patch{_, _, _, generation, _, _, _, _, _, _, _, _, _, _, _}:
      generation

def generation_matches(viewport: Viewport, patch: Patch) -> Bool:
  match viewport patch:
    case Viewport{generation, view, _, _} Patch{base, base_view, _, _, _, _, _, _, _, _, _, _, _, _, _}:
      Core.id_equal(generation, base) && Core.id_equal(view, base_view)

def dictionary_matches(viewport: Viewport, patch: Patch) -> Bool:
  match viewport patch:
    case Viewport{_, _, _, Dictionary{generation, _, _, _}} Patch{_, _, _, _, dictionary, _, _, _, _, _, _, _, _, _, _}:
      Core.u32_equal(generation, dictionary)

def grid_matches(viewport: Viewport, patch: Patch) -> Bool:
  match viewport patch:
    case Viewport{_, _, Grid{+columns, +rows}, _} Patch{_, _, _, _, _, Grid{pc, pr}, _, _, _, _, _, _, _, _, _}:
      Core.u32_equal(columns, pc) && (Core.u32_equal(rows, pr) && (U32.is_le(columns, 65535) && U32.is_le(rows, 65535)))

def appends(styles: U32, graphemes: U32, bytes: U32) -> Bool:
  U32.is_ne(styles, 0) || U32.is_ne(graphemes, 0) || U32.is_ne(bytes, 0)

def bases_match(styles: U32, graphemes: U32, style_base: U32, grapheme_base: U32, append: Bool) -> Bool:
  match append:
    case False{}:
      True{}
    case True{}:
      Core.u32_equal(styles, style_base) && Core.u32_equal(graphemes, grapheme_base)

def dictionary_valid(viewport: Viewport, patch: Patch) -> Bool:
  match viewport patch:
    case Viewport{_, _, _, Dictionary{_, +styles, +graphemes, +bytes}} Patch{_, _, _, _, _, _, style_base, +added_styles, grapheme_base, +added_graphemes, +added_bytes, _, _, _, _}:
      bases_match(styles, graphemes, style_base, grapheme_base, appends(added_styles, added_graphemes, added_bytes)) &&
      ((U32.is_le(styles, 65536) && U32.is_le(added_styles, U32.sub(65536, styles))) &&
      ((U32.is_le(graphemes, 2147483648) && U32.is_le(added_graphemes, U32.sub(2147483648, graphemes))) &&
      U32.is_le(added_bytes, U32.sub(4294967295, bytes))))

def follows(previous: Maybe<&2, U32>, row: U32) -> Bool:
  match previous:
    case None{}:
      True{}
    case Some{before}:
      U32.is_lt(before, row)

def span_valid(span: Span, previous: Maybe<&2, U32>, +rows: U32, +columns: U32, cells: U32) -> Bool:
  match span:
    case Span{+row, +start, +length, _}:
      U32.is_lt(row, rows) && (follows(previous, row) && (U32.is_le(start, columns) && (U32.is_le(length, U32.sub(columns, start)) && U32.is_le(length, cells))))

def spans_fold(spans: List<&2, Span>, previous: Maybe<&2, U32>, +rows: U32, +columns: U32, +cells: U32, valid: Bool) -> Bool:
  match spans:
    case Nil{}:
      valid && U32.is_eq(cells, 0)
    case Span{+row, start, +length, clear} <> tail:
      spans_fold(tail, Some{row}, rows, columns, U32.sub(cells, length), valid && span_valid(Span{row, start, length, clear}, previous, rows, columns, cells))

def spans_valid(spans: List<&2, Span>, previous: Maybe<&2, U32>, rows: U32, columns: U32, cells: U32) -> Bool:
  spans_fold(spans, previous, rows, columns, cells, True{})

def reverse_spans(spans: List<&2, Span>, reversed: List<&2, Span>) -> List<&2, Span>:
  match spans:
    case Nil{}:
      reversed
    case head <> tail:
      reverse_spans(tail, head <> reversed)

def next_row(row: U32, descending: Bool) -> U32:
  match descending:
    case False{}:
      U32.add(row, 1)
    case True{}:
      U32.sub(row, 1)

def edge_span(span: Span, row: U32, columns: U32) -> Bool:
  match span:
    case Span{actual, start, length, clear}:
      Core.u32_equal(actual, row) && Core.u32_equal(start, 0) && (Core.u32_equal(length, columns) || clear)

def edge_fold(spans: List<&2, Span>, +row: U32, count: Nat, +columns: U32, +descending: Bool, valid: Bool) -> Bool:
  match spans count:
    case _ 0n:
      valid
    case Nil{} 1n+_:
      False{}
    case head <> tail 1n+remaining:
      edge_fold(tail, next_row(row, descending), remaining, columns, descending, valid && edge_span(head, row, columns))

def edge_valid(spans: List<&2, Span>, row: U32, count: Nat, columns: U32, descending: Bool) -> Bool:
  edge_fold(spans, row, count, columns, descending, True{})

def exposed_valid(spans: List<&2, Span>, +rows: U32, columns: U32, shift: U32, positive: Bool) -> Bool:
  match positive:
    case True{}:
      edge_valid(spans, 0, U32.to_nat(shift), columns, False{})
    case False{}:
      edge_valid(reverse_spans(spans, Nil{}), U32.sub(rows, 1), U32.to_nat(shift), columns, True{})

def scroll_limit(positive: Bool) -> U32:
  match positive:
    case True{}:
      32767
    case False{}:
      32768

def rows_eligible(+rows: U32, columns: U32, +shift: U32, positive: Bool) -> Bool:
  U32.is_le(rows, 65535) && U32.is_le(columns, 65535) && U32.is_le(shift, scroll_limit(positive)) && (U32.is_eq(shift, 0) || U32.is_lt(shift, rows))

def rows_gate(+spans: List<&2, Span>, +rows: U32, +columns: U32, shift: U32, positive: Bool, cells: U32, eligible: Bool) -> Bool:
  match eligible:
    case False{}:
      False{}
    case True{}:
      spans_valid(spans, None{}, rows, columns, cells) && exposed_valid(spans, rows, columns, shift, positive)

def rows_valid(viewport: Viewport, patch: Patch) -> Bool:
  match viewport patch:
    case Viewport{_, _, Grid{+columns, +rows}, _} Patch{_, _, _, _, _, _, _, _, _, _, _, +positive, +shift, cells, spans}:
      rows_gate(spans, rows, columns, shift, positive, cells, rows_eligible(rows, columns, shift, positive))

def status_gate(generation: Bool, dictionary: Bool, dimensions: Bool, counts: Bool, rows: Bool) -> U32:
  match generation dictionary dimensions counts rows:
    case False{} _ _ _ _:
      1
    case True{} False{} _ _ _:
      2
    case True{} True{} False{} _ _:
      3
    case True{} True{} True{} False{} _:
      2
    case True{} True{} True{} True{} False{}:
      4
    case True{} True{} True{} True{} True{}:
      0

def status(+viewport: Viewport, +patch: Patch) -> U32:
  status_gate(generation_matches(viewport, patch), dictionary_matches(viewport, patch), grid_matches(viewport, patch), dictionary_valid(viewport, patch), rows_valid(viewport, patch))

def checks(+viewport: Viewport, +patch: Patch) -> Bool:
  generation_matches(viewport, patch) && (dictionary_matches(viewport, patch) && (grid_matches(viewport, patch) && (dictionary_valid(viewport, patch) && rows_valid(viewport, patch))))

def candidate(viewport: Viewport, patch: Patch) -> Viewport:
  match viewport patch:
    case Viewport{_, _, grid, Dictionary{dictionary, styles, graphemes, bytes}} Patch{_, _, generation, view, _, _, _, added_styles, _, added_graphemes, added_bytes, _, _, _, _}:
      Viewport{generation, view, grid, Dictionary{dictionary, U32.add(styles, added_styles), U32.add(graphemes, added_graphemes), U32.add(bytes, added_bytes)}}

def choose(viewport: Viewport, patch: Patch, code: U32, accepted: Bool) -> Outcome:
  match accepted:
    case False{}:
      Outcome{code, viewport}
    case True{}:
      Outcome{code, candidate(viewport, patch)}

def transition(+viewport: Viewport, +patch: Patch) -> Outcome:
  +code = status(viewport, patch)
  choose(viewport, patch, code, U32.is_eq(code, 0))

def outcome_viewport(outcome: Outcome) -> Viewport:
  match outcome:
    case Outcome{_, viewport}:
      viewport

def SpanRanges(spans: List<&2, Span>, +rows: U32, +columns: U32) -> Type:
  match spans:
    case Nil{}:
      Unit
    case Span{row, +start, length, _} <> tail:
      {U32.is_lt(row, rows) == True{} : Bool} &
      ({U32.is_le(start, columns) == True{} : Bool} &
      ({U32.is_le(length, U32.sub(columns, start)) == True{} : Bool} & SpanRanges(tail, rows, columns)))

def BoundedPatch(viewport: Viewport, patch: Patch) -> Type:
  match viewport patch:
    case Viewport{_, _, Grid{columns, rows}, _} Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, _, spans}:
      SpanRanges(spans, rows, columns)

def CoveredEdge(spans: List<&2, Span>, +row: U32, count: Nat, +columns: U32, +descending: Bool) -> Type:
  match spans count:
    case _ 0n:
      Unit
    case Nil{} 1n+_:
      Empty
    case Span{actual, start, length, clear} <> tail 1n+remaining:
      {actual == row : U32} &
      ({start == 0 : U32} &
      (Either<&1, &1, {length == columns : U32}, {clear == True{} : Bool}> &
      CoveredEdge(tail, next_row(row, descending), remaining, columns, descending)))

def Exposure(spans: List<&2, Span>, rows: U32, columns: U32, shift: U32, positive: Bool) -> Type:
  match positive:
    case True{}:
      CoveredEdge(spans, 0, U32.to_nat(shift), columns, False{})
    case False{}:
      CoveredEdge(reverse_spans(spans, Nil{}), U32.sub(rows, 1), U32.to_nat(shift), columns, True{})

def ExposedPatch(viewport: Viewport, patch: Patch) -> Type:
  match viewport patch:
    case Viewport{_, _, Grid{columns, rows}, _} Patch{_, _, _, _, _, _, _, _, _, _, _, positive, shift, _, spans}:
      Exposure(spans, rows, columns, shift, positive)

def DictionaryLimits(viewport: Viewport, patch: Patch) -> Type:
  match viewport patch:
    case Viewport{_, _, _, Dictionary{_, +styles, +graphemes, bytes}} Patch{_, _, _, _, _, _, _, added_styles, _, added_graphemes, added_bytes, _, _, _, _}:
      {U32.is_le(styles, 65536) == True{} : Bool} &
      ({U32.is_le(added_styles, U32.sub(65536, styles)) == True{} : Bool} &
      ({U32.is_le(graphemes, 2147483648) == True{} : Bool} &
      ({U32.is_le(added_graphemes, U32.sub(2147483648, graphemes)) == True{} : Bool} &
      {U32.is_le(added_bytes, U32.sub(4294967295, bytes)) == True{} : Bool})))
