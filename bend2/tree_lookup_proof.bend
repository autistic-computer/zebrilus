import Base
import ./core.bend as Core
import ./id_proof.bend as Id
import ./tree_list.bend as Lists
import ./tree_lookup.bend as Lookup
import ./tree_lookup_laws.bend as Laws
import ./tree_sort.bend as Sort

def zipper_original(-A: Data, zipper: Lookup.Zipper<A>) -> List<&2, Sort.Tagged<A>>:
  match zipper:
    case Lookup.Zipper{before, entry, after}:
      Lists.reverse_into(Sort.Tagged<A>, before, entry <> after)

def search_original(-A: Data, state: Lookup.Search<A>) -> List<&2, Sort.Tagged<A>>:
  match state:
    case Lookup.Searching{remaining, before}:
      Lists.reverse_into(Sort.Tagged<A>, before, remaining)
    case Lookup.Located{zipper}:
      zipper_original(A, zipper)

def result_original(-A: Data, result: Maybe<&2, Lookup.Zipper<A>>, fallback: List<&2, Sort.Tagged<A>>) -> List<&2, Sort.Tagged<A>>:
  match result:
    case None{}:
      fallback
    case Some{zipper}:
      zipper_original(A, zipper)

law found_original:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +before: List<&2, Sort.Tagged<A>>
  for +same: Bool
  {search_original(A, Lookup.found(A, head, tail, before, same)) == Lists.reverse_into(Sort.Tagged<A>, before, head <> tail) : List<&2, Sort.Tagged<A>>}

def found_original(A, head, tail, before, same):
  match same:
    case False{}:
      {==}
    case True{}:
      {==}

law locate_loop_original:
  for -A: Data
  for +fuel: Nat
  for +state: Lookup.Search<A>
  for +target: Core.StableId
  {result_original(A, Lookup.locate_loop(A, fuel, state, target), search_original(A, state)) == search_original(A, state) : List<&2, Sort.Tagged<A>>}

def locate_loop_original(A, fuel, state, target):
  match fuel state:
    case _ Lookup.Located{_}:
      {==}
    case _ Lookup.Searching{Nil{}, _}:
      {==}
    case 0n Lookup.Searching{_ <> _, _}:
      {==}
    case 1n+remaining Lookup.Searching{head <> tail, before}:
      +next = Lookup.found(A, head, tail, before, Core.id_equal(Lookup.entry_id(A, head), target))
      +result = Lookup.locate_loop(A, remaining, next, target)
      +old = Lists.reverse_into(Sort.Tagged<A>, before, head <> tail)
      +fresh = search_original(A, next)
      +stable = found_original(A, head, tail, before, Core.id_equal(Lookup.entry_id(A, head), target))
      adjusted = Equal.cong(List<&2, Sort.Tagged<A>>, List<&2, Sort.Tagged<A>>, fallback => result_original(A, result, fallback), fresh, old, stable)
      Equal.trans(List<&2, Sort.Tagged<A>>, result_original(A, result, old), result_original(A, result, fresh), old, Equal.sym(List<&2, Sort.Tagged<A>>, result_original(A, result, fresh), result_original(A, result, old), adjusted), Equal.trans(List<&2, Sort.Tagged<A>>, result_original(A, result, fresh), fresh, old, locate_loop_original(A, remaining, next, target), stable))

law replace_same:
  for -A: Data
  for +zipper: Lookup.Zipper<A>
  {Lookup.replace(A, zipper, Lookup.zipper_entry(A, zipper)) == zipper_original(A, zipper) : List<&2, Sort.Tagged<A>>}

def replace_same(A, zipper):
  match zipper:
    case Lookup.Zipper{_, _, _}:
      {==}

law locate_original:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +target: Core.StableId
  {result_original(A, Lookup.locate(A, values, target), values) == values : List<&2, Sort.Tagged<A>>}

def locate_original(A, values, target):
  locate_loop_original(A, 1n+Lists.length(Sort.Tagged<A>, values), Lookup.Searching{values, Nil{}}, target)

law located_original:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +target: Core.StableId
  for +zipper: Lookup.Zipper<A>
  for e: {Lookup.locate(A, values, target) == Some{zipper} : Maybe<&2, Lookup.Zipper<A>>}
  {zipper_original(A, zipper) == values : List<&2, Sort.Tagged<A>>}

def located_original(A, values, target, zipper, e):
  %e : {result_original(A, _, values) == values : List<&2, Sort.Tagged<A>>}
  locate_original(A, values, target)

def Laws.found_zipper_reconstructs_the_complete_source(A, values, target, zipper, e):
  Equal.trans(List<&2, Sort.Tagged<A>>, Lookup.replace(A, zipper, Lookup.zipper_entry(A, zipper)), zipper_original(A, zipper), values, replace_same(A, zipper), located_original(A, values, target, zipper, e))

def FoundMatches(-A: Data, result: Maybe<&2, Lookup.Zipper<A>>, target: Core.StableId) -> Data:
  match result:
    case None{}:
      Unit
    case Some{zipper}:
      {Lookup.entry_id(A, Lookup.zipper_entry(A, zipper)) == target : Core.StableId}

def SearchMatches(-A: Data, state: Lookup.Search<A>, target: Core.StableId) -> Data:
  match state:
    case Lookup.Searching{_, _}:
      Unit
    case Lookup.Located{zipper}:
      {Lookup.entry_id(A, Lookup.zipper_entry(A, zipper)) == target : Core.StableId}

law found_matches:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +before: List<&2, Sort.Tagged<A>>
  for +target: Core.StableId
  for +same: Bool
  for e: {Core.id_equal(Lookup.entry_id(A, head), target) == same : Bool}
  SearchMatches(A, Lookup.found(A, head, tail, before, same), target)

def found_matches(A, head, tail, before, target, same, e):
  match same:
    case False{}:
      Unit{}
    case True{}:
      Id.id_sound(Lookup.entry_id(A, head), target, e)

law locate_loop_matches:
  for -A: Data
  for +fuel: Nat
  for +state: Lookup.Search<A>
  for +target: Core.StableId
  for proof: SearchMatches(A, state, target)
  FoundMatches(A, Lookup.locate_loop(A, fuel, state, target), target)

def locate_loop_matches(A, fuel, state, target, proof):
  match fuel state:
    case _ Lookup.Located{_}:
      proof
    case _ Lookup.Searching{Nil{}, _}:
      Unit{}
    case 0n Lookup.Searching{_ <> _, _}:
      Unit{}
    case 1n+remaining Lookup.Searching{head <> tail, before}:
      locate_loop_matches(A, remaining, Lookup.found(A, head, tail, before, Core.id_equal(Lookup.entry_id(A, head), target)), target, found_matches(A, head, tail, before, target, Core.id_equal(Lookup.entry_id(A, head), target), {==}))

def Laws.found_zipper_has_the_exact_requested_id(A, values, target, zipper, e):
  %e : FoundMatches(A, _, target)
  locate_loop_matches(A, 1n+Lists.length(Sort.Tagged<A>, values), Lookup.Searching{values, Nil{}}, target, Unit{})
