import Base
import ./core.bend as Core
import ./tree_list.bend as Lists
import ./input_router_data.bend as D

type Last<-A: Data> is Data:
  Last{head: A, rest: List<&2, A>}

type Found is Data:
  Found{index: Nat, pane: Core.StableId}

type Released is Data:
  Released{claims: D.Claims, pane: Maybe<&2, Core.StableId>}

type LocalRelease is Data:
  LocalRelease{claims: D.Claims, consumed: Bool}

type PressResult is Data:
  PressResult{claims: D.Claims, autorepeat: Bool, stale: Bool}

def last_fold(-A: Data, values: List<&2, A>, previous: A, reversed: List<&2, A>) -> Last<A>:
  match values:
    case Nil{}:
      Last{previous, Lists.reverse(A, reversed)}
    case head <> tail:
      last_fold(A, tail, head, previous <> reversed)

def pop_last(-A: Data, values: List<&2, A>) -> Maybe<&2, Last<A>>:
  match values:
    case Nil{}:
      None{}
    case head <> tail:
      Some{last_fold(A, tail, head, Nil{})}

def replace_at(-A: Data, values: List<&2, A>, index: Nat, replacement: A) -> List<&2, A>:
  match values index:
    case Nil{} _:
      Nil{}
    case _ <> tail 0n:
      replacement <> tail
    case head <> tail 1n+remaining:
      head <> replace_at(A, tail, remaining, replacement)

def swap_last(-A: Data, value: Maybe<&2, Last<A>>, index: Nat) -> List<&2, A>:
  match value:
    case None{}:
      Nil{}
    case Some{Last{head, rest}}:
      replace_at(A, rest, index, head)

def swap_remove(-A: Data, values: List<&2, A>, index: Nat) -> List<&2, A>:
  swap_last(A, pop_last(A, values), index)

def found_choice(same: Bool, index: Nat, pane: Core.StableId, rest: Maybe<&2, Found>) -> Maybe<&2, Found>:
  match same:
    case True{}:
      Some{Found{index, pane}}
    case False{}:
      rest

def find_held(values: List<&2, D.Held>, +key: D.KeyToken, +index: Nat) -> Maybe<&2, Found>:
  match values:
    case Nil{}:
      None{}
    case D.Held{held, pane} <> tail:
      found_choice(D.key_equal(held, key), index, pane, find_held(tail, key, 1n+index))

def index_choice(same: Bool, index: Nat, rest: Maybe<&2, Nat>) -> Maybe<&2, Nat>:
  match same:
    case True{}:
      Some{index}
    case False{}:
      rest

def find_local(values: List<&2, D.KeyToken>, +key: D.KeyToken, +index: Nat) -> Maybe<&2, Nat>:
  match values:
    case Nil{}:
      None{}
    case head <> tail:
      index_choice(D.key_equal(head, key), index, find_local(tail, key, 1n+index))

def remove_local(values: List<&2, D.KeyToken>, +key: D.KeyToken) -> List<&2, D.KeyToken>:
  match values:
    case Nil{}:
      Nil{}
    case +head <> tail:
      Lists.selected(D.KeyToken, head, remove_local(tail, key), Bool.not(D.key_equal(head, key)))

def released(found: Maybe<&2, Found>, held: List<&2, D.Held>, local: List<&2, D.KeyToken>) -> Released:
  match found:
    case None{}:
      Released{D.Claims{held, local}, None{}}
    case Some{Found{index, pane}}:
      Released{D.Claims{swap_remove(D.Held, held, index), local}, Some{pane}}

def consume_release(claims: D.Claims, +key: D.KeyToken) -> Released:
  match claims:
    case D.Claims{+held, local}:
      released(find_held(held, key, 0n), held, local)

def local_consumed(result: Released) -> LocalRelease:
  match result:
    case Released{claims, _}:
      LocalRelease{claims, True{}}

def consume_local_found(found: Maybe<&2, Nat>, claims: D.Claims, key: D.KeyToken) -> LocalRelease:
  match found claims:
    case None{} original:
      LocalRelease{original, False{}}
    case Some{index} D.Claims{held, local}:
      local_consumed(consume_release(D.Claims{held, swap_remove(D.KeyToken, local, index)}, key))

def consume_local_release(+claims: D.Claims, +key: D.KeyToken) -> LocalRelease:
  match claims:
    case D.Claims{held, +local}:
      consume_local_found(find_local(local, key, 0n), D.Claims{held, local}, key)

def suppress_choice(found: Maybe<&2, Nat>, held: List<&2, D.Held>, local: List<&2, D.KeyToken>, key: D.KeyToken) -> D.Claims:
  match found:
    case None{}:
      D.Claims{held, Lists.append(D.KeyToken, local, key <> Nil{})}
    case Some{_}:
      D.Claims{held, local}

def suppress_release(claims: D.Claims, +key: D.KeyToken) -> D.Claims:
  match claims:
    case D.Claims{held, +local}:
      suppress_choice(find_local(local, key, 0n), held, local, key)

def was_stale(pane: Maybe<&2, Core.StableId>) -> Bool:
  match pane:
    case None{}:
      False{}
    case Some{_}:
      True{}

def pressed_release(result: Released, key: D.KeyToken, pane: Core.StableId) -> PressResult:
  match result:
    case Released{D.Claims{held, local}, old}:
      PressResult{D.Claims{Lists.append(D.Held, held, D.Held{key, pane} <> Nil{}), local}, False{}, was_stale(old)}

def press(claims: D.Claims, +key: D.KeyToken, pane: Core.StableId, repeat: Bool) -> PressResult:
  match claims repeat:
    case original True{}:
      PressResult{original, True{}, False{}}
    case D.Claims{held, local} False{}:
      pressed_release(consume_release(D.Claims{held, remove_local(local, key)}, key), key, pane)

def retain_held(values: List<&2, D.Held>, +pane: Core.StableId) -> List<&2, D.Held>:
  match values:
    case Nil{}:
      Nil{}
    case +head <> tail:
      match head:
        case D.Held{_, owner}:
          Lists.selected(D.Held, head, retain_held(tail, pane), Bool.not(Core.id_equal(owner, pane)))

def matches_pane(held: List<&2, D.Held>, +key: D.KeyToken, +pane: Core.StableId) -> Bool:
  match held:
    case Nil{}:
      False{}
    case D.Held{token, owner} <> tail:
      (D.key_equal(token, key) && Core.id_equal(owner, pane)) || matches_pane(tail, key, pane)

def retain_local(local: List<&2, D.KeyToken>, +held: List<&2, D.Held>, +pane: Core.StableId) -> List<&2, D.KeyToken>:
  match local:
    case Nil{}:
      Nil{}
    case +key <> tail:
      Lists.selected(D.KeyToken, key, retain_local(tail, held, pane), Bool.not(matches_pane(held, key, pane)))

def remove_pane(claims: D.Claims, +pane: Core.StableId) -> D.Claims:
  match claims:
    case D.Claims{+held, local}:
      next_held next_local = retain_held(held, pane) retain_local(local, held, pane)
      D.Claims{next_held, next_local}
