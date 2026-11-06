import Base
import ./core.bend as Core
import ./tree_list.bend as Lists

type SortKey is Data:
  SortKey{major: U32, high: U32, low: U32}

type Tagged<-A: Data> is Data:
  Tagged{key: SortKey, value: A}

type Both<-A: Data, -B: Data> is Data:
  Both{first: A, second: B}

def key_equal(a: SortKey, b: SortKey) -> Bool:
  match a b:
    case SortKey{am, ah, al} SortKey{bm, bh, bl}:
      Core.u32_equal(am, bm) && (Core.u32_equal(ah, bh) && Core.u32_equal(al, bl))

def key_word(key: SortKey, upper: Bool, middle: Bool) -> U32:
  match key upper middle:
    case SortKey{major, _, _} True{} _:
      major
    case SortKey{_, high, _} False{} True{}:
      high
    case SortKey{_, _, low} False{} False{}:
      low

def key_bit(key: SortKey, +position: U32) -> Bool:
  U32.is_ne(U32.and(U32.shrn(key_word(key, U32.is_ge(position, 64), U32.is_ge(position, 32)), U32.to_nat(U32.and(position, 31))), 1), 0)

def item_key(-A: Data, item: Tagged<A>) -> SortKey:
  match item:
    case Tagged{key, _}:
      key

def bit_equal(a: Bool, b: Bool) -> Bool:
  Bool.not(Bool.xor(a, b))

def keep(-A: Data, item: Tagged<A>, position: U32, bit: Bool) -> Bool:
  bit_equal(key_bit(item_key(A, item), position), bit)

def filtered(-A: Data, values: List<&2, Tagged<A>>, +position: U32, +bit: Bool) -> List<&2, Tagged<A>>:
  match values:
    case Nil{}:
      Nil{}
    case Con{+head, tail}:
      Lists.selected(Tagged<A>, head, filtered(A, tail, position, bit), keep(A, head, position, bit))

def filter_fold(-A: Data, values: List<&2, Tagged<A>>, +position: U32, +bit: Bool, acc: List<&2, Tagged<A>>) -> List<&2, Tagged<A>>:
  match values:
    case Nil{}:
      Lists.reverse(Tagged<A>, acc)
    case Con{+head, tail}:
      filter_fold(A, tail, position, bit, Lists.selected(Tagged<A>, head, acc, keep(A, head, position, bit)))

def filter(-A: Data, values: List<&2, Tagged<A>>, position: U32, bit: Bool) -> List<&2, Tagged<A>>:
  filter_fold(A, values, position, bit, Nil{})

def radix(-A: Data, +count: Nat, +position: U32, +values: List<&2, Tagged<A>>) -> List<&2, Tagged<A>>:
  match count values:
    case 0n rest:
      rest
    case 1n+_ Nil{}:
      Nil{}
    case 1n+remaining head <> tail:
      zero one = filter(A, head <> tail, position, False{}) filter(A, head <> tail, position, True{})
      left = radix(A, remaining, U32.sub(position, 1), zero)
      right = radix(A, remaining, U32.sub(position, 1), one)
      Lists.append(Tagged<A>, left, right)

def sort(-A: Data, values: List<&2, Tagged<A>>) -> List<&2, Tagged<A>>:
  radix(A, 96n, 95, values)

def value(-A: Data, item: Tagged<A>) -> A:
  match item:
    case Tagged{_, value}:
      value

def values_fold(-A: Data, items: List<&2, Tagged<A>>, acc: List<&2, A>) -> List<&2, A>:
  match items:
    case Nil{}:
      Lists.reverse(A, acc)
    case Tagged{_, value} <> tail:
      values_fold(A, tail, value <> acc)

def values(-A: Data, items: List<&2, Tagged<A>>) -> List<&2, A>:
  values_fold(A, items, Nil{})

def values_for_key(-A: Data, items: List<&2, Tagged<A>>, +key: SortKey) -> List<&2, A>:
  match items:
    case Nil{}:
      Nil{}
    case Tagged{actual, value} <> tail:
      Lists.selected(A, value, values_for_key(A, tail, key), key_equal(actual, key))

def chosen_values(-A: Data, values: List<&2, A>, chosen: Bool) -> List<&2, A>:
  match chosen:
    case False{}:
      Nil{}
    case True{}:
      values

def AllBits(-A: Data, items: List<&2, Tagged<A>>, +position: U32, +bit: Bool) -> Data:
  match items:
    case Nil{}:
      Unit
    case Tagged{key, _} <> tail:
      Both<{key_bit(key, position) == bit : Bool}, AllBits(A, tail, position, bit)>

def Ordered(-A: Data, count: Nat, +position: U32, +source: List<&2, Tagged<A>>, +items: List<&2, Tagged<A>>) -> Type:
  match count source:
    case 0n _:
      Unit
    case 1n+_ Nil{}:
      {items == Nil{} : List<&2, Tagged<A>>}
    case 1n+remaining head <> tail:
      Exists(List<&2, Tagged<A>>, left =>
        Exists(List<&2, Tagged<A>>, right =>
          {items == Lists.concat(Tagged<A>, left, right) : List<&2, Tagged<A>>} &
          (AllBits(A, left, position, False{}) &
          (AllBits(A, right, position, True{}) &
          (Ordered(A, remaining, U32.sub(position, 1), filter(A, head <> tail, position, False{}), left) & Ordered(A, remaining, U32.sub(position, 1), filter(A, head <> tail, position, True{}), right))))))
