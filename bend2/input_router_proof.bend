import Base
import ./core.bend as Core
import ./tree_list.bend as Lists
import ./id_proof.bend as Id
import ./input_router_data.bend as D
import ./input_router_claims.bend as Claims
import ./input_router.bend as Router
import ./input_router_views.bend as Views
import ./input_router_laws.bend as Laws
import ./input_router_context_proof.bend as Context
import ./input_router_event_proof.bend as Events
import ./input_router_vector_proof.bend as Vector

law surface_reflexive:
  for +surface: D.Surface
  {D.surface_equal(surface, surface) == True{} : Bool}

def surface_reflexive(surface):
  match surface:
    case D.Terminal{}:
      {==}
    case D.Browser{}:
      {==}
    case D.Other{}:
      {==}

law owner_reflexive:
  for +owner: D.Owner
  {D.owner_equal(owner, owner) == True{} : Bool}

def owner_reflexive(owner):
  match owner:
    case D.NoOwner{}:
      {==}
    case D.Pane{pane, surface}:
      %Equal.sym(Bool, Core.id_equal(pane, pane), True{}, Id.id_refl(pane)) : {Bool.and(_, D.surface_equal(surface, surface)) == True{} : Bool}
      surface_reflexive(surface)
    case D.Sidebar{}:
      {==}
    case D.Overlay{}:
      {==}
    case D.NativeEditor{}:
      {==}

law key_reflexive:
  for +key: D.KeyToken
  {D.key_equal(key, key) == True{} : Bool}

def key_reflexive(key):
  match key:
    case D.KeyToken{id}:
      Id.id_refl(id)

law release_head:
  for +key: D.KeyToken
  for +pane: Core.StableId
  for +tail: List<&2, D.Held>
  for +local: List<&2, D.KeyToken>
  {Claims.consume_release(D.Claims{D.Held{key, pane} <> tail, local}, key) == Claims.Released{D.Claims{Claims.swap_remove(D.Held, D.Held{key, pane} <> tail, 0n), local}, Some{pane}} : Claims.Released}

def release_head(key, pane, tail, local):
  %Equal.sym(Bool, D.key_equal(key, key), True{}, key_reflexive(key)) : {Claims.released(Claims.found_choice(_, 0n, pane, Claims.find_held(tail, key, 1n)), D.Held{key, pane} <> tail, local) == Claims.Released{D.Claims{Claims.swap_remove(D.Held, D.Held{key, pane} <> tail, 0n), local}, Some{pane}} : Claims.Released}
  {==}

def Laws.every_event_preserves_capture(state, event):
  Events.every_event_preserves_capture(state, event)

def Laws.every_key_preserves_complete_focus_context(state, input, prefix, lookup):
  Context.key_action_preserves_context(state, input, prefix, lookup, Router.action(input))

def Laws.swap_remove_preserves_arbitrary_prefix_and_middle(A, prefix, removed, middle, last):
  Vector.exact_swap_remove(A, prefix, removed, middle, last)

def Laws.swap_remove_last_preserves_arbitrary_prefix(A, prefix, last):
  Vector.exact_swap_remove_last(A, prefix, last)

def Laws.key_normalization_prefers_unshifted_identity(raw, unshifted, action, payload):
  {==}

def Laws.detach_clears_focus_and_claims_but_preserves_capture_and_effects(map, owner, pending, observed, restore, active, capture, claims, effects):
  {==}

def Laws.window_deactivation_only_clears_claims(map, owner, pending, observed, restore, active, capture, claims, effects):
  {==}

def Laws.active_selection_does_not_request_focus(map, owner, pending, observed, restore, active, capture, claims, effects, pane, surface):
  {==}

def Laws.overlay_activation_updates_restore_without_focus_effect(map, observed, restore, active, capture, claims, effects, pane, surface):
  {==}

def Laws.observed_sidebar_cannot_replace_a_pending_pane(map, owner, pane, surface, observed, restore, active, capture, claims, effects):
  {==}

def Laws.acknowledged_pending_pane_commits_exact_focus(map, owner, pane, surface, observed, restore, active, capture, claims, effects):
  %Equal.sym(Bool, D.owner_equal(D.Pane{pane, surface}, D.Pane{pane, surface}), True{}, owner_reflexive(D.Pane{pane, surface})) : {Router.observed(D.State{map, owner, Some{D.Pane{pane, surface}}, observed, restore, active, capture, claims, effects}, D.Pane{pane, surface}, _) == D.State{map, D.Pane{pane, surface}, None{}, D.Pane{pane, surface}, restore, active, capture, claims, effects} : D.State}
  {==}

def Laws.overlay_round_trip_restores_pending_pane(map, owner, pane, surface, observed, restore, active, capture, claims, effects):
  {==}

def Laws.drain_preserves_state_except_exact_effect_queue(map, owner, pending, observed, restore, active, capture, claims, effects):
  {==}

def Laws.overlay_press_clears_local_claim_without_forwarding(map, observed, restore, active, capture, held, local, effects, key, unshifted, payload, prefix, lookup):
  {==}

def Laws.armed_claimed_repeat_preserves_held_and_effects(map, owner_pane, observed, restore, active_pane, active_surface, held, local, effects, key, unshifted, payload, sidebar, terminal, browser):
  {==}

def Laws.captured_press_precedes_ui_and_prefix(map, observed, restore, active, pane, effects, key, unshifted, payload, prefix, lookup):
  {==}

def Laws.ui_binding_precedes_claimed_active_pane(map, pane, observed, restore, active, effects, key, unshifted, payload, binding, sidebar, terminal, browser):
  {==}

def Laws.claimed_release_precedes_overlay_capture_and_keymap(map, owner, pending, observed, restore, active, capture, tail, effects, pane, key, unshifted, payload, prefix, lookup):
  %Equal.sym(Claims.Released, Claims.consume_release(D.Claims{D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, Nil{}}, D.normalized_key(D.Input{key, unshifted, D.Release{}, payload})), Claims.Released{D.Claims{Claims.swap_remove(D.Held, D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, 0n), Nil{}}, Some{pane}}, release_head(D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane, tail, Nil{})) : {Router.release_held(_, D.State{map, owner, pending, observed, restore, active, capture, D.Claims{D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, Nil{}}, effects}, D.Input{key, unshifted, D.Release{}, payload}) == D.Transition{D.State{map, owner, pending, observed, restore, active, capture, D.Claims{Claims.swap_remove(D.Held, D.Held{D.normalized_key(D.Input{key, unshifted, D.Release{}, payload}), pane} <> tail, 0n), Nil{}}, Lists.append(D.Effect, effects, D.Forward{pane, D.Input{key, unshifted, D.Release{}, payload}} <> Nil{})}, D.Consumed{}} : D.Transition}
  {==}
