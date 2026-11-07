import Base
import ./core.bend as Core
import ./tree_list.bend as Lists
import ./input_router_data.bend as D
import ./input_router_claims.bend as Claims

def state_claims(state: D.State) -> D.Claims:
  match state:
    case D.State{_, _, _, _, _, _, _, claims, _}:
      claims

def state_effects(state: D.State) -> List<&2, D.Effect>:
  match state:
    case D.State{_, _, _, _, _, _, _, _, effects}:
      effects

def state_capture(state: D.State) -> Maybe<&2, Core.StableId>:
  match state:
    case D.State{_, _, _, _, _, _, capture, _, _}:
      capture

def state_active(state: D.State) -> Maybe<&2, D.Active>:
  match state:
    case D.State{_, _, _, _, _, active, _, _, _}:
      active

def state_restore(state: D.State) -> D.Owner:
  match state:
    case D.State{_, _, _, _, restore, _, _, _, _}:
      restore

def with_claims(state: D.State, claims: D.Claims) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, restore, active, capture, _, effects}:
      D.State{map, owner, pending, observed, restore, active, capture, claims, effects}

def with_effect(state: D.State, effect: D.Effect) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, restore, active, capture, claims, effects}:
      D.State{map, owner, pending, observed, restore, active, capture, claims, Lists.append(D.Effect, effects, effect <> Nil{})}

def with_restore(state: D.State, restore: D.Owner) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, _, active, capture, claims, effects}:
      D.State{map, owner, pending, observed, restore, active, capture, claims, effects}

def set_capture(state: D.State, capture: Maybe<&2, Core.StableId>) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, restore, active, _, claims, effects}:
      D.State{map, owner, pending, observed, restore, active, capture, claims, effects}

def set_active(state: D.State, pane: Core.StableId, surface: D.Surface) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, restore, _, capture, claims, effects}:
      D.State{map, owner, pending, observed, restore, Some{D.Active{pane, surface}}, capture, claims, effects}

def is_overlay(owner: D.Owner) -> Bool:
  match owner:
    case D.Overlay{}:
      True{}
    case _:
      False{}

def focused(state: D.State, +target: D.Owner, restore: D.Owner) -> D.State:
  match state:
    case D.State{map, _, _, observed, _, active, capture, claims, effects}:
      D.State{map, target, None{}, observed, restore, active, capture, claims, Lists.append(D.Effect, effects, D.RequestFocus{target} <> Nil{})}

def focus_previous(+state: D.State, +target: D.Owner, previous: D.Owner) -> D.State:
  match previous:
    case D.Overlay{}:
      with_restore(state, target)
    case D.Pane{pane, surface}:
      focused(state, target, D.Pane{pane, surface})
    case _:
      focused(state, target, state_restore(state))

def focus(+state: D.State, target: D.Owner) -> D.State:
  focus_previous(state, target, D.owner(state))

def activated(state: D.State, +pane: Core.StableId, +surface: D.Surface, overlay: Bool) -> D.State:
  match state overlay:
    case D.State{map, owner, pending, observed, _, _, capture, claims, effects} True{}:
      D.State{map, owner, pending, observed, D.Pane{pane, surface}, Some{D.Active{pane, surface}}, capture, claims, effects}
    case D.State{map, owner, _, observed, restore, _, capture, claims, effects} False{}:
      D.State{map, owner, Some{D.Pane{pane, surface}}, observed, restore, Some{D.Active{pane, surface}}, capture, claims, Lists.append(D.Effect, effects, D.RequestFocus{D.Pane{pane, surface}} <> Nil{})}

def activate(+state: D.State, pane: Core.StableId, surface: D.Surface) -> D.State:
  activated(state, pane, surface, is_overlay(D.owner(state)))

def opened(state: D.State, previous: D.Owner, overlay: Bool) -> D.State:
  match state overlay:
    case original True{}:
      original
    case D.State{map, _, _, observed, _, active, capture, claims, effects} False{}:
      D.State{map, D.Overlay{}, None{}, observed, previous, active, capture, claims, effects}

def overlay_opened(+state: D.State) -> D.State:
  +previous = D.owner(state)
  opened(state, previous, is_overlay(previous))

def closed(state: D.State, overlay: Bool) -> D.State:
  match state overlay:
    case original False{}:
      original
    case D.State{map, _, _, observed, +restore, active, capture, claims, effects} True{}:
      D.State{map, restore, None{}, observed, restore, active, capture, claims, Lists.append(D.Effect, effects, D.RequestFocus{restore} <> Nil{})}

def overlay_closed(+state: D.State) -> D.State:
  closed(state, is_overlay(D.owner(state)))

def observed_matches(pending: Maybe<&2, D.Owner>, observed: D.Owner) -> Bool:
  match pending:
    case None{}:
      True{}
    case Some{requested}:
      D.owner_equal(requested, observed)

def observed(state: D.State, +seen: D.Owner, matches: Bool) -> D.State:
  match state matches:
    case D.State{map, _, _, _, restore, active, capture, claims, effects} True{}:
      D.State{map, seen, None{}, seen, restore, active, capture, claims, effects}
    case D.State{map, owner, pending, _, restore, active, capture, claims, effects} False{}:
      D.State{map, owner, pending, seen, restore, active, capture, claims, effects}

def native_focus_observed(state: D.State, +seen: D.Owner) -> D.State:
  match state:
    case D.State{map, owner, +pending, old_seen, restore, active, capture, claims, effects}:
      observed(D.State{map, owner, pending, old_seen, restore, active, capture, claims, effects}, seen, observed_matches(pending, seen))

def owner_removed(owner: D.Owner, remove: Bool) -> D.Owner:
  match remove:
    case True{}:
      D.NoOwner{}
    case False{}:
      owner

def remove_owner(owner: D.Owner, pane: Core.StableId) -> D.Owner:
  match owner:
    case D.Pane{+id, +surface}:
      owner_removed(D.Pane{id, surface}, Core.id_equal(id, pane))
    case rest:
      rest

def pending_is_pane(pending: Maybe<&2, D.Owner>, pane: Core.StableId) -> Bool:
  match pending:
    case Some{D.Pane{id, _}}:
      Core.id_equal(id, pane)
    case _:
      False{}

def remove_active_choice(active: Maybe<&2, D.Active>, remove: Bool) -> Maybe<&2, D.Active>:
  match remove:
    case True{}:
      None{}
    case False{}:
      active

def remove_active(active: Maybe<&2, D.Active>, pane: Core.StableId) -> Maybe<&2, D.Active>:
  match active:
    case None{}:
      None{}
    case Some{D.Active{+id, +surface}}:
      remove_active_choice(Some{D.Active{id, surface}}, Core.id_equal(id, pane))

def remove_pending_choice(state: D.State, clear: Bool) -> D.State:
  match state clear:
    case D.State{map, _, _, observed, restore, active, capture, claims, effects} True{}:
      D.State{map, D.NoOwner{}, None{}, observed, restore, active, capture, claims, effects}
    case original False{}:
      original

def pane_removed(state: D.State, +pane: Core.StableId) -> D.State:
  match state:
    case D.State{map, owner, +pending, seen, restore, active, capture, claims, effects}:
      next_owner next_restore = remove_owner(owner, pane) remove_owner(restore, pane)
      next_seen next_active = remove_owner(seen, pane) remove_active(active, pane)
      remove_pending_choice(D.State{map, next_owner, pending, next_seen, next_restore, next_active, capture, Claims.remove_pane(claims, pane), effects}, pending_is_pane(pending, pane))

def detached(state: D.State) -> D.State:
  match state:
    case D.State{map, _, _, _, _, _, capture, _, effects}:
      D.State{map, D.NoOwner{}, None{}, D.NoOwner{}, D.NoOwner{}, None{}, capture, D.Claims{Nil{}, Nil{}}, effects}

def window_deactivated(state: D.State) -> D.State:
  with_claims(state, D.Claims{Nil{}, Nil{}})

def event(state: D.State, event: D.RouterEvent) -> D.State:
  match event:
    case D.ActivatePane{pane, surface}:
      activate(state, pane, surface)
    case D.SetActivePane{pane, surface}:
      set_active(state, pane, surface)
    case D.FocusSidebar{}:
      focus(state, D.Sidebar{})
    case D.FocusNativeEditor{}:
      focus(state, D.NativeEditor{})
    case D.OverlayOpened{}:
      overlay_opened(state)
    case D.OverlayClosed{}:
      overlay_closed(state)
    case D.NativeFocusObserved{owner}:
      native_focus_observed(state, owner)
    case D.PaneRemoved{pane}:
      pane_removed(state, pane)
    case D.Detached{}:
      detached(state)
    case D.WindowDeactivated{}:
      window_deactivated(state)

def suppress_release(+state: D.State, key: D.KeyToken) -> D.State:
  with_claims(state, Claims.suppress_release(state_claims(state), key))

def clear_local(state: D.State, key: D.KeyToken) -> D.State:
  match state:
    case D.State{map, owner, pending, observed, restore, active, capture, D.Claims{held, local}, effects}:
      D.State{map, owner, pending, observed, restore, active, capture, D.Claims{held, Claims.remove_local(local, key)}, effects}

def native(state: D.State) -> D.Transition:
  D.Transition{state, D.Native{}}

def chrome(+state: D.State, key: D.KeyToken, action: D.ChromeAction) -> D.Transition:
  D.Transition{with_effect(with_claims(state, Claims.suppress_release(state_claims(state), key)), D.Chrome{action}), D.Consumed{}}

def forwarded(result: Claims.PressResult, state: D.State, input: D.Input, pane: Core.StableId) -> D.Transition:
  match result:
    case Claims.PressResult{claims, True{}, _}:
      D.Transition{with_claims(state, claims), D.Consumed{}}
    case Claims.PressResult{claims, False{}, _}:
      D.Transition{with_effect(with_claims(state, claims), D.Forward{pane, input}), D.Consumed{}}

def forward(+state: D.State, +input: D.Input, +pane: Core.StableId, autorepeat: Bool) -> D.Transition:
  forwarded(Claims.press(state_claims(state), D.normalized_key(input), pane, autorepeat), state, input, pane)

def owner_lookup(owner: D.Owner, lookup: D.Lookups) -> Maybe<&2, D.ChromeAction>:
  match owner lookup:
    case D.Sidebar{} D.Lookups{_, action, _, _}:
      action
    case D.Pane{_, D.Terminal{}} D.Lookups{_, _, action, _}:
      action
    case D.Pane{_, D.Browser{}} D.Lookups{_, _, _, action}:
      action
    case _ _:
      None{}

def owner_action(action: Maybe<&2, D.ChromeAction>, state: D.State, input: D.Input) -> D.Transition:
  match action:
    case None{}:
      native(state)
    case Some{binding}:
      chrome(state, D.normalized_key(input), binding)

def owner_key(+state: D.State, input: D.Input, lookup: D.Lookups) -> D.Transition:
  owner_action(owner_lookup(D.owner(state), lookup), state, input)

def armed_repeat(input: D.Input, prefix: D.Prefix) -> Bool:
  match input prefix:
    case D.Input{_, _, D.Repeat{}, _} D.Prefix{armed, _}:
      armed
    case _ _:
      False{}

def prefix_key(prefix: D.Prefix, active: Maybe<&2, D.Active>, state: D.State, +input: D.Input, lookup: D.Lookups) -> D.Transition:
  match prefix active:
    case D.Prefix{+armed, True{}} Some{D.Active{pane, _}}:
      forward(state, input, pane, armed_repeat(input, D.Prefix{armed, True{}}))
    case _ _:
      owner_key(state, input, lookup)

def ui_action(action: Maybe<&2, D.ChromeAction>, +state: D.State, input: D.Input, prefix: D.Prefix, lookup: D.Lookups) -> D.Transition:
  match action:
    case Some{binding}:
      chrome(state, D.normalized_key(input), binding)
    case None{}:
      prefix_key(prefix, state_active(state), state, input, lookup)

def ui_lookup(lookup: D.Lookups) -> Maybe<&2, D.ChromeAction>:
  match lookup:
    case D.Lookups{ui, _, _, _}:
      ui

def capture_key(capture: Maybe<&2, Core.StableId>, state: D.State, input: D.Input, prefix: D.Prefix, +lookup: D.Lookups) -> D.Transition:
  match capture:
    case Some{pane}:
      forward(state, input, pane, False{})
    case None{}:
      ui_action(ui_lookup(lookup), state, input, prefix, lookup)

def overlay_key(previous: D.Owner, +state: D.State, input: D.Input, prefix: D.Prefix, lookup: D.Lookups) -> D.Transition:
  match previous:
    case D.Overlay{}:
      native(state)
    case _:
      capture_key(state_capture(state), state, input, prefix, lookup)

def nonrelease(state: D.State, +input: D.Input, prefix: D.Prefix, lookup: D.Lookups) -> D.Transition:
  +cleaned = clear_local(state, D.normalized_key(input))
  overlay_key(D.owner(cleaned), cleaned, input, prefix, lookup)

def release_held(result: Claims.Released, state: D.State, input: D.Input) -> D.Transition:
  match result:
    case Claims.Released{claims, None{}}:
      native(with_claims(state, claims))
    case Claims.Released{claims, Some{pane}}:
      D.Transition{with_effect(with_claims(state, claims), D.Forward{pane, input}), D.Consumed{}}

def release_local(result: Claims.LocalRelease, state: D.State, +input: D.Input) -> D.Transition:
  match result:
    case Claims.LocalRelease{claims, True{}}:
      D.Transition{with_claims(state, claims), D.Consumed{}}
    case Claims.LocalRelease{claims, False{}}:
      release_held(Claims.consume_release(claims, D.normalized_key(input)), state, input)

def release(+state: D.State, +input: D.Input) -> D.Transition:
  release_local(Claims.consume_local_release(state_claims(state), D.normalized_key(input)), state, input)

def action(input: D.Input) -> D.KeyAction:
  match input:
    case D.Input{_, _, action, _}:
      action

def key_action(state: D.State, input: D.Input, prefix: D.Prefix, lookup: D.Lookups, action: D.KeyAction) -> D.Transition:
  match action:
    case D.Release{}:
      release(state, input)
    case _:
      nonrelease(state, input, prefix, lookup)

def key(state: D.State, +input: D.Input, prefix: D.Prefix, lookup: D.Lookups) -> D.Transition:
  key_action(state, input, prefix, lookup, action(input))

def drain_effects(state: D.State) -> D.Drained:
  match state:
    case D.State{map, owner, pending, observed, restore, active, capture, claims, effects}:
      D.Drained{D.State{map, owner, pending, observed, restore, active, capture, claims, Nil{}}, effects}

def local_empty(local: List<&2, D.KeyToken>) -> Bool:
  match local:
    case Nil{}:
      True{}
    case _ <> _:
      False{}

def effects_empty(effects: List<&2, D.Effect>) -> Bool:
  match effects:
    case Nil{}:
      True{}
    case _ <> _:
      False{}

def capture_empty(capture: Maybe<&2, Core.StableId>) -> Bool:
  match capture:
    case None{}:
      True{}
    case Some{_}:
      False{}

def plain_gate(state: D.State, releases: Bool, ui_binds: Bool, table_binds: Bool) -> Bool:
  match state:
    case D.State{_, _, _, _, _, _, capture, D.Claims{_, local}, effects}:
      capture_empty(capture) && effects_empty(effects) && (Bool.not(releases) || local_empty(local)) && Bool.not(ui_binds) && Bool.not(table_binds)

def plain_owner(owner: D.Owner, state: D.State, releases: Bool, ui_binds: Bool, terminal_binds: Bool) -> Bool:
  match owner:
    case D.NoOwner{}:
      plain_gate(state, releases, ui_binds, False{})
    case D.Pane{_, D.Terminal{}}:
      plain_gate(state, releases, ui_binds, terminal_binds)
    case _:
      False{}

def passes_plain_keys(+state: D.State, releases: Bool, ui_binds: Bool, terminal_binds: Bool) -> Bool:
  plain_owner(D.owner(state), state, releases, ui_binds, terminal_binds)
