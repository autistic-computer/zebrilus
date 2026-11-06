import Base
import ./core.bend as Core
import ./tree_lookup.bend as Lookup
import ./tree_sort.bend as Sort

law found_zipper_reconstructs_the_complete_source:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +target: Core.StableId
  for +zipper: Lookup.Zipper<A>
  for e: {Lookup.locate(A, values, target) == Some{zipper} : Maybe<&2, Lookup.Zipper<A>>}
  {Lookup.replace(A, zipper, Lookup.zipper_entry(A, zipper)) == values : List<&2, Sort.Tagged<A>>}

law found_zipper_has_the_exact_requested_id:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +target: Core.StableId
  for +zipper: Lookup.Zipper<A>
  for e: {Lookup.locate(A, values, target) == Some{zipper} : Maybe<&2, Lookup.Zipper<A>>}
  {Lookup.entry_id(A, Lookup.zipper_entry(A, zipper)) == target : Core.StableId}
