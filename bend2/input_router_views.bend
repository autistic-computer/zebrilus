import Base
import ./core.bend as Core
import ./input_router_data.bend as D

type FocusContext is Data:
  FocusContext{keymap: Core.StableId, owner: D.Owner, pending: Maybe<&2, D.Owner>, observed: D.Owner, restore: D.Owner, active: Maybe<&2, D.Active>, capture: Maybe<&2, Core.StableId>}

def context(state: D.State) -> FocusContext:
  match state:
    case D.State{map, owner, pending, observed, restore, active, capture, _, _}:
      FocusContext{map, owner, pending, observed, restore, active, capture}

def transition_state(transition: D.Transition) -> D.State:
  match transition:
    case D.Transition{state, _}:
      state

def count(-A: Data, values: List<&2, A>) -> Nat:
  match values:
    case Nil{}:
      0n
    case _ <> tail:
      1n+count(A, tail)
