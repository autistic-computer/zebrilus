import Base
import ./core.bend as Core
import ./id_proof.bend as Id
import ./tree_list.bend as Lists
import ./tree_list_proof.bend as ListProof
import ./tree_sort.bend as Sort

law bit_equal_sound:
  for +a: Bool
  for +b: Bool
  for e: {Sort.bit_equal(a, b) == True{} : Bool}
  {a == b : Bool}

def bit_equal_sound(a, b, e):
  match a b:
    case False{} False{}:
      {==}
    case True{} True{}:
      {==}
    case False{} True{}:
      Id.absurd({False{} == True{} : Bool}, Id.false_is_not_true(e))
    case True{} False{}:
      Id.absurd({True{} == False{} : Bool}, Id.false_is_not_true(e))

law key_equal_sound:
  for +a: Sort.SortKey
  for +b: Sort.SortKey
  for +e: {Sort.key_equal(a, b) == True{} : Bool}
  {a == b : Sort.SortKey}

def key_equal_sound(a, b, e):
  match a b:
    case Sort.SortKey{am, ah, al} Sort.SortKey{bm, bh, bl}:
      +m = Core.u32_equal(am, bm)
      +h = Core.u32_equal(ah, bh)
      +l = Core.u32_equal(al, bl)
      +rest = Id.and_right(m, h && l, e)
      %Id.u32_sound(am, bm, Id.and_left(m, h && l, e)) : {Sort.SortKey{am, ah, al} == Sort.SortKey{_, bh, bl} : Sort.SortKey}
      %Id.u32_sound(ah, bh, Id.and_left(h, l, rest)) : {Sort.SortKey{am, ah, al} == Sort.SortKey{am, _, bl} : Sort.SortKey}
      %Id.u32_sound(al, bl, Id.and_right(h, l, rest)) : {Sort.SortKey{am, ah, al} == Sort.SortKey{am, ah, _} : Sort.SortKey}
      {==}

law fold_selection:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +acc: List<&2, Sort.Tagged<A>>
  for +chosen: Bool
  for no: {Sort.filter_fold(A, tail, position, bit, acc) == Lists.reverse_into(Sort.Tagged<A>, acc, Sort.filtered(A, tail, position, bit)) : List<&2, Sort.Tagged<A>>}
  for yes: {Sort.filter_fold(A, tail, position, bit, head <> acc) == Lists.reverse_into(Sort.Tagged<A>, head <> acc, Sort.filtered(A, tail, position, bit)) : List<&2, Sort.Tagged<A>>}
  {Sort.filter_fold(A, tail, position, bit, Lists.selected(Sort.Tagged<A>, head, acc, chosen)) == Lists.reverse_into(Sort.Tagged<A>, acc, Lists.selected(Sort.Tagged<A>, head, Sort.filtered(A, tail, position, bit), chosen)) : List<&2, Sort.Tagged<A>>}

def fold_selection(A, head, tail, position, bit, acc, chosen, no, yes):
  match chosen:
    case False{}:
      no
    case True{}:
      yes

law filter_fold_spec:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +acc: List<&2, Sort.Tagged<A>>
  {Sort.filter_fold(A, values, position, bit, acc) == Lists.reverse_into(Sort.Tagged<A>, acc, Sort.filtered(A, values, position, bit)) : List<&2, Sort.Tagged<A>>}

def filter_fold_spec(A, values, position, bit, acc):
  match values:
    case Nil{}:
      {==}
    case head <> tail:
      fold_selection(A, head, tail, position, bit, acc, Sort.keep(A, head, position, bit), filter_fold_spec(A, tail, position, bit, acc), filter_fold_spec(A, tail, position, bit, head <> acc))

law filter_is_filtered:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  {Sort.filter(A, values, position, bit) == Sort.filtered(A, values, position, bit) : List<&2, Sort.Tagged<A>>}

def filter_is_filtered(A, values, position, bit):
  filter_fold_spec(A, values, position, bit, Nil{})

law selected_all_bits:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +chosen: Bool
  for e: {Sort.keep(A, head, position, bit) == chosen : Bool}
  for rest: Sort.AllBits(A, tail, position, bit)
  Sort.AllBits(A, Lists.selected(Sort.Tagged<A>, head, tail, chosen), position, bit)

def selected_all_bits(A, head, tail, position, bit, chosen, e, rest):
  match head chosen:
    case Sort.Tagged{_, _} False{}:
      rest
    case Sort.Tagged{key, _} True{}:
      Sort.Both{bit_equal_sound(Sort.key_bit(key, position), bit, e), rest}

law filtered_all_bits:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  Sort.AllBits(A, Sort.filtered(A, values, position, bit), position, bit)

def filtered_all_bits(A, values, position, bit):
  match values:
    case Nil{}:
      Unit{}
    case head <> tail:
      selected_all_bits(A, head, Sort.filtered(A, tail, position, bit), position, bit, Sort.keep(A, head, position, bit), {==}, filtered_all_bits(A, tail, position, bit))

law filter_all_bits:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  Sort.AllBits(A, Sort.filter(A, values, position, bit), position, bit)

def filter_all_bits(A, values, position, bit):
  %Equal.sym(List<&2, Sort.Tagged<A>>, Sort.filter(A, values, position, bit), Sort.filtered(A, values, position, bit), filter_is_filtered(A, values, position, bit)) : Sort.AllBits(A, _, position, bit)
  filtered_all_bits(A, values, position, bit)

law selected_preserves_bits:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +chosen: Bool
  for eh: {Sort.key_bit(Sort.item_key(A, head), position) == bit : Bool}
  for rest: Sort.AllBits(A, tail, position, bit)
  Sort.AllBits(A, Lists.selected(Sort.Tagged<A>, head, tail, chosen), position, bit)

def selected_preserves_bits(A, head, tail, position, bit, chosen, eh, rest):
  match head chosen:
    case Sort.Tagged{_, _} False{}:
      rest
    case Sort.Tagged{_, _} True{}:
      Sort.Both{eh, rest}

law filtered_preserves_bits:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +other_position: U32
  for +other_bit: Bool
  for proof: Sort.AllBits(A, values, other_position, other_bit)
  Sort.AllBits(A, Sort.filtered(A, values, position, bit), other_position, other_bit)

def filtered_preserves_bits(A, values, position, bit, other_position, other_bit, proof):
  match values proof:
    case Nil{} _:
      Unit{}
    case Sort.Tagged{key, value} <> tail Sort.Both{eh, rest}:
      +head = {Sort.Tagged{key, value} : Sort.Tagged<A>}
      selected_preserves_bits(A, head, Sort.filtered(A, tail, position, bit), other_position, other_bit, Sort.keep(A, head, position, bit), eh, filtered_preserves_bits(A, tail, position, bit, other_position, other_bit, rest))

law filter_preserves_bits:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +other_position: U32
  for +other_bit: Bool
  for proof: Sort.AllBits(A, values, other_position, other_bit)
  Sort.AllBits(A, Sort.filter(A, values, position, bit), other_position, other_bit)

def filter_preserves_bits(A, values, position, bit, other_position, other_bit, proof):
  %Equal.sym(List<&2, Sort.Tagged<A>>, Sort.filter(A, values, position, bit), Sort.filtered(A, values, position, bit), filter_is_filtered(A, values, position, bit)) : Sort.AllBits(A, _, other_position, other_bit)
  filtered_preserves_bits(A, values, position, bit, other_position, other_bit, proof)

law concat_all_bits:
  for -A: Data
  for +left: List<&2, Sort.Tagged<A>>
  for +right: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for lp: Sort.AllBits(A, left, position, bit)
  for rp: Sort.AllBits(A, right, position, bit)
  Sort.AllBits(A, Lists.concat(Sort.Tagged<A>, left, right), position, bit)

def concat_all_bits(A, left, right, position, bit, lp, rp):
  match left lp:
    case Nil{} _:
      rp
    case Sort.Tagged{_, _} <> tail Sort.Both{eh, rest}:
      Sort.Both{eh, concat_all_bits(A, tail, right, position, bit, rest, rp)}

law radix_preserves_bits:
  for -A: Data
  for +count: Nat
  for +position: U32
  for +values: List<&2, Sort.Tagged<A>>
  for +other_position: U32
  for +other_bit: Bool
  for +proof: Sort.AllBits(A, values, other_position, other_bit)
  Sort.AllBits(A, Sort.radix(A, count, position, values), other_position, other_bit)

def radix_preserves_bits(A, count, position, values, other_position, other_bit, proof):
  match count values:
    case 0n _:
      proof
    case 1n+_ Nil{}:
      Unit{}
    case 1n+remaining head <> tail:
      +left = Sort.radix(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, False{}))
      +right = Sort.radix(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, True{}))
      %Equal.sym(List<&2, Sort.Tagged<A>>, Lists.append(Sort.Tagged<A>, left, right), Lists.concat(Sort.Tagged<A>, left, right), ListProof.append_is_concat(Sort.Tagged<A>, left, right)) : Sort.AllBits(A, _, other_position, other_bit)
      concat_all_bits(A, left, right, other_position, other_bit,
        radix_preserves_bits(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, False{}), other_position, other_bit, filter_preserves_bits(A, head <> tail, position, False{}, other_position, other_bit, proof)),
        radix_preserves_bits(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, True{}), other_position, other_bit, filter_preserves_bits(A, head <> tail, position, True{}, other_position, other_bit, proof)))

law empty_ordered:
  for -A: Data
  for +count: Nat
  for +position: U32
  Sort.Ordered(A, count, position, Nil{}, Nil{})

def empty_ordered(A, count, position):
  match count:
    case 0n:
      Unit{}
    case 1n+remaining:
      {==}

law radix_ordered:
  for -A: Data
  for +count: Nat
  for +position: U32
  for +values: List<&2, Sort.Tagged<A>>
  Sort.Ordered(A, count, position, values, Sort.radix(A, count, position, values))

def radix_ordered(A, count, position, values):
  match count values:
    case 0n _:
      Unit{}
    case 1n+remaining Nil{}:
      {==}
    case 1n+remaining head <> tail:
      +left = Sort.radix(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, False{}))
      +right = Sort.radix(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, True{}))
      (left, (right, (ListProof.append_is_concat(Sort.Tagged<A>, left, right),
        (radix_preserves_bits(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, False{}), position, False{}, filter_all_bits(A, head <> tail, position, False{})),
        (radix_preserves_bits(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, True{}), position, True{}, filter_all_bits(A, head <> tail, position, True{})),
        (radix_ordered(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, False{})), radix_ordered(A, remaining, U32.sub(position, 1), Sort.filter(A, head <> tail, position, True{}))))))))

law and_false:
  for +value: Bool
  {value && False{} == False{} : Bool}

def and_false(value):
  match value:
    case False{}:
      {==}
    case True{}:
      {==}

law equal_keys_have_equal_bits:
  for +actual: Sort.SortKey
  for +target: Sort.SortKey
  for +position: U32
  for +bit: Bool
  for e: {Sort.key_equal(actual, target) == True{} : Bool}
  {Sort.bit_equal(Sort.key_bit(actual, position), bit) == Sort.bit_equal(Sort.key_bit(target, position), bit) : Bool}

def equal_keys_have_equal_bits(actual, target, position, bit, e):
  Equal.cong(Sort.SortKey, Bool, key => Sort.bit_equal(Sort.key_bit(key, position), bit), actual, target, key_equal_sound(actual, target, e))

law keep_alignment_case:
  for +actual: Sort.SortKey
  for +target: Sort.SortKey
  for +position: U32
  for +bit: Bool
  for +same: Bool
  for e: {Sort.key_equal(actual, target) == same : Bool}
  {Sort.bit_equal(Sort.key_bit(actual, position), bit) && same == Sort.bit_equal(Sort.key_bit(target, position), bit) && same : Bool}

def keep_alignment_case(actual, target, position, bit, same, e):
  match same:
    case False{}:
      Equal.trans(Bool, Sort.bit_equal(Sort.key_bit(actual, position), bit) && False{}, False{}, Sort.bit_equal(Sort.key_bit(target, position), bit) && False{},
        and_false(Sort.bit_equal(Sort.key_bit(actual, position), bit)), Equal.sym(Bool, Sort.bit_equal(Sort.key_bit(target, position), bit) && False{}, False{}, and_false(Sort.bit_equal(Sort.key_bit(target, position), bit))))
    case True{}:
      Equal.cong(Bool, Bool, value => value && True{}, Sort.bit_equal(Sort.key_bit(actual, position), bit), Sort.bit_equal(Sort.key_bit(target, position), bit), equal_keys_have_equal_bits(actual, target, position, bit, e))

law project_selected:
  for -A: Data
  for +head: Sort.Tagged<A>
  for +tail: List<&2, Sort.Tagged<A>>
  for +key: Sort.SortKey
  for +chosen: Bool
  {Sort.values_for_key(A, Lists.selected(Sort.Tagged<A>, head, tail, chosen), key) == Lists.selected(A, Sort.value(A, head), Sort.values_for_key(A, tail, key), chosen && Sort.key_equal(Sort.item_key(A, head), key)) : List<&2, A>}

def project_selected(A, head, tail, key, chosen):
  match head chosen:
    case Sort.Tagged{_, _} False{}:
      {==}
    case Sort.Tagged{_, _} True{}:
      {==}

law choose_selected:
  for -A: Data
  for +head: A
  for +tail: List<&2, A>
  for +same: Bool
  for +chosen: Bool
  {Sort.chosen_values(A, Lists.selected(A, head, tail, same), chosen) == Lists.selected(A, head, Sort.chosen_values(A, tail, chosen), chosen && same) : List<&2, A>}

def choose_selected(A, head, tail, same, chosen):
  match chosen:
    case False{}:
      {==}
    case True{}:
      {==}

law chosen_empty:
  for -A: Data
  for +chosen: Bool
  {Nil{} == Sort.chosen_values(A, Nil{}, chosen) : List<&2, A>}

def chosen_empty(A, chosen):
  match chosen:
    case False{}:
      {==}
    case True{}:
      {==}

law filtered_stable:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +key: Sort.SortKey
  {Sort.values_for_key(A, Sort.filtered(A, values, position, bit), key) == Sort.chosen_values(A, Sort.values_for_key(A, values, key), Sort.bit_equal(Sort.key_bit(key, position), bit)) : List<&2, A>}

def filtered_stable(A, values, position, bit, key):
  match values:
    case Nil{}:
      chosen_empty(A, Sort.bit_equal(Sort.key_bit(key, position), bit))
    case Sort.Tagged{actual, item} <> tail:
      +head = {Sort.Tagged{actual, item} : Sort.Tagged<A>}
      +rest = Sort.filtered(A, tail, position, bit)
      +projected = Sort.values_for_key(A, tail, key)
      +hk = Sort.keep(A, head, position, bit)
      +tk = Sort.bit_equal(Sort.key_bit(key, position), bit)
      +same = Sort.key_equal(actual, key)
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, Lists.selected(Sort.Tagged<A>, head, rest, hk), key), Lists.selected(A, item, Sort.values_for_key(A, rest, key), hk && same), project_selected(A, head, rest, key, hk)) : {_ == Sort.chosen_values(A, Lists.selected(A, item, projected, same), tk) : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, rest, key), Sort.chosen_values(A, projected, tk), filtered_stable(A, tail, position, bit, key)) : {Lists.selected(A, item, _, hk && same) == Sort.chosen_values(A, Lists.selected(A, item, projected, same), tk) : List<&2, A>}
      %Equal.sym(Bool, hk && same, tk && same, keep_alignment_case(actual, key, position, bit, same, {==})) : {Lists.selected(A, item, Sort.chosen_values(A, projected, tk), _) == Sort.chosen_values(A, Lists.selected(A, item, projected, same), tk) : List<&2, A>}
      Equal.sym(List<&2, A>, Sort.chosen_values(A, Lists.selected(A, item, projected, same), tk), Lists.selected(A, item, Sort.chosen_values(A, projected, tk), tk && same), choose_selected(A, item, projected, same, tk))

law filter_stable:
  for -A: Data
  for +values: List<&2, Sort.Tagged<A>>
  for +position: U32
  for +bit: Bool
  for +key: Sort.SortKey
  {Sort.values_for_key(A, Sort.filter(A, values, position, bit), key) == Sort.chosen_values(A, Sort.values_for_key(A, values, key), Sort.bit_equal(Sort.key_bit(key, position), bit)) : List<&2, A>}

def filter_stable(A, values, position, bit, key):
  %Equal.sym(List<&2, Sort.Tagged<A>>, Sort.filter(A, values, position, bit), Sort.filtered(A, values, position, bit), filter_is_filtered(A, values, position, bit)) : {Sort.values_for_key(A, _, key) == Sort.chosen_values(A, Sort.values_for_key(A, values, key), Sort.bit_equal(Sort.key_bit(key, position), bit)) : List<&2, A>}
  filtered_stable(A, values, position, bit, key)

law concat_selected:
  for -A: Data
  for +head: A
  for +tail: List<&2, A>
  for +rest: List<&2, A>
  for +chosen: Bool
  {Lists.concat(A, Lists.selected(A, head, tail, chosen), rest) == Lists.selected(A, head, Lists.concat(A, tail, rest), chosen) : List<&2, A>}

def concat_selected(A, head, tail, rest, chosen):
  match chosen:
    case False{}:
      {==}
    case True{}:
      {==}

law project_concat:
  for -A: Data
  for +left: List<&2, Sort.Tagged<A>>
  for +right: List<&2, Sort.Tagged<A>>
  for +key: Sort.SortKey
  {Sort.values_for_key(A, Lists.concat(Sort.Tagged<A>, left, right), key) == Lists.concat(A, Sort.values_for_key(A, left, key), Sort.values_for_key(A, right, key)) : List<&2, A>}

def project_concat(A, left, right, key):
  match left:
    case Nil{}:
      {==}
    case Sort.Tagged{actual, item} <> tail:
      +l = Sort.values_for_key(A, tail, key)
      +r = Sort.values_for_key(A, right, key)
      +same = Sort.key_equal(actual, key)
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, Lists.concat(Sort.Tagged<A>, tail, right), key), Lists.concat(A, l, r), project_concat(A, tail, right, key)) : {Lists.selected(A, item, _, same) == Lists.concat(A, Lists.selected(A, item, l, same), r) : List<&2, A>}
      Equal.sym(List<&2, A>, Lists.concat(A, Lists.selected(A, item, l, same), r), Lists.selected(A, item, Lists.concat(A, l, r), same), concat_selected(A, item, l, r, same))

law partition_values:
  for -A: Data
  for +values: List<&2, A>
  for +bit: Bool
  {Lists.concat(A, Sort.chosen_values(A, values, Sort.bit_equal(bit, False{})), Sort.chosen_values(A, values, Sort.bit_equal(bit, True{}))) == values : List<&2, A>}

def partition_values(A, values, bit):
  match bit:
    case False{}:
      ListProof.concat_empty(A, values)
    case True{}:
      {==}

law radix_stable:
  for -A: Data
  for +count: Nat
  for +position: U32
  for +values: List<&2, Sort.Tagged<A>>
  for +key: Sort.SortKey
  {Sort.values_for_key(A, Sort.radix(A, count, position, values), key) == Sort.values_for_key(A, values, key) : List<&2, A>}

def radix_stable(A, count, position, values, key):
  match count values:
    case 0n _:
      {==}
    case 1n+_ Nil{}:
      {==}
    case 1n+remaining head <> tail:
      +input = {head <> tail : List<&2, Sort.Tagged<A>>}
      +zero = Sort.filter(A, input, position, False{})
      +one = Sort.filter(A, input, position, True{})
      +left = Sort.radix(A, remaining, U32.sub(position, 1), zero)
      +right = Sort.radix(A, remaining, U32.sub(position, 1), one)
      +projected = Sort.values_for_key(A, input, key)
      %Equal.sym(List<&2, Sort.Tagged<A>>, Lists.append(Sort.Tagged<A>, left, right), Lists.concat(Sort.Tagged<A>, left, right), ListProof.append_is_concat(Sort.Tagged<A>, left, right)) : {Sort.values_for_key(A, _, key) == projected : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, Lists.concat(Sort.Tagged<A>, left, right), key), Lists.concat(A, Sort.values_for_key(A, left, key), Sort.values_for_key(A, right, key)), project_concat(A, left, right, key)) : {_ == projected : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, left, key), Sort.values_for_key(A, zero, key), radix_stable(A, remaining, U32.sub(position, 1), zero, key)) : {Lists.concat(A, _, Sort.values_for_key(A, right, key)) == projected : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, right, key), Sort.values_for_key(A, one, key), radix_stable(A, remaining, U32.sub(position, 1), one, key)) : {Lists.concat(A, Sort.values_for_key(A, zero, key), _) == projected : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, zero, key), Sort.chosen_values(A, projected, Sort.bit_equal(Sort.key_bit(key, position), False{})), filter_stable(A, input, position, False{}, key)) : {Lists.concat(A, _, Sort.values_for_key(A, one, key)) == projected : List<&2, A>}
      %Equal.sym(List<&2, A>, Sort.values_for_key(A, one, key), Sort.chosen_values(A, projected, Sort.bit_equal(Sort.key_bit(key, position), True{})), filter_stable(A, input, position, True{}, key)) : {Lists.concat(A, Sort.chosen_values(A, projected, Sort.bit_equal(Sort.key_bit(key, position), False{})), _) == projected : List<&2, A>}
      partition_values(A, projected, Sort.key_bit(key, position))
