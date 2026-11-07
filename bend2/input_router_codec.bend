import Base
import ./core.bend as Core
import ./tree_codec.bend as P
import ./tree_encode.bend as E
import ./input_router_data.bend as D
import ./input_router_transaction.bend as T

def boolean_tag(tag: U32) -> P.Parser(Bool):
  match tag:
    case 0:
      P.Parser.pure(Bool, False{})
    case 1:
      P.Parser.pure(Bool, True{})
    case _:
      P.Parser.failed(Bool)

def boolean() -> P.Parser(Bool):
  do P.Parser<Bool>:
    tag : U32 <- P.word
    boolean_tag(tag)

def token() -> P.Parser(D.KeyToken):
  do P.Parser<D.KeyToken>:
    id : Core.StableId <- P.identifier()
    return D.KeyToken{id}

def action() -> P.Parser(D.ChromeAction):
  do P.Parser<D.ChromeAction>:
    id : Core.StableId <- P.identifier()
    return D.ChromeAction{id}

def surface_tag(tag: U32) -> P.Parser(D.Surface):
  match tag:
    case 0:
      P.Parser.pure(D.Surface, D.Terminal{})
    case 1:
      P.Parser.pure(D.Surface, D.Browser{})
    case 2:
      P.Parser.pure(D.Surface, D.Other{})
    case _:
      P.Parser.failed(D.Surface)

def surface() -> P.Parser(D.Surface):
  do P.Parser<D.Surface>:
    tag : U32 <- P.word
    surface_tag(tag)

def key_action_tag(tag: U32) -> P.Parser(D.KeyAction):
  match tag:
    case 0:
      P.Parser.pure(D.KeyAction, D.KeyPressed{})
    case 1:
      P.Parser.pure(D.KeyAction, D.Repeat{})
    case 2:
      P.Parser.pure(D.KeyAction, D.Release{})
    case _:
      P.Parser.failed(D.KeyAction)

def key_action() -> P.Parser(D.KeyAction):
  do P.Parser<D.KeyAction>:
    tag : U32 <- P.word
    key_action_tag(tag)

def owner_tag(tag: U32) -> P.Parser(D.Owner):
  match tag:
    case 0:
      P.Parser.pure(D.Owner, D.NoOwner{})
    case 1:
      do P.Parser<D.Owner>:
        id : Core.StableId <- P.identifier()
        kind : D.Surface <- surface()
        return D.Pane{id, kind}
    case 2:
      P.Parser.pure(D.Owner, D.Sidebar{})
    case 3:
      P.Parser.pure(D.Owner, D.Overlay{})
    case 4:
      P.Parser.pure(D.Owner, D.NativeEditor{})
    case _:
      P.Parser.failed(D.Owner)

def owner() -> P.Parser(D.Owner):
  do P.Parser<D.Owner>:
    tag : U32 <- P.word
    owner_tag(tag)

def optional_owner_tag(tag: U32) -> P.Parser(Maybe<&2, D.Owner>):
  match tag:
    case 0:
      P.Parser.pure(Maybe<&2, D.Owner>, None{})
    case 1:
      do P.Parser<Maybe<&2, D.Owner>>:
        value : D.Owner <- owner()
        return Some{value}
    case _:
      P.Parser.failed(Maybe<&2, D.Owner>)

def optional_owner() -> P.Parser(Maybe<&2, D.Owner>):
  do P.Parser<Maybe<&2, D.Owner>>:
    tag : U32 <- P.word
    optional_owner_tag(tag)

def optional_token_tag(tag: U32) -> P.Parser(Maybe<&2, D.KeyToken>):
  match tag:
    case 0:
      P.Parser.pure(Maybe<&2, D.KeyToken>, None{})
    case 1:
      do P.Parser<Maybe<&2, D.KeyToken>>:
        value : D.KeyToken <- token()
        return Some{value}
    case _:
      P.Parser.failed(Maybe<&2, D.KeyToken>)

def optional_token() -> P.Parser(Maybe<&2, D.KeyToken>):
  do P.Parser<Maybe<&2, D.KeyToken>>:
    tag : U32 <- P.word
    optional_token_tag(tag)

def optional_action_tag(tag: U32) -> P.Parser(Maybe<&2, D.ChromeAction>):
  match tag:
    case 0:
      P.Parser.pure(Maybe<&2, D.ChromeAction>, None{})
    case 1:
      do P.Parser<Maybe<&2, D.ChromeAction>>:
        value : D.ChromeAction <- action()
        return Some{value}
    case _:
      P.Parser.failed(Maybe<&2, D.ChromeAction>)

def optional_action() -> P.Parser(Maybe<&2, D.ChromeAction>):
  do P.Parser<Maybe<&2, D.ChromeAction>>:
    tag : U32 <- P.word
    optional_action_tag(tag)

def active() -> P.Parser(D.Active):
  do P.Parser<D.Active>:
    id : Core.StableId <- P.identifier()
    kind : D.Surface <- surface()
    return D.Active{id, kind}

def optional_active_tag(tag: U32) -> P.Parser(Maybe<&2, D.Active>):
  match tag:
    case 0:
      P.Parser.pure(Maybe<&2, D.Active>, None{})
    case 1:
      do P.Parser<Maybe<&2, D.Active>>:
        value : D.Active <- active()
        return Some{value}
    case _:
      P.Parser.failed(Maybe<&2, D.Active>)

def optional_active() -> P.Parser(Maybe<&2, D.Active>):
  do P.Parser<Maybe<&2, D.Active>>:
    tag : U32 <- P.word
    optional_active_tag(tag)

def input() -> P.Parser(D.Input):
  do P.Parser<D.Input>:
    key : D.KeyToken <- token()
    unshifted : Maybe<&2, D.KeyToken> <- optional_token()
    action : D.KeyAction <- key_action()
    payload : Core.StableId <- P.identifier()
    return D.Input{key, unshifted, action, D.Payload{payload}}

def effect_tag(tag: U32) -> P.Parser(D.Effect):
  match tag:
    case 0:
      do P.Parser<D.Effect>:
        pane : Core.StableId <- P.identifier()
        value : D.Input <- input()
        return D.Forward{pane, value}
    case 1:
      do P.Parser<D.Effect>:
        value : D.ChromeAction <- action()
        return D.Chrome{value}
    case 2:
      do P.Parser<D.Effect>:
        value : D.Owner <- owner()
        return D.RequestFocus{value}
    case _:
      P.Parser.failed(D.Effect)

def effect() -> P.Parser(D.Effect):
  do P.Parser<D.Effect>:
    tag : U32 <- P.word
    effect_tag(tag)

def held() -> P.Parser(D.Held):
  do P.Parser<D.Held>:
    key : D.KeyToken <- token()
    pane : Core.StableId <- P.identifier()
    return D.Held{key, pane}

def limited_count(parsed: Maybe<&2, P.Parsed<U32>>, limit: U32) -> Maybe<&2, P.Parsed<U32>>:
  match parsed:
    case None{}:
      None{}
    case Some{P.Parsed{+count, reader}}:
      P.accepted_value(U32, count, reader, U32.is_le(count, limit))

def vector(~A: Data, ~element: P.Parser(A), minimum: U32, limit: U32) -> P.Parser(List<&2, A>):
  do P.Parser<List<&2, A>>:
    count : U32 <- reader => limited_count(P.count(minimum)(reader), limit)
    P.vector_count(~A, ~element, count)

def state() -> P.Parser(D.State):
  do P.Parser<D.State>:
    map : Core.StableId <- P.identifier()
    focused : D.Owner <- owner()
    pending : Maybe<&2, D.Owner> <- optional_owner()
    observed : D.Owner <- owner()
    restore : D.Owner <- owner()
    active : Maybe<&2, D.Active> <- optional_active()
    capture : Maybe<&2, Core.StableId> <- P.optional_id()
    held : List<&2, D.Held> <- vector(~D.Held, ~held(), 4, 256)
    local : List<&2, D.KeyToken> <- vector(~D.KeyToken, ~token(), 2, 256)
    effects : List<&2, D.Effect> <- vector(~D.Effect, ~effect(), 2, 512)
    return D.State{map, focused, pending, observed, restore, active, capture, D.Claims{held, local}, effects}

def event_tag(tag: U32) -> P.Parser(D.RouterEvent):
  match tag:
    case 0:
      do P.Parser<D.RouterEvent>:
        pane : Core.StableId <- P.identifier()
        kind : D.Surface <- surface()
        return D.ActivatePane{pane, kind}
    case 1:
      do P.Parser<D.RouterEvent>:
        pane : Core.StableId <- P.identifier()
        kind : D.Surface <- surface()
        return D.SetActivePane{pane, kind}
    case 2:
      P.Parser.pure(D.RouterEvent, D.FocusSidebar{})
    case 3:
      P.Parser.pure(D.RouterEvent, D.FocusNativeEditor{})
    case 4:
      P.Parser.pure(D.RouterEvent, D.OverlayOpened{})
    case 5:
      P.Parser.pure(D.RouterEvent, D.OverlayClosed{})
    case 6:
      do P.Parser<D.RouterEvent>:
        value : D.Owner <- owner()
        return D.NativeFocusObserved{value}
    case 7:
      do P.Parser<D.RouterEvent>:
        pane : Core.StableId <- P.identifier()
        return D.PaneRemoved{pane}
    case 8:
      P.Parser.pure(D.RouterEvent, D.Detached{})
    case 9:
      P.Parser.pure(D.RouterEvent, D.WindowDeactivated{})
    case _:
      P.Parser.failed(D.RouterEvent)

def event() -> P.Parser(D.RouterEvent):
  do P.Parser<D.RouterEvent>:
    tag : U32 <- P.word
    event_tag(tag)

def operation_tag(tag: U32) -> P.Parser(T.Operation):
  match tag:
    case 0:
      do P.Parser<T.Operation>:
        value : D.RouterEvent <- event()
        return T.Event{value}
    case 1:
      do P.Parser<T.Operation>:
        value : Maybe<&2, Core.StableId> <- P.optional_id()
        return T.Capture{value}
    case 2:
      do P.Parser<T.Operation>:
        value : D.KeyToken <- token()
        return T.Suppress{value}
    case 3:
      do P.Parser<T.Operation>:
        value : D.Input <- input()
        armed : Bool <- boolean()
        claimed : Bool <- boolean()
        ui : Maybe<&2, D.ChromeAction> <- optional_action()
        sidebar : Maybe<&2, D.ChromeAction> <- optional_action()
        terminal : Maybe<&2, D.ChromeAction> <- optional_action()
        browser : Maybe<&2, D.ChromeAction> <- optional_action()
        return T.OpKey{value, D.Prefix{armed, claimed}, D.Lookups{ui, sidebar, terminal, browser}}
    case 4:
      P.Parser.pure(T.Operation, T.Drain{})
    case 5:
      do P.Parser<T.Operation>:
        releases : Bool <- boolean()
        ui : Bool <- boolean()
        terminal : Bool <- boolean()
        return T.Plain{releases, ui, terminal}
    case _:
      P.Parser.failed(T.Operation)

def operation() -> P.Parser(T.Operation):
  do P.Parser<T.Operation>:
    tag : U32 <- P.word
    operation_tag(tag)

def request() -> P.Parser(T.Request):
  do P.Parser<T.Request>:
    magic : U32 <- P.word
    version : U32 <- P.word
    _ : Unit <- P.require(Unit, Unit{}, U32.is_eq(magic, 1230131266) && U32.is_eq(version, 1))
    state : D.State <- state()
    operation : T.Operation <- operation()
    return T.Request{state, operation}

def put_bool(builder: E.Builder, value: Bool) -> E.Builder:
  match value:
    case False{}:
      E.put(builder, 0)
    case True{}:
      E.put(builder, 1)

def put_token(builder: E.Builder, value: D.KeyToken) -> E.Builder:
  match value:
    case D.KeyToken{id}:
      E.identifier(builder, id)

def put_action(builder: E.Builder, value: D.ChromeAction) -> E.Builder:
  match value:
    case D.ChromeAction{id}:
      E.identifier(builder, id)

def put_surface(builder: E.Builder, value: D.Surface) -> E.Builder:
  match value:
    case D.Terminal{}:
      E.put(builder, 0)
    case D.Browser{}:
      E.put(builder, 1)
    case D.Other{}:
      E.put(builder, 2)

def put_owner(builder: E.Builder, value: D.Owner) -> E.Builder:
  match value:
    case D.NoOwner{}:
      E.put(builder, 0)
    case D.Pane{pane, kind}:
      put_surface(E.identifier(E.put(builder, 1), pane), kind)
    case D.Sidebar{}:
      E.put(builder, 2)
    case D.Overlay{}:
      E.put(builder, 3)
    case D.NativeEditor{}:
      E.put(builder, 4)

def put_optional_owner(builder: E.Builder, value: Maybe<&2, D.Owner>) -> E.Builder:
  match value:
    case None{}:
      E.put(builder, 0)
    case Some{value}:
      put_owner(E.put(builder, 1), value)

def put_optional_token(builder: E.Builder, value: Maybe<&2, D.KeyToken>) -> E.Builder:
  match value:
    case None{}:
      E.put(builder, 0)
    case Some{value}:
      put_token(E.put(builder, 1), value)

def put_optional_action(builder: E.Builder, value: Maybe<&2, D.ChromeAction>) -> E.Builder:
  match value:
    case None{}:
      E.put(builder, 0)
    case Some{value}:
      put_action(E.put(builder, 1), value)

def put_active(builder: E.Builder, value: Maybe<&2, D.Active>) -> E.Builder:
  match value:
    case None{}:
      E.put(builder, 0)
    case Some{D.Active{pane, kind}}:
      put_surface(E.identifier(E.put(builder, 1), pane), kind)

def put_key_action(builder: E.Builder, value: D.KeyAction) -> E.Builder:
  match value:
    case D.KeyPressed{}:
      E.put(builder, 0)
    case D.Repeat{}:
      E.put(builder, 1)
    case D.Release{}:
      E.put(builder, 2)

def put_input(builder: E.Builder, value: D.Input) -> E.Builder:
  match value:
    case D.Input{key, unshifted, action, D.Payload{payload}}:
      E.identifier(put_key_action(put_optional_token(put_token(builder, key), unshifted), action), payload)

def put_effect(builder: E.Builder, value: D.Effect) -> E.Builder:
  match value:
    case D.Forward{pane, input}:
      put_input(E.identifier(E.put(builder, 0), pane), input)
    case D.Chrome{action}:
      put_action(E.put(builder, 1), action)
    case D.RequestFocus{owner}:
      put_owner(E.put(builder, 2), owner)

def put_effects(values: List<&2, D.Effect>, builder: E.Builder) -> E.Builder:
  match values:
    case Nil{}:
      builder
    case head <> tail:
      put_effects(tail, put_effect(builder, head))

def put_held(values: List<&2, D.Held>, builder: E.Builder) -> E.Builder:
  match values:
    case Nil{}:
      builder
    case D.Held{key, pane} <> tail:
      put_held(tail, E.identifier(put_token(builder, key), pane))

def put_local(values: List<&2, D.KeyToken>, builder: E.Builder) -> E.Builder:
  match values:
    case Nil{}:
      builder
    case head <> tail:
      put_local(tail, put_token(builder, head))

def put_state(builder: E.Builder, value: D.State) -> E.Builder:
  match value:
    case D.State{map, owner, pending, observed, restore, active, capture, D.Claims{+held, +local}, +effects}:
      focus = E.optional_id(put_active(put_owner(put_owner(put_optional_owner(put_owner(E.identifier(builder, map), owner), pending), observed), restore), active), capture)
      claims = put_local(local, E.put(put_held(held, E.put(focus, E.count(D.Held, held))), E.count(D.KeyToken, local)))
      put_effects(effects, E.put(claims, E.count(D.Effect, effects)))

def put_answer(builder: E.Builder, value: T.Answer) -> E.Builder:
  match value:
    case T.NoAnswer{}:
      E.put(builder, 0)
    case T.KeyAnswer{D.Consumed{}}:
      E.put(E.put(builder, 1), 0)
    case T.KeyAnswer{D.Native{}}:
      E.put(E.put(builder, 1), 1)
    case T.EffectsAnswer{+effects}:
      put_effects(effects, E.put(E.put(builder, 2), E.count(D.Effect, effects)))
    case T.PlainAnswer{value}:
      put_bool(E.put(builder, 3), value)

def outcome(value: T.Outcome) -> E.Builder:
  match value:
    case T.Outcome{state, answer}:
      put_answer(put_state(E.put(E.put(E.Builder{0, Nil{}}, 1230131266), 1), state), answer)
