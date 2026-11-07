import Base
import ./input_router_data.bend as D
import ./input_router_transaction_laws.bend as Laws

def Laws.failed_worker_uses_unchanged_request(request):
  {==}

def Laws.second_drain_has_no_effects(state):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}
