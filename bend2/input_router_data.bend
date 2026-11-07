import Base
import ./core.bend as Core

type Surface is Data:
  Terminal{}
  Browser{}
  Other{}

type Owner is Data:
  NoOwner{}
  Pane{pane: Core.StableId, surface: Surface}
  Sidebar{}
  Overlay{}
  NativeEditor{}

type Active is Data:
  Active{pane: Core.StableId, surface: Surface}

type KeyToken is Data:
  KeyToken{id: Core.StableId}

type Payload is Data:
  Payload{id: Core.StableId}

type ChromeAction is Data:
  ChromeAction{id: Core.StableId}

type KeyAction is Data:
  KeyPressed{}
  Repeat{}
  Release{}

type Input is Data:
  Input{key: KeyToken, unshifted: Maybe<&2, KeyToken>, action: KeyAction, payload: Payload}

type Prefix is Data:
  Prefix{armed: Bool, claimed: Bool}

type Lookups is Data:
  Lookups{ui: Maybe<&2, ChromeAction>, sidebar: Maybe<&2, ChromeAction>, terminal: Maybe<&2, ChromeAction>, browser: Maybe<&2, ChromeAction>}

type Effect is Data:
  Forward{pane: Core.StableId, input: Input}
  Chrome{action: ChromeAction}
  RequestFocus{owner: Owner}

type Held is Data:
  Held{key: KeyToken, pane: Core.StableId}

type Claims is Data:
  Claims{held: List<&2, Held>, local: List<&2, KeyToken>}

type State is Data:
  State{keymap: Core.StableId, owner: Owner, pending: Maybe<&2, Owner>, observed: Owner, restore: Owner, active: Maybe<&2, Active>, capture: Maybe<&2, Core.StableId>, claims: Claims, effects: List<&2, Effect>}

type RouterEvent is Data:
  ActivatePane{pane: Core.StableId, surface: Surface}
  SetActivePane{pane: Core.StableId, surface: Surface}
  FocusSidebar{}
  FocusNativeEditor{}
  OverlayOpened{}
  OverlayClosed{}
  NativeFocusObserved{owner: Owner}
  PaneRemoved{pane: Core.StableId}
  Detached{}
  WindowDeactivated{}

type Disposition is Data:
  Consumed{}
  Native{}

type Transition is Data:
  Transition{state: State, disposition: Disposition}

def key_equal(left: KeyToken, right: KeyToken) -> Bool:
  match left right:
    case KeyToken{a} KeyToken{b}:
      Core.id_equal(a, b)

def surface_equal(left: Surface, right: Surface) -> Bool:
  match left right:
    case Terminal{} Terminal{}:
      True{}
    case Browser{} Browser{}:
      True{}
    case Other{} Other{}:
      True{}
    case _ _:
      False{}

def owner_equal(left: Owner, right: Owner) -> Bool:
  match left right:
    case NoOwner{} NoOwner{}:
      True{}
    case Pane{a, ak} Pane{b, bk}:
      Core.id_equal(a, b) && surface_equal(ak, bk)
    case Sidebar{} Sidebar{}:
      True{}
    case Overlay{} Overlay{}:
      True{}
    case NativeEditor{} NativeEditor{}:
      True{}
    case _ _:
      False{}

def effective_owner(owner: Owner, pending: Maybe<&2, Owner>) -> Owner:
  match pending:
    case None{}:
      owner
    case Some{requested}:
      requested

def owner(state: State) -> Owner:
  match state:
    case State{_, actual, pending, _, _, _, _, _, _}:
      effective_owner(actual, pending)

def normalized_key(input: Input) -> KeyToken:
  match input:
    case Input{key, None{}, _, _}:
      key
    case Input{_, Some{key}, _, _}:
      key

def new(keymap: Core.StableId) -> State:
  State{keymap, NoOwner{}, None{}, NoOwner{}, NoOwner{}, None{}, None{}, Claims{Nil{}, Nil{}}, Nil{}}

type Drained is Data:
  Drained{state: State, effects: List<&2, Effect>}
