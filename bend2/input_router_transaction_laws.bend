import Base
import ./input_router_data.bend as D
import ./input_router_transaction.bend as T

law failed_worker_uses_unchanged_request:
  for +request: T.Request
  {T.resolve(request, None{}) == T.apply(request) : T.Outcome}

law second_drain_has_no_effects:
  for state: D.State
  {T.answer(T.apply(T.Request{T.state(T.apply(T.Request{state, T.Drain{}})), T.Drain{}})) == T.EffectsAnswer{Nil{}} : T.Answer}
