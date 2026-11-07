import Base
import ./core.bend as Core
import ./input_router_data.bend as D
import ./input_router_claims.bend as Claims
import ./input_router.bend as Router

law claims_preserve_capture:
  for +state: D.State
  for +claims: D.Claims
  {Router.state_capture(Router.with_claims(state, claims)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def claims_preserve_capture(state, claims):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law restore_preserves_capture:
  for +state: D.State
  for +restore: D.Owner
  {Router.state_capture(Router.with_restore(state, restore)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def restore_preserves_capture(state, restore):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law active_preserves_capture:
  for +state: D.State
  for +pane: Core.StableId
  for +surface: D.Surface
  {Router.state_capture(Router.set_active(state, pane, surface)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def active_preserves_capture(state, pane, surface):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law focused_preserves_capture:
  for +state: D.State
  for +target: D.Owner
  for +restore: D.Owner
  {Router.state_capture(Router.focused(state, target, restore)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def focused_preserves_capture(state, target, restore):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law focus_previous_preserves_capture:
  for +state: D.State
  for +target: D.Owner
  for +previous: D.Owner
  {Router.state_capture(Router.focus_previous(state, target, previous)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def focus_previous_preserves_capture(state, target, previous):
  match previous:
    case D.Overlay{}:
      restore_preserves_capture(state, target)
    case D.Pane{pane, surface}:
      focused_preserves_capture(state, target, D.Pane{pane, surface})
    case D.NoOwner{}:
      focused_preserves_capture(state, target, Router.state_restore(state))
    case D.Sidebar{}:
      focused_preserves_capture(state, target, Router.state_restore(state))
    case D.NativeEditor{}:
      focused_preserves_capture(state, target, Router.state_restore(state))

law focus_preserves_capture:
  for +state: D.State
  for +target: D.Owner
  {Router.state_capture(Router.focus(state, target)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def focus_preserves_capture(state, target):
  focus_previous_preserves_capture(state, target, D.owner(state))

law activated_preserves_capture:
  for +state: D.State
  for +pane: Core.StableId
  for +surface: D.Surface
  for +overlay: Bool
  {Router.state_capture(Router.activated(state, pane, surface, overlay)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def activated_preserves_capture(state, pane, surface, overlay):
  match state overlay:
    case D.State{_, _, _, _, _, _, _, _, _} True{}:
      {==}
    case D.State{_, _, _, _, _, _, _, _, _} False{}:
      {==}

law activate_preserves_capture:
  for +state: D.State
  for +pane: Core.StableId
  for +surface: D.Surface
  {Router.state_capture(Router.activate(state, pane, surface)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def activate_preserves_capture(state, pane, surface):
  activated_preserves_capture(state, pane, surface, Router.is_overlay(D.owner(state)))

law opened_preserves_capture:
  for +state: D.State
  for +previous: D.Owner
  for +overlay: Bool
  {Router.state_capture(Router.opened(state, previous, overlay)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def opened_preserves_capture(state, previous, overlay):
  match state overlay:
    case _ True{}:
      {==}
    case D.State{_, _, _, _, _, _, _, _, _} False{}:
      {==}

law overlay_opened_preserves_capture:
  for +state: D.State
  {Router.state_capture(Router.overlay_opened(state)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def overlay_opened_preserves_capture(state):
  opened_preserves_capture(state, D.owner(state), Router.is_overlay(D.owner(state)))

law closed_preserves_capture:
  for +state: D.State
  for +overlay: Bool
  {Router.state_capture(Router.closed(state, overlay)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def closed_preserves_capture(state, overlay):
  match state overlay:
    case _ False{}:
      {==}
    case D.State{_, _, _, _, _, _, _, _, _} True{}:
      {==}

law overlay_closed_preserves_capture:
  for +state: D.State
  {Router.state_capture(Router.overlay_closed(state)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def overlay_closed_preserves_capture(state):
  closed_preserves_capture(state, Router.is_overlay(D.owner(state)))

law observed_preserves_capture:
  for +state: D.State
  for +seen: D.Owner
  for +matches: Bool
  {Router.state_capture(Router.observed(state, seen, matches)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def observed_preserves_capture(state, seen, matches):
  match state matches:
    case D.State{_, _, _, _, _, _, _, _, _} True{}:
      {==}
    case D.State{_, _, _, _, _, _, _, _, _} False{}:
      {==}

law native_focus_observed_preserves_capture:
  for +state: D.State
  for +seen: D.Owner
  {Router.state_capture(Router.native_focus_observed(state, seen)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def native_focus_observed_preserves_capture(state, seen):
  match state:
    case D.State{map, owner, pending, old, restore, active, capture, claims, effects}:
      observed_preserves_capture(D.State{map, owner, pending, old, restore, active, capture, claims, effects}, seen, Router.observed_matches(pending, seen))

law remove_pending_preserves_capture:
  for +state: D.State
  for +clear: Bool
  {Router.state_capture(Router.remove_pending_choice(state, clear)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def remove_pending_preserves_capture(state, clear):
  match state clear:
    case D.State{_, _, _, _, _, _, _, _, _} True{}:
      {==}
    case _ False{}:
      {==}

law pane_removed_preserves_capture:
  for +state: D.State
  for +pane: Core.StableId
  {Router.state_capture(Router.pane_removed(state, pane)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def pane_removed_preserves_capture(state, pane):
  match state:
    case D.State{map, owner, pending, seen, restore, active, capture, claims, effects}:
      remove_pending_preserves_capture(D.State{map, Router.remove_owner(owner, pane), pending, Router.remove_owner(seen, pane), Router.remove_owner(restore, pane), Router.remove_active(active, pane), capture, Claims.remove_pane(claims, pane), effects}, Router.pending_is_pane(pending, pane))

law detached_preserves_capture:
  for +state: D.State
  {Router.state_capture(Router.detached(state)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def detached_preserves_capture(state):
  match state:
    case D.State{_, _, _, _, _, _, _, _, _}:
      {==}

law every_event_preserves_capture:
  for +state: D.State
  for +event: D.RouterEvent
  {Router.state_capture(Router.event(state, event)) == Router.state_capture(state) : Maybe<&2, Core.StableId>}

def every_event_preserves_capture(state, event):
  match event:
    case D.ActivatePane{pane, surface}:
      activate_preserves_capture(state, pane, surface)
    case D.SetActivePane{pane, surface}:
      active_preserves_capture(state, pane, surface)
    case D.FocusSidebar{}:
      focus_preserves_capture(state, D.Sidebar{})
    case D.FocusNativeEditor{}:
      focus_preserves_capture(state, D.NativeEditor{})
    case D.OverlayOpened{}:
      overlay_opened_preserves_capture(state)
    case D.OverlayClosed{}:
      overlay_closed_preserves_capture(state)
    case D.NativeFocusObserved{owner}:
      native_focus_observed_preserves_capture(state, owner)
    case D.PaneRemoved{pane}:
      pane_removed_preserves_capture(state, pane)
    case D.Detached{}:
      detached_preserves_capture(state)
    case D.WindowDeactivated{}:
      claims_preserve_capture(state, D.Claims{Nil{}, Nil{}})
