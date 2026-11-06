import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_sort.bend as Sort
import ./tree_list.bend as Lists

type Zipper<-A: Data> is Data:
  Zipper{before: List<&2, Sort.Tagged<A>>, entry: Sort.Tagged<A>, after: List<&2, Sort.Tagged<A>>}

type Search<-A: Data> is Data:
  Searching{remaining: List<&2, Sort.Tagged<A>>, before: List<&2, Sort.Tagged<A>>}
  Located{zipper: Zipper<A>}

def entry_id(-A: Data, entry: Sort.Tagged<A>) -> Core.StableId:
  match entry:
    case Sort.Tagged{Sort.SortKey{_, high, low}, _}:
      Core.Id{high, low}

def found(-A: Data, head: Sort.Tagged<A>, tail: List<&2, Sort.Tagged<A>>, before: List<&2, Sort.Tagged<A>>, same: Bool) -> Search<A>:
  match same:
    case False{}:
      Searching{tail, head <> before}
    case True{}:
      Located{Zipper{before, head, tail}}

def locate_loop(-A: Data, fuel: Nat, state: Search<A>, +target: Core.StableId) -> Maybe<&2, Zipper<A>>:
  match fuel state:
    case _ Located{zipper}:
      Some{zipper}
    case _ Searching{Nil{}, _}:
      None{}
    case 0n _:
      None{}
    case 1n+rest Searching{Con{+head, tail}, before}:
      locate_loop(A, rest, found(A, head, tail, before, Core.id_equal(entry_id(A, head), target)), target)

def locate(-A: Data, +values: List<&2, Sort.Tagged<A>>, target: Core.StableId) -> Maybe<&2, Zipper<A>>:
  locate_loop(A, 1n+Lists.length(Sort.Tagged<A>, values), Searching{values, Nil{}}, target)

def zipper_entry(-A: Data, zipper: Zipper<A>) -> Sort.Tagged<A>:
  match zipper:
    case Zipper{_, entry, _}:
      entry

def zipper_value(-A: Data, zipper: Zipper<A>) -> A:
  match zipper:
    case Zipper{_, Sort.Tagged{_, value}, _}:
      value

def replace(-A: Data, zipper: Zipper<A>, entry: Sort.Tagged<A>) -> List<&2, Sort.Tagged<A>>:
  match zipper:
    case Zipper{before, _, after}:
      Lists.reverse_into(Sort.Tagged<A>, before, entry <> after)

def replace_value(-A: Data, zipper: Zipper<A>, value: A) -> List<&2, Sort.Tagged<A>>:
  match zipper:
    case Zipper{before, Sort.Tagged{id, _}, after}:
      Lists.reverse_into(Sort.Tagged<A>, before, Sort.Tagged{id, value} <> after)

def upsert_found(-A: Data, values: List<&2, Sort.Tagged<A>>, entry: Sort.Tagged<A>, found: Maybe<&2, Zipper<A>>) -> List<&2, Sort.Tagged<A>>:
  match found:
    case None{}:
      Lists.append(Sort.Tagged<A>, values, [entry])
    case Some{zipper}:
      replace(A, zipper, entry)

def upsert(-A: Data, +values: List<&2, Sort.Tagged<A>>, +entry: Sort.Tagged<A>) -> List<&2, Sort.Tagged<A>>:
  upsert_found(A, values, entry, locate(A, values, entry_id(A, entry)))

def remove_fold(-A: Data, values: List<&2, Sort.Tagged<A>>, +target: Core.StableId, acc: List<&2, Sort.Tagged<A>>) -> List<&2, Sort.Tagged<A>>:
  match values:
    case Nil{}:
      Lists.reverse(Sort.Tagged<A>, acc)
    case Con{+head, tail}:
      remove_fold(A, tail, target, Lists.selected(Sort.Tagged<A>, head, acc, Bool.not(Core.id_equal(entry_id(A, head), target))))

def remove(-A: Data, values: List<&2, Sort.Tagged<A>>, target: Core.StableId) -> List<&2, Sort.Tagged<A>>:
  remove_fold(A, values, target, Nil{})
