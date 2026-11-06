import Base
import ./core.bend as Core

def absurd(-A: Type, e: Empty) -> A:
  match e:

def true_forbidden(b: Bool) -> Type:
  match b:
    case False{}:
      Unit
    case True{}:
      Empty

law false_is_not_true:
  for e: {False{} == True{} : Bool}
  Empty

def false_is_not_true(e):
  %e : true_forbidden(_)
  Unit{}

law bits_refl:
  for +n: Nat
  for +a: Word(n)
  {Core.bits_equal(n, a, a) == True{} : Bool}

def bits_refl(n, a):
  match n:
    case 0n:
      {==}
    case 1n+p:
      match a:
        case WCon{x, xs}:
          match x:
            case False{}:
              bits_refl(p, xs)
            case True{}:
              bits_refl(p, xs)

law bits_sound:
  for +n: Nat
  for +a: Word(n)
  for +b: Word(n)
  for e: {Core.bits_equal(n, a, b) == True{} : Bool}
  {a == b : Word(n)}

def bits_sound(n, a, b, e):
  match n:
    case 0n:
      match a b:
        case WNil{} WNil{}:
          {==}
    case 1n+p:
      match a b:
        case WCon{False{}, xs} WCon{False{}, ys}:
          %bits_sound(p, xs, ys, e) : {WCon{False{}, xs} == WCon{False{}, _} : Word(1n+p)}
          {==}
        case WCon{True{}, xs} WCon{True{}, ys}:
          %bits_sound(p, xs, ys, e) : {WCon{True{}, xs} == WCon{True{}, _} : Word(1n+p)}
          {==}
        case WCon{False{}, _} WCon{True{}, _}:
          absurd({a == b : Word(1n+p)}, false_is_not_true(e))
        case WCon{True{}, _} WCon{False{}, _}:
          absurd({a == b : Word(1n+p)}, false_is_not_true(e))

law u32_refl:
  for +a: U32
  {Core.u32_equal(a, a) == True{} : Bool}

def u32_refl(a):
  match a:
    case U32{x}:
      bits_refl(32n, x)

law u32_sound:
  for +a: U32
  for +b: U32
  for e: {Core.u32_equal(a, b) == True{} : Bool}
  {a == b : U32}

def u32_sound(a, b, e):
  match a b:
    case U32{x} U32{y}:
      Equal.cong(Word(32n), U32, w => U32{w}, x, y, bits_sound(32n, x, y, e))

law and_left:
  for +a: Bool
  for b: Bool
  for e: {Bool.and(a, b) == True{} : Bool}
  {a == True{} : Bool}

def and_left(a, b, e):
  match a:
    case False{}:
      absurd({a == True{} : Bool}, false_is_not_true(e))
    case True{}:
      {==}

law and_right:
  for a: Bool
  for +b: Bool
  for e: {Bool.and(a, b) == True{} : Bool}
  {b == True{} : Bool}

def and_right(a, b, e):
  match a:
    case False{}:
      absurd({b == True{} : Bool}, false_is_not_true(e))
    case True{}:
      e

law id_refl:
  for +a: Core.StableId
  {Core.id_equal(a, a) == True{} : Bool}

def id_refl(a):
  match a:
    case Core.Id{high, low}:
      %Equal.sym(Bool, Core.u32_equal(high, high), True{}, u32_refl(high)) : {Bool.and(_, Core.u32_equal(low, low)) == True{} : Bool}
      u32_refl(low)

law id_sound:
  for +a: Core.StableId
  for +b: Core.StableId
  for +e: {Core.id_equal(a, b) == True{} : Bool}
  {a == b : Core.StableId}

def id_sound(a, b, e):
  match a b:
    case Core.Id{ah, al} Core.Id{bh, bl}:
      +eh = Core.u32_equal(ah, bh)
      +el = Core.u32_equal(al, bl)
      %u32_sound(ah, bh, and_left(eh, el, e)) : {Core.Id{ah, al} == Core.Id{_, bl} : Core.StableId}
      %u32_sound(al, bl, and_right(eh, el, e)) : {Core.Id{ah, al} == Core.Id{ah, _} : Core.StableId}
      {==}

law swap_source:
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.swap_id(source, source, target) == target : Core.StableId}

def swap_source(source, target):
  %Equal.sym(Bool, Core.id_equal(source, source), True{}, id_refl(source)) :
    {Core.swap_id.choose(source, source, target, _, Core.id_equal(source, target)) == target : Core.StableId}
  {==}

law swap_target_case:
  for +source: Core.StableId
  for +target: Core.StableId
  for +same: Bool
  for +e: {Core.id_equal(target, source) == same : Bool}
  {Core.swap_id(target, source, target) == source : Core.StableId}

def swap_target_case(source, target, same, e):
  match same:
    case True{}:
      %Equal.sym(Bool, Core.id_equal(target, source), True{}, e) :
        {Core.swap_id.choose(target, source, target, _, Core.id_equal(target, target)) == source : Core.StableId}
      id_sound(target, source, e)
    case False{}:
      %Equal.sym(Bool, Core.id_equal(target, source), False{}, e) :
        {Core.swap_id.choose(target, source, target, _, Core.id_equal(target, target)) == source : Core.StableId}
      %Equal.sym(Bool, Core.id_equal(target, target), True{}, id_refl(target)) :
        {Core.swap_id.choose(target, source, target, False{}, _) == source : Core.StableId}
      {==}

law swap_target:
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.swap_id(target, source, target) == source : Core.StableId}

def swap_target(source, target):
  swap_target_case(source, target, Core.id_equal(target, source), {==})

law swap_other:
  for +pane: Core.StableId
  for +source: Core.StableId
  for +target: Core.StableId
  for es: {Core.id_equal(pane, source) == False{} : Bool}
  for et: {Core.id_equal(pane, target) == False{} : Bool}
  {Core.swap_id(pane, source, target) == pane : Core.StableId}

def swap_other(pane, source, target, es, et):
  %Equal.sym(Bool, Core.id_equal(pane, source), False{}, es) :
    {Core.swap_id.choose(pane, source, target, _, Core.id_equal(pane, target)) == pane : Core.StableId}
  %Equal.sym(Bool, Core.id_equal(pane, target), False{}, et) :
    {Core.swap_id.choose(pane, source, target, False{}, _) == pane : Core.StableId}
  {==}

law swap_twice_case:
  for +pane: Core.StableId
  for +source: Core.StableId
  for +target: Core.StableId
  for +is_source: Bool
  for +is_target: Bool
  for +es: {Core.id_equal(pane, source) == is_source : Bool}
  for +et: {Core.id_equal(pane, target) == is_target : Bool}
  {Core.swap_id(Core.swap_id(pane, source, target), source, target) == pane : Core.StableId}

def swap_twice_case(pane, source, target, is_source, is_target, es, et):
  match is_source is_target:
    case True{} _:
      %Equal.sym(Core.StableId, pane, source, id_sound(pane, source, es)) :
        {Core.swap_id(Core.swap_id(_, source, target), source, target) == _ : Core.StableId}
      Equal.trans(Core.StableId, Core.swap_id(Core.swap_id(source, source, target), source, target), Core.swap_id(target, source, target), source,
        Equal.cong(Core.StableId, Core.StableId, x => Core.swap_id(x, source, target), Core.swap_id(source, source, target), target, swap_source(source, target)),
        swap_target(source, target))
    case False{} True{}:
      %Equal.sym(Core.StableId, pane, target, id_sound(pane, target, et)) :
        {Core.swap_id(Core.swap_id(_, source, target), source, target) == _ : Core.StableId}
      Equal.trans(Core.StableId, Core.swap_id(Core.swap_id(target, source, target), source, target), Core.swap_id(source, source, target), target,
        Equal.cong(Core.StableId, Core.StableId, x => Core.swap_id(x, source, target), Core.swap_id(target, source, target), source, swap_target(source, target)),
        swap_source(source, target))
    case False{} False{}:
      Equal.trans(Core.StableId, Core.swap_id(Core.swap_id(pane, source, target), source, target), Core.swap_id(pane, source, target), pane,
        Equal.cong(Core.StableId, Core.StableId, x => Core.swap_id(x, source, target), Core.swap_id(pane, source, target), pane, swap_other(pane, source, target, es, et)),
        swap_other(pane, source, target, es, et))

law swap_twice:
  for +pane: Core.StableId
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.swap_id(Core.swap_id(pane, source, target), source, target) == pane : Core.StableId}

def swap_twice(pane, source, target):
  swap_twice_case(pane, source, target, Core.id_equal(pane, source), Core.id_equal(pane, target), {==}, {==})
