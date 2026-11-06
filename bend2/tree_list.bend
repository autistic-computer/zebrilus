import Base

def concat(-A: Data, left: List<&2, A>, right: List<&2, A>) -> List<&2, A>:
  match left:
    case Nil{}:
      right
    case head <> tail:
      head <> concat(A, tail, right)

def reverse_into(-A: Data, values: List<&2, A>, acc: List<&2, A>) -> List<&2, A>:
  match values:
    case Nil{}:
      acc
    case head <> tail:
      reverse_into(A, tail, head <> acc)

def reverse(-A: Data, values: List<&2, A>) -> List<&2, A>:
  reverse_into(A, values, Nil{})

def append(-A: Data, left: List<&2, A>, right: List<&2, A>) -> List<&2, A>:
  reverse_into(A, reverse(A, left), right)

def selected(-A: Data, head: A, tail: List<&2, A>, keep: Bool) -> List<&2, A>:
  match keep:
    case False{}:
      tail
    case True{}:
      head <> tail

def length_fold(-A: Data, values: List<&2, A>, count: Nat) -> Nat:
  match values:
    case Nil{}:
      count
    case _ <> tail:
      length_fold(A, tail, 1n+count)

def length(-A: Data, values: List<&2, A>) -> Nat:
  length_fold(A, values, 0n)
