import Base
import ./tree_list.bend as Lists

law concat_empty:
  for -A: Data
  for +values: List<&2, A>
  {Lists.concat(A, values, Nil{}) == values : List<&2, A>}

def concat_empty(A, values):
  match values:
    case Nil{}:
      {==}
    case head <> tail:
      %concat_empty(A, tail) : {head <> Lists.concat(A, tail, Nil{}) == head <> _ : List<&2, A>}
      {==}

law concat_associative:
  for -A: Data
  for +first: List<&2, A>
  for +second: List<&2, A>
  for +third: List<&2, A>
  {Lists.concat(A, Lists.concat(A, first, second), third) == Lists.concat(A, first, Lists.concat(A, second, third)) : List<&2, A>}

def concat_associative(A, first, second, third):
  match first:
    case Nil{}:
      {==}
    case head <> tail:
      %concat_associative(A, tail, second, third) : {head <> Lists.concat(A, Lists.concat(A, tail, second), third) == head <> _ : List<&2, A>}
      {==}

law reverse_into_concat:
  for -A: Data
  for +values: List<&2, A>
  for +acc: List<&2, A>
  for +tail: List<&2, A>
  {Lists.reverse_into(A, values, Lists.concat(A, acc, tail)) == Lists.concat(A, Lists.reverse_into(A, values, acc), tail) : List<&2, A>}

def reverse_into_concat(A, values, acc, tail):
  match values:
    case Nil{}:
      {==}
    case head <> rest:
      reverse_into_concat(A, rest, head <> acc, tail)

law reverse_into_twice:
  for -A: Data
  for +values: List<&2, A>
  for +acc: List<&2, A>
  for +tail: List<&2, A>
  {Lists.reverse_into(A, Lists.reverse_into(A, values, acc), tail) == Lists.reverse_into(A, acc, Lists.concat(A, values, tail)) : List<&2, A>}

def reverse_into_twice(A, values, acc, tail):
  match values:
    case Nil{}:
      {==}
    case head <> rest:
      reverse_into_twice(A, rest, head <> acc, tail)

law reverse_twice:
  for -A: Data
  for +values: List<&2, A>
  {Lists.reverse(A, Lists.reverse(A, values)) == values : List<&2, A>}

def reverse_twice(A, values):
  %Equal.sym(List<&2, A>, Lists.reverse_into(A, Lists.reverse_into(A, values, Nil{}), Nil{}), Lists.concat(A, values, Nil{}), reverse_into_twice(A, values, Nil{}, Nil{})) : {_ == values : List<&2, A>}
  concat_empty(A, values)

law append_is_concat:
  for -A: Data
  for +left: List<&2, A>
  for +right: List<&2, A>
  {Lists.append(A, left, right) == Lists.concat(A, left, right) : List<&2, A>}

def append_is_concat(A, left, right):
  %Equal.sym(List<&2, A>, Lists.reverse_into(A, Lists.reverse(A, left), right), Lists.concat(A, Lists.reverse(A, Lists.reverse(A, left)), right), reverse_into_concat(A, Lists.reverse(A, left), Nil{}, right)) : {_ == Lists.concat(A, left, right) : List<&2, A>}
  %Equal.sym(List<&2, A>, Lists.reverse(A, Lists.reverse(A, left)), left, reverse_twice(A, left)) : {Lists.concat(A, _, right) == Lists.concat(A, left, right) : List<&2, A>}
  {==}
