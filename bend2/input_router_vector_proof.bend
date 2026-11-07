import Base
import ./tree_list.bend as Lists
import ./tree_list_proof.bend as ListProof
import ./input_router_claims.bend as Claims
import ./input_router_views.bend as Views

law reverse_cons:
  for -A: Data
  for +head: A
  for +tail: List<&2, A>
  {Lists.reverse(A, head <> tail) == Lists.concat(A, Lists.reverse(A, tail), head <> Nil{}) : List<&2, A>}

def reverse_cons(A, head, tail):
  ListProof.reverse_into_concat(A, tail, Nil{}, head <> Nil{})

law last_fold_suffix:
  for -A: Data
  for +middle: List<&2, A>
  for +last: A
  for +previous: A
  for +reversed: List<&2, A>
  {Claims.last_fold(A, Lists.concat(A, middle, last <> Nil{}), previous, reversed) == Claims.Last{last, Lists.concat(A, Lists.reverse(A, reversed), previous <> middle)} : Claims.Last<A>}

def last_fold_suffix(A, middle, last, previous, reversed):
  match middle:
    case Nil{}:
      %reverse_cons(A, previous, reversed) : {Claims.Last{last, Lists.reverse(A, previous <> reversed)} == Claims.Last{last, _} : Claims.Last<A>}
      {==}
    case head <> tail:
      %Equal.sym(Claims.Last<A>, Claims.last_fold(A, Lists.concat(A, tail, last <> Nil{}), head, previous <> reversed), Claims.Last{last, Lists.concat(A, Lists.reverse(A, previous <> reversed), head <> tail)}, last_fold_suffix(A, tail, last, head, previous <> reversed)) : {_ == Claims.Last{last, Lists.concat(A, Lists.reverse(A, reversed), previous <> head <> tail)} : Claims.Last<A>}
      %Equal.sym(List<&2, A>, Lists.reverse(A, previous <> reversed), Lists.concat(A, Lists.reverse(A, reversed), previous <> Nil{}), reverse_cons(A, previous, reversed)) : {Claims.Last{last, Lists.concat(A, _, head <> tail)} == Claims.Last{last, Lists.concat(A, Lists.reverse(A, reversed), previous <> head <> tail)} : Claims.Last<A>}
      %Equal.sym(List<&2, A>, Lists.concat(A, Lists.concat(A, Lists.reverse(A, reversed), previous <> Nil{}), head <> tail), Lists.concat(A, Lists.reverse(A, reversed), previous <> head <> tail), ListProof.concat_associative(A, Lists.reverse(A, reversed), previous <> Nil{}, head <> tail)) : {Claims.Last{last, _} == Claims.Last{last, Lists.concat(A, Lists.reverse(A, reversed), previous <> head <> tail)} : Claims.Last<A>}
      {==}

law pop_last_suffix:
  for -A: Data
  for +prefix: List<&2, A>
  for +last: A
  {Claims.pop_last(A, Lists.concat(A, prefix, last <> Nil{})) == Some{Claims.Last{last, prefix}} : Maybe<&2, Claims.Last<A>>}

def pop_last_suffix(A, prefix, last):
  match prefix:
    case Nil{}:
      {==}
    case head <> tail:
      %last_fold_suffix(A, tail, last, head, Nil{}) : {Some{Claims.last_fold(A, Lists.concat(A, tail, last <> Nil{}), head, Nil{})} == Some{_} : Maybe<&2, Claims.Last<A>>}
      {==}

law replace_at_prefix:
  for -A: Data
  for +prefix: List<&2, A>
  for +old: A
  for +suffix: List<&2, A>
  for +replacement: A
  {Claims.replace_at(A, Lists.concat(A, prefix, old <> suffix), Views.count(A, prefix), replacement) == Lists.concat(A, prefix, replacement <> suffix) : List<&2, A>}

def replace_at_prefix(A, prefix, old, suffix, replacement):
  match prefix:
    case Nil{}:
      {==}
    case head <> tail:
      %replace_at_prefix(A, tail, old, suffix, replacement) : {head <> Claims.replace_at(A, Lists.concat(A, tail, old <> suffix), Views.count(A, tail), replacement) == head <> _ : List<&2, A>}
      {==}

law replace_past_end:
  for -A: Data
  for +values: List<&2, A>
  for replacement: A
  {Claims.replace_at(A, values, Views.count(A, values), replacement) == values : List<&2, A>}

def replace_past_end(A, values, replacement):
  match values:
    case Nil{}:
      {==}
    case head <> tail:
      %replace_past_end(A, tail, replacement) : {head <> Claims.replace_at(A, tail, Views.count(A, tail), replacement) == head <> _ : List<&2, A>}
      {==}

law exact_swap_remove:
  for -A: Data
  for +prefix: List<&2, A>
  for +removed: A
  for +middle: List<&2, A>
  for +last: A
  {Claims.swap_remove(A, Lists.concat(A, prefix, removed <> Lists.concat(A, middle, last <> Nil{})), Views.count(A, prefix)) == Lists.concat(A, prefix, last <> middle) : List<&2, A>}

def exact_swap_remove(A, prefix, removed, middle, last):
  %ListProof.concat_associative(A, prefix, removed <> middle, last <> Nil{}) : {Claims.swap_remove(A, _, Views.count(A, prefix)) == Lists.concat(A, prefix, last <> middle) : List<&2, A>}
  %Equal.sym(Maybe<&2, Claims.Last<A>>, Claims.pop_last(A, Lists.concat(A, Lists.concat(A, prefix, removed <> middle), last <> Nil{})), Some{Claims.Last{last, Lists.concat(A, prefix, removed <> middle)}}, pop_last_suffix(A, Lists.concat(A, prefix, removed <> middle), last)) : {Claims.swap_last(A, _, Views.count(A, prefix)) == Lists.concat(A, prefix, last <> middle) : List<&2, A>}
  replace_at_prefix(A, prefix, removed, middle, last)

law exact_swap_remove_last:
  for -A: Data
  for +prefix: List<&2, A>
  for +last: A
  {Claims.swap_remove(A, Lists.concat(A, prefix, last <> Nil{}), Views.count(A, prefix)) == prefix : List<&2, A>}

def exact_swap_remove_last(A, prefix, last):
  %Equal.sym(Maybe<&2, Claims.Last<A>>, Claims.pop_last(A, Lists.concat(A, prefix, last <> Nil{})), Some{Claims.Last{last, prefix}}, pop_last_suffix(A, prefix, last)) : {Claims.swap_last(A, _, Views.count(A, prefix)) == prefix : List<&2, A>}
  replace_past_end(A, prefix, last)
