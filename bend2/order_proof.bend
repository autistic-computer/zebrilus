import Base
import ./core.bend as Core
import ./replay.bend as Replay

def reverse_order(order: Cmp) -> Cmp:
  match order:
    case LT{}:
      GT{}
    case EQ{}:
      EQ{}
    case GT{}:
      LT{}

law word_cmp_refl:
  for +n: Nat
  for +a: Word(n)
  {Word.cmp(n, a, a) == EQ{} : Cmp}

def word_cmp_refl(n, a):
  match n:
    case 0n:
      {==}
    case 1n+p:
      match a:
        case WCon{False{}, tail}:
          %Equal.sym(Cmp, Word.cmp(p, tail, tail), EQ{}, word_cmp_refl(p, tail)) : {Word.cmp.fin(False{}, False{}, _) == EQ{} : Cmp}
          {==}
        case WCon{True{}, tail}:
          %Equal.sym(Cmp, Word.cmp(p, tail, tail), EQ{}, word_cmp_refl(p, tail)) : {Word.cmp.fin(True{}, True{}, _) == EQ{} : Cmp}
          {==}

law word_fin_sym:
  for +a: Bool
  for +b: Bool
  for +order: Cmp
  {Word.cmp.fin(b, a, reverse_order(order)) == reverse_order(Word.cmp.fin(a, b, order)) : Cmp}

def word_fin_sym(a, b, order):
  match a b order:
    case _ _ LT{}:
      {==}
    case _ _ GT{}:
      {==}
    case False{} False{} EQ{}:
      {==}
    case False{} True{} EQ{}:
      {==}
    case True{} False{} EQ{}:
      {==}
    case True{} True{} EQ{}:
      {==}

law word_cmp_sym:
  for +n: Nat
  for +a: Word(n)
  for +b: Word(n)
  {Word.cmp(n, b, a) == reverse_order(Word.cmp(n, a, b)) : Cmp}

def word_cmp_sym(n, a, b):
  match n:
    case 0n:
      {==}
    case 1n+p:
      match a b:
        case WCon{ab, at} WCon{bb, bt}:
          %Equal.sym(Cmp, Word.cmp(p, bt, at), reverse_order(Word.cmp(p, at, bt)), word_cmp_sym(p, at, bt)) :
            {Word.cmp.fin(bb, ab, _) == reverse_order(Word.cmp.fin(ab, bb, Word.cmp(p, at, bt))) : Cmp}
          word_fin_sym(ab, bb, Word.cmp(p, at, bt))

law u32_cmp_refl:
  for +a: U32
  {U32.cmp(a, a) == EQ{} : Cmp}

def u32_cmp_refl(a):
  match a:
    case U32{word}:
      word_cmp_refl(32n, word)

law u32_cmp_sym:
  for +a: U32
  for +b: U32
  {U32.cmp(b, a) == reverse_order(U32.cmp(a, b)) : Cmp}

def u32_cmp_sym(a, b):
  match a b:
    case U32{aw} U32{bw}:
      word_cmp_sym(32n, aw, bw)

law compare_words_sym:
  for +high: Cmp
  for +low: Cmp
  {Replay.compare_words(reverse_order(high), reverse_order(low)) == reverse_order(Replay.compare_words(high, low)) : Cmp}

def compare_words_sym(high, low):
  match high:
    case LT{}:
      {==}
    case EQ{}:
      {==}
    case GT{}:
      {==}

law sequence_cmp_refl:
  for +a: Core.StableId
  {Replay.sequence_cmp(a, a) == EQ{} : Cmp}

def sequence_cmp_refl(a):
  match a:
    case Core.Id{high, low}:
      %Equal.sym(Cmp, U32.cmp(high, high), EQ{}, u32_cmp_refl(high)) : {Replay.compare_words(_, U32.cmp(low, low)) == EQ{} : Cmp}
      u32_cmp_refl(low)

law sequence_cmp_sym:
  for +a: Core.StableId
  for +b: Core.StableId
  {Replay.sequence_cmp(b, a) == reverse_order(Replay.sequence_cmp(a, b)) : Cmp}

def sequence_cmp_sym(a, b):
  match a b:
    case Core.Id{ah, al} Core.Id{bh, bl}:
      %Equal.sym(Cmp, U32.cmp(bh, ah), reverse_order(U32.cmp(ah, bh)), u32_cmp_sym(ah, bh)) :
        {Replay.compare_words(_, U32.cmp(bl, al)) == reverse_order(Replay.compare_words(U32.cmp(ah, bh), U32.cmp(al, bl))) : Cmp}
      %Equal.sym(Cmp, U32.cmp(bl, al), reverse_order(U32.cmp(al, bl)), u32_cmp_sym(al, bl)) :
        {Replay.compare_words(reverse_order(U32.cmp(ah, bh)), _) == reverse_order(Replay.compare_words(U32.cmp(ah, bh), U32.cmp(al, bl))) : Cmp}
      compare_words_sym(U32.cmp(ah, bh), U32.cmp(al, bl))

law sequence_ge_refl:
  for +a: Core.StableId
  {Replay.sequence_ge(a, a) == True{} : Bool}

def sequence_ge_refl(a):
  %Equal.sym(Cmp, Replay.sequence_cmp(a, a), EQ{}, sequence_cmp_refl(a)) : {Cmp.is_ge(_) == True{} : Bool}
  {==}

law max_monotonic_case:
  for +a: Core.StableId
  for +b: Core.StableId
  for +order: Cmp
  for +e: {Replay.sequence_cmp(a, b) == order : Cmp}
  {Replay.sequence_ge(Replay.sequence_max(a, b), a) == True{} : Bool}

def max_monotonic_case(a, b, order, e):
  match order:
    case GT{}:
      %Equal.sym(Cmp, Replay.sequence_cmp(a, b), GT{}, e) : {Replay.sequence_ge(Replay.sequence_max.choose(a, b, _), a) == True{} : Bool}
      sequence_ge_refl(a)
    case LT{}:
      %Equal.sym(Cmp, Replay.sequence_cmp(a, b), LT{}, e) : {Replay.sequence_ge(Replay.sequence_max.choose(a, b, _), a) == True{} : Bool}
      %Equal.sym(Cmp, Replay.sequence_cmp(b, a), reverse_order(Replay.sequence_cmp(a, b)), sequence_cmp_sym(a, b)) : {Cmp.is_ge(_) == True{} : Bool}
      %Equal.sym(Cmp, Replay.sequence_cmp(a, b), LT{}, e) : {Cmp.is_ge(reverse_order(_)) == True{} : Bool}
      {==}
    case EQ{}:
      %Equal.sym(Cmp, Replay.sequence_cmp(a, b), EQ{}, e) : {Replay.sequence_ge(Replay.sequence_max.choose(a, b, _), a) == True{} : Bool}
      %Equal.sym(Cmp, Replay.sequence_cmp(b, a), reverse_order(Replay.sequence_cmp(a, b)), sequence_cmp_sym(a, b)) : {Cmp.is_ge(_) == True{} : Bool}
      %Equal.sym(Cmp, Replay.sequence_cmp(a, b), EQ{}, e) : {Cmp.is_ge(reverse_order(_)) == True{} : Bool}
      {==}

law max_monotonic:
  for +a: Core.StableId
  for +b: Core.StableId
  {Replay.sequence_ge(Replay.sequence_max(a, b), a) == True{} : Bool}

def max_monotonic(a, b):
  max_monotonic_case(a, b, Replay.sequence_cmp(a, b), {==})
