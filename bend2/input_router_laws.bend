import Base
import ./core.bend as Core
import ./tree_list.bend as Lists
import ./input_router_data.bend as D
import ./input_router_claims.bend as Claims
import ./input_router.bend as Router
import ./input_router_views.bend as Views

law every_event_preserves_capture:
  for +state: D.State
  for event: D.RouterEvent
  {Router.state_capture(Router.event(state, event)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

law every_key_preserves_complete_focus_context:
  for +state: D.State
  for +input: D.Input
  for prefix: D.Prefix
  for lookup: D.Lookups
  {Views.context(Views.transition_state(Router.key(state, input, prefix, lookup))) == Views.context(state) : Views.FocusContext}

law swap_remove_preserves_arbitrary_prefix_and_middle:
  for -A: Data
  for +prefix: List<&2, A>
  for +removed: A
  for +middle: List<&2, A>
  for +last: A
  {Claims.swap_remove(A, Lists.concat(A, prefix, removed <> Lists.concat(A, middle, last <> Nil{})), Views.count(A, prefix)) == Lists.concat(A, prefix, last <> middle) : List<&2, A>}

law swap_remove_last_preserves_arbitrary_prefix:
  for -A: Data
  for +prefix: List<&2, A>
  for +last: A
  {Claims.swap_remove(A, Lists.concat(A, prefix, last <> Nil{}), Views.count(A, prefix)) == prefix : List<&2, A>}

law key_normalization_prefers_unshifted_identity:
  for raw: D.KeyToken
  for +unshifted: D.KeyToken
  for action: D.KeyAction
  for payload: D.Payload
  {D.normalized_key(D.Input{raw, Some{unshifted}, action, payload}) == unshifted : D.KeyToken}

law detach_clears_focus_and_claims_but_preserves_capture_and_effects:
  for +map: Core.StableId
  for owner: D.Owner
  for pending: Maybe<&2, D.Owner>
  for observed: D.Owner
  for restore: D.Owner
  for active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.event(D.State{map, owner, pending, observed, restore, active, capture, claims, effects}, D.Detached{}) == D.State{map, D.NoOwner{}, None{}, D.NoOwner{}, D.NoOwner{}, None{}, capture, D.Claims{Nil{}, Nil{}}, effects} : D.State}

law window_deactivation_only_clears_claims:
  for +map: Core.StableId
  for +owner: D.Owner
  for +pending: Maybe<&2, D.Owner>
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.event(D.State{map, owner, pending, observed, restore, active, capture, claims, effects}, D.WindowDeactivated{}) == D.State{map, owner, pending, observed, restore, active, capture, D.Claims{Nil{}, Nil{}}, effects} : D.State}

law active_selection_does_not_request_focus:
  for +map: Core.StableId
  for +owner: D.Owner
  for +pending: Maybe<&2, D.Owner>
  for +observed: D.Owner
  for +restore: D.Owner
  for active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  for +pane: Core.StableId
  for +surface: D.Surface
  {Router.event(D.State{map, owner, pending, observed, restore, active, capture, claims, effects}, D.SetActivePane{pane, surface}) == D.State{map, owner, pending, observed, restore, Some{D.Active{pane, surface}}, capture, claims, effects} : D.State}

law overlay_activation_updates_restore_without_focus_effect:
  for +map: Core.StableId
  for +observed: D.Owner
  for restore: D.Owner
  for active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  for +pane: Core.StableId
  for +surface: D.Surface
  {Router.event(D.State{map, D.Overlay{}, None{}, observed, restore, active, capture, claims, effects}, D.ActivatePane{pane, surface}) == D.State{map, D.Overlay{}, None{}, observed, D.Pane{pane, surface}, Some{D.Active{pane, surface}}, capture, claims, effects} : D.State}

law observed_sidebar_cannot_replace_a_pending_pane:
  for +map: Core.StableId
  for +owner: D.Owner
  for +pane: Core.StableId
  for +surface: D.Surface
  for observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.event(D.State{map, owner, Some{D.Pane{pane, surface}}, observed, restore, active, capture, claims, effects}, D.NativeFocusObserved{D.Sidebar{}}) == D.State{map, owner, Some{D.Pane{pane, surface}}, D.Sidebar{}, restore, active, capture, claims, effects} : D.State}

law acknowledged_pending_pane_commits_exact_focus:
  for +map: Core.StableId
  for owner: D.Owner
  for +pane: Core.StableId
  for +surface: D.Surface
  for observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.event(D.State{map, owner, Some{D.Pane{pane, surface}}, observed, restore, active, capture, claims, effects}, D.NativeFocusObserved{D.Pane{pane, surface}}) == D.State{map, D.Pane{pane, surface}, None{}, D.Pane{pane, surface}, restore, active, capture, claims, effects} : D.State}

law overlay_round_trip_restores_pending_pane:
  for +map: Core.StableId
  for owner: D.Owner
  for +pane: Core.StableId
  for +surface: D.Surface
  for +observed: D.Owner
  for restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.event(Router.event(D.State{map, owner, Some{D.Pane{pane, surface}}, observed, restore, active, capture, claims, effects}, D.OverlayOpened{}), D.OverlayClosed{}) == D.State{map, D.Pane{pane, surface}, None{}, observed, D.Pane{pane, surface}, active, capture, claims, Lists.append(D.Effect, effects, D.RequestFocus{D.Pane{pane, surface}} <> Nil{})} : D.State}

law drain_preserves_state_except_exact_effect_queue:
  for +map: Core.StableId
  for +owner: D.Owner
  for +pending: Maybe<&2, D.Owner>
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +claims: D.Claims
  for +effects: List<&2, D.Effect>
  {Router.drain_effects(D.State{map, owner, pending, observed, restore, active, capture, claims, effects}) == D.Drained{D.State{map, owner, pending, observed, restore, active, capture, claims, Nil{}}, effects} : D.Drained}

law overlay_press_clears_local_claim_without_forwarding:
  for +map: Core.StableId
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +held: List<&2, D.Held>
  for +local: List<&2, D.KeyToken>
  for +effects: List<&2, D.Effect>
  for +key: D.KeyToken
  for +unshifted: Maybe<&2, D.KeyToken>
  for +payload: D.Payload
  for prefix: D.Prefix
  for lookup: D.Lookups
  {Router.key(D.State{map, D.Overlay{}, None{}, observed, restore, active, capture, D.Claims{held, local}, effects}, D.Input{key, unshifted, D.KeyPressed{}, payload}, prefix, lookup) == D.Transition{D.State{map, D.Overlay{}, None{}, observed, restore, active, capture, D.Claims{held, Claims.remove_local(local, D.normalized_key(D.Input{key, unshifted, D.KeyPressed{}, payload}))}, effects}, D.Native{}} : D.Transition}

law armed_claimed_repeat_preserves_held_and_effects:
  for +map: Core.StableId
  for +owner_pane: Core.StableId
  for +observed: D.Owner
  for +restore: D.Owner
  for +active_pane: Core.StableId
  for +active_surface: D.Surface
  for +held: List<&2, D.Held>
  for +local: List<&2, D.KeyToken>
  for +effects: List<&2, D.Effect>
  for +key: D.KeyToken
  for +unshifted: Maybe<&2, D.KeyToken>
  for +payload: D.Payload
  for sidebar: Maybe<&2, D.ChromeAction>
  for terminal: Maybe<&2, D.ChromeAction>
  for browser: Maybe<&2, D.ChromeAction>
  {Router.key(D.State{map, D.Pane{owner_pane, D.Terminal{}}, None{}, observed, restore, Some{D.Active{active_pane, active_surface}}, None{}, D.Claims{held, local}, effects}, D.Input{key, unshifted, D.Repeat{}, payload}, D.Prefix{True{}, True{}}, D.Lookups{None{}, sidebar, terminal, browser}) == D.Transition{D.State{map, D.Pane{owner_pane, D.Terminal{}}, None{}, observed, restore, Some{D.Active{active_pane, active_surface}}, None{}, D.Claims{held, Claims.remove_local(local, D.normalized_key(D.Input{key, unshifted, D.Repeat{}, payload}))}, effects}, D.Consumed{}} : D.Transition}

law captured_press_precedes_ui_and_prefix:
  for +map: Core.StableId
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +pane: Core.StableId
  for +effects: List<&2, D.Effect>
  for +key: D.KeyToken
  for +unshifted: Maybe<&2, D.KeyToken>
  for +payload: D.Payload
  for prefix: D.Prefix
  for lookup: D.Lookups
  {Router.key(D.State{map, D.Sidebar{}, None{}, observed, restore, active, Some{pane}, D.Claims{Nil{}, Nil{}}, effects}, D.Input{key, unshifted, D.KeyPressed{}, payload}, prefix, lookup) == D.Transition{D.State{map, D.Sidebar{}, None{}, observed, restore, active, Some{pane}, D.Claims{D.Held{D.normalized_key(D.Input{key, unshifted, D.KeyPressed{}, payload}), pane} <> Nil{}, Nil{}}, Lists.append(D.Effect, effects, D.Forward{pane, D.Input{key, unshifted, D.KeyPressed{}, payload}} <> Nil{})}, D.Consumed{}} : D.Transition}

law ui_binding_precedes_claimed_active_pane:
  for +map: Core.StableId
  for +pane: Core.StableId
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: D.Active
  for +effects: List<&2, D.Effect>
  for +key: D.KeyToken
  for +unshifted: Maybe<&2, D.KeyToken>
  for +payload: D.Payload
  for +binding: D.ChromeAction
  for sidebar: Maybe<&2, D.ChromeAction>
  for terminal: Maybe<&2, D.ChromeAction>
  for browser: Maybe<&2, D.ChromeAction>
  {Router.key(D.State{map, D.Pane{pane, D.Terminal{}}, None{}, observed, restore, Some{active}, None{}, D.Claims{Nil{}, Nil{}}, effects}, D.Input{key, unshifted, D.KeyPressed{}, payload}, D.Prefix{True{}, True{}}, D.Lookups{Some{binding}, sidebar, terminal, browser}) == D.Transition{D.State{map, D.Pane{pane, D.Terminal{}}, None{}, observed, restore, Some{active}, None{}, D.Claims{Nil{}, D.normalized_key(D.Input{key, unshifted, D.KeyPressed{}, payload}) <> Nil{}}, Lists.append(D.Effect, effects, D.Chrome{binding} <> Nil{})}, D.Consumed{}} : D.Transition}

law claimed_release_precedes_overlay_capture_and_keymap:
  for +map: Core.StableId
  for +owner: D.Owner
  for +pending: Maybe<&2, D.Owner>
  for +observed: D.Owner
  for +restore: D.Owner
  for +active: Maybe<&2, D.Active>
  for +capture: Maybe<&2, Core.StableId>
  for +tail: List<&2, D.Held>
  for +effects: List<&2, D.Effect>
  for +pane: Core.StableId
  for +key: D.KeyToken
  for +unshifted: Maybe<&2, D.KeyToken>
  for +payload: D.Payload
  for prefix: D.Prefix
  for lookup: D.Lookups
  {Router.key(D.State{map, owner, pending, observed, restore, active, capture, D.Claims{D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, Nil{}}, effects}, D.Input{key, unshifted, D.Release{}, payload}, prefix, lookup) == D.Transition{D.State{map, owner, pending, observed, restore, active, capture, D.Claims{Claims.swap_remove(D.Held, D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, 0n), Nil{}}, Lists.append(D.Effect, effects, D.Forward{pane, D.Input{key, unshifted, D.Release{}, payload}} <> Nil{})}, D.Consumed{}} : D.Transition}
