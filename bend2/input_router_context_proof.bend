import Base
import ./core.bend as Core
import ./input_router_data.bend as D
import ./input_router_claims.bend as Claims
import ./input_router.bend as Router
import ./input_router_views.bend as Views

law claims_preserve_context:
  for +state: D.State
  for +claims: D.Claims
  {Views.context(Router.with_claims(state, claims)) == Views.context(state) : Views.FocusContext}

def claims_preserve_context(state, claims):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law effect_preserves_context:
  for +state: D.State
  for +effect: D.Effect
  {Views.context(Router.with_effect(state, effect)) == Views.context(state) : Views.FocusContext}

def effect_preserves_context(state, effect):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law clear_local_preserves_context:
  for +state: D.State
  for +key: D.KeyToken
  {Views.context(Router.clear_local(state, key)) == Views.context(state) : Views.FocusContext}

def clear_local_preserves_context(state, key):
  match state:
    case D.State{_, _, _, _, _, _, _, D.Claims{_, _}, _}:
      {==}

law native_preserves_context:
  for +state: D.State
  {Views.context(Views.transition_state(Router.native(state))) == Views.context(state) : Views.FocusContext}

def native_preserves_context(state):
  {==}

law chrome_preserves_context:
  for +state: D.State
  for +key: D.KeyToken
  for +action: D.ChromeAction
  {Views.context(Views.transition_state(Router.chrome(state, key, action))) == Views.context(state) : Views.FocusContext}

def chrome_preserves_context(state, key, action):
  %Equal.sym(Views.FocusContext, Views.context(Router.with_effect(Router.with_claims(state, Claims.suppress_release(Router.state_claims(state), key)), D.Chrome{action})), Views.context(Router.with_claims(state, Claims.suppress_release(Router.state_claims(state), key))), effect_preserves_context(Router.with_claims(state, Claims.suppress_release(Router.state_claims(state), key)), D.Chrome{action})) : {_ == Views.context(state) : Views.FocusContext}
  claims_preserve_context(state, Claims.suppress_release(Router.state_claims(state), key))

law forwarded_preserves_context:
  for +result: Claims.PressResult
  for +state: D.State
  for +input: D.Input
  for +pane: Core.StableId
  {Views.context(Views.transition_state(Router.forwarded(result, state, input, pane))) == Views.context(state) : Views.FocusContext}

def forwarded_preserves_context(result, state, input, pane):
  match result:
    case Claims.PressResult{claims, True{}, _}:
      claims_preserve_context(state, claims)
    case Claims.PressResult{claims, False{}, _}:
      %Equal.sym(Views.FocusContext, Views.context(Router.with_effect(Router.with_claims(state, claims), D.Forward{pane, input})), Views.context(Router.with_claims(state, claims)), effect_preserves_context(Router.with_claims(state, claims), D.Forward{pane, input})) : {_ == Views.context(state) : Views.FocusContext}
      claims_preserve_context(state, claims)

law forward_preserves_context:
  for +state: D.State
  for +input: D.Input
  for +pane: Core.StableId
  for +autorepeat: Bool
  {Views.context(Views.transition_state(Router.forward(state, input, pane, autorepeat))) == Views.context(state) : Views.FocusContext}

def forward_preserves_context(state, input, pane, autorepeat):
  forwarded_preserves_context(Claims.press(Router.state_claims(state), D.normalized_key(input), pane, autorepeat), state, input, pane)

law owner_action_preserves_context:
  for +action: Maybe<&2, D.ChromeAction>
  for +state: D.State
  for +input: D.Input
  {Views.context(Views.transition_state(Router.owner_action(action, state, input))) == Views.context(state) : Views.FocusContext}

def owner_action_preserves_context(action, state, input):
  match action:
    case None{}:
      native_preserves_context(state)
    case Some{binding}:
      chrome_preserves_context(state, D.normalized_key(input), binding)

law owner_key_preserves_context:
  for +state: D.State
  for +input: D.Input
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.owner_key(state, input, lookup))) == Views.context(state) : Views.FocusContext}

def owner_key_preserves_context(state, input, lookup):
  owner_action_preserves_context(Router.owner_lookup(D.owner(state), lookup), state, input)

law prefix_key_preserves_context:
  for +prefix: D.Prefix
  for +active: Maybe<&2, D.Active>
  for +state: D.State
  for +input: D.Input
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.prefix_key(prefix, active, state, input, lookup))) == Views.context(state) : Views.FocusContext}

def prefix_key_preserves_context(prefix, active, state, input, lookup):
  match prefix active:
    case D.Prefix{armed, True{}} Some{D.Active{pane, _}}:
      forward_preserves_context(state, input, pane, Router.armed_repeat(input, D.Prefix{armed, True{}}))
    case D.Prefix{_, True{}} None{}:
      owner_key_preserves_context(state, input, lookup)
    case D.Prefix{_, False{}} _:
      owner_key_preserves_context(state, input, lookup)

law ui_action_preserves_context:
  for +action: Maybe<&2, D.ChromeAction>
  for +state: D.State
  for +input: D.Input
  for +prefix: D.Prefix
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.ui_action(action, state, input, prefix, lookup))) == Views.context(state) : Views.FocusContext}

def ui_action_preserves_context(action, state, input, prefix, lookup):
  match action:
    case Some{binding}:
      chrome_preserves_context(state, D.normalized_key(input), binding)
    case None{}:
      prefix_key_preserves_context(prefix, Router.state_active(state), state, input, lookup)

law capture_key_preserves_context:
  for +capture: Maybe<&2, Core.StableId>
  for +state: D.State
  for +input: D.Input
  for +prefix: D.Prefix
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.capture_key(capture, state, input, prefix, lookup))) == Views.context(state) : Views.FocusContext}

def capture_key_preserves_context(capture, state, input, prefix, lookup):
  match capture:
    case Some{pane}:
      forward_preserves_context(state, input, pane, False{})
    case None{}:
      ui_action_preserves_context(Router.ui_lookup(lookup), state, input, prefix, lookup)

law overlay_key_preserves_context:
  for +previous: D.Owner
  for +state: D.State
  for +input: D.Input
  for +prefix: D.Prefix
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.overlay_key(previous, state, input, prefix, lookup))) == Views.context(state) : Views.FocusContext}

def overlay_key_preserves_context(previous, state, input, prefix, lookup):
  match previous:
    case D.Overlay{}:
      native_preserves_context(state)
    case D.NoOwner{}:
      capture_key_preserves_context(Router.state_capture(state), state, input, prefix, lookup)
    case D.Pane{_, _}:
      capture_key_preserves_context(Router.state_capture(state), state, input, prefix, lookup)
    case D.Sidebar{}:
      capture_key_preserves_context(Router.state_capture(state), state, input, prefix, lookup)
    case D.NativeEditor{}:
      capture_key_preserves_context(Router.state_capture(state), state, input, prefix, lookup)

law nonrelease_preserves_context:
  for +state: D.State
  for +input: D.Input
  for +prefix: D.Prefix
  for +lookup: D.Lookups
  {Views.context(Views.transition_state(Router.nonrelease(state, input, prefix, lookup))) == Views.context(state) : Views.FocusContext}

def nonrelease_preserves_context(state, input, prefix, lookup):
  %clear_local_preserves_context(state, D.normalized_key(input)) : {Views.context(Views.transition_state(Router.nonrelease(state, input, prefix, lookup))) == _ : Views.FocusContext}
  overlay_key_preserves_context(D.owner(Router.clear_local(state, D.normalized_key(input))), Router.clear_local(state, D.normalized_key(input)), input, prefix, lookup)

law release_held_preserves_context:
  for +result: Claims.Released
  for +state: D.State
  for +input: D.Input
  {Views.context(Views.transition_state(Router.release_held(result, state, input))) == Views.context(state) : Views.FocusContext}

def release_held_preserves_context(result, state, input):
  match result:
    case Claims.Released{claims, None{}}:
      claims_preserve_context(state, claims)
    case Claims.Released{claims, Some{pane}}:
      %Equal.sym(Views.FocusContext, Views.context(Router.with_effect(Router.with_claims(state, claims), D.Forward{pane, input})), Views.context(Router.with_claims(state, claims)), effect_preserves_context(Router.with_claims(state, claims), D.Forward{pane, input})) : {_ == Views.context(state) : Views.FocusContext}
      claims_preserve_context(state, claims)

law release_local_preserves_context:
  for +result: Claims.LocalRelease
  for +state: D.State
  for +input: D.Input
  {Views.context(Views.transition_state(Router.release_local(result, state, input))) == Views.context(state) : Views.FocusContext}

def release_local_preserves_context(result, state, input):
  match result:
    case Claims.LocalRelease{claims, True{}}:
      claims_preserve_context(state, claims)
    case Claims.LocalRelease{claims, False{}}:
      release_held_preserves_context(Claims.consume_release(claims, D.normalized_key(input)), state, input)

law release_preserves_context:
  for +state: D.State
  for +input: D.Input
  {Views.context(Views.transition_state(Router.release(state, input))) == Views.context(state) : Views.FocusContext}

def release_preserves_context(state, input):
  release_local_preserves_context(Claims.consume_local_release(Router.state_claims(state), D.normalized_key(input)), state, input)

law key_action_preserves_context:
  for +state: D.State
  for +input: D.Input
  for +prefix: D.Prefix
  for +lookup: D.Lookups
  for +action: D.KeyAction
  {Views.context(Views.transition_state(Router.key_action(state, input, prefix, lookup, action))) == Views.context(state) : Views.FocusContext}

def key_action_preserves_context(state, input, prefix, lookup, action):
  match action:
    case D.Release{}:
      release_preserves_context(state, input)
    case D.KeyPressed{}:
      nonrelease_preserves_context(state, input, prefix, lookup)
    case D.Repeat{}:
      nonrelease_preserves_context(state, input, prefix, lookup)
