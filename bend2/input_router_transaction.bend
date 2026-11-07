import Base
import ./core.bend as Core
import ./input_router_data.bend as D
import ./input_router.bend as Router

type Operation is Data:
  Event{value: D.RouterEvent}
  Capture{value: Maybe<&2, Core.StableId>}
  Suppress{value: D.KeyToken}
  OpKey{input: D.Input, prefix: D.Prefix, lookup: D.Lookups}
  Drain{}
  Plain{releases: Bool, ui_binds: Bool, terminal_binds: Bool}

type Answer is Data:
  NoAnswer{}
  KeyAnswer{value: D.Disposition}
  EffectsAnswer{value: List<&2, D.Effect>}
  PlainAnswer{value: Bool}

type Request is Data:
  Request{state: D.State, operation: Operation}

type Outcome is Data:
  Outcome{state: D.State, answer: Answer}

def key_outcome(value: D.Transition) -> Outcome:
  match value:
    case D.Transition{state, disposition}:
      Outcome{state, KeyAnswer{disposition}}

def drain_outcome(value: D.Drained) -> Outcome:
  match value:
    case D.Drained{state, effects}:
      Outcome{state, EffectsAnswer{effects}}

def apply(request: Request) -> Outcome:
  match request:
    case Request{state, Event{value}}:
      Outcome{Router.event(state, value), NoAnswer{}}
    case Request{state, Capture{value}}:
      Outcome{Router.set_capture(state, value), NoAnswer{}}
    case Request{state, Suppress{value}}:
      Outcome{Router.suppress_release(state, value), NoAnswer{}}
    case Request{state, OpKey{input, prefix, lookup}}:
      key_outcome(Router.key(state, input, prefix, lookup))
    case Request{state, Drain{}}:
      drain_outcome(Router.drain_effects(state))
    case Request{+state, Plain{releases, ui_binds, terminal_binds}}:
      Outcome{state, PlainAnswer{Router.passes_plain_keys(state, releases, ui_binds, terminal_binds)}}

def resolve(request: Request, candidate: Maybe<&2, Outcome>) -> Outcome:
  match candidate:
    case None{}:
      apply(request)
    case Some{value}:
      value

def state(value: Outcome) -> D.State:
  match value:
    case Outcome{state, _}:
      state

def answer(value: Outcome) -> Answer:
  match value:
    case Outcome{_, answer}:
      answer
