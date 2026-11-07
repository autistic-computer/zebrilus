from std.memory import ArcPointer
from .browser_host import BrowserHost, BrowserSource
from .browser_state import BrowserModel, BrowserEffect


@fieldwise_init
struct BrowserDeliveryItem(Copyable, Movable):
    var effect: BrowserEffect
    var source: Optional[BrowserSource]
    var dispatch: Bool


@fieldwise_init
struct BrowserDeliveryPlan(Movable):
    var generation: UInt64
    var expected: List[UInt8]
    var model_actor: ArcPointer[UInt8]
    var host_actor: ArcPointer[UInt8]
    var host_generation: UInt64
    var items: List[BrowserDeliveryItem]

    def validate(self, model: BrowserModel, host: BrowserHost) raises:
        model.validate()
        if not host.same_actor(self.host_actor):
            raise Error("browser delivery belongs to another host")
        if not model.matches_identity(self.expected):
            raise Error("browser delivery belongs to another state snapshot")
        if (
            self.generation != model.generation
            or self.host_generation != host.generation
            or len(self.items) != len(model.effects)
        ):
            raise Error("browser delivery receipt is stale")
        for item in self.items:
            if item.source:
                host.validate_owner(item.source.value())

    def acknowledge(self, mut model: BrowserModel) raises:
        model.validate()
        if not model.matches_identity(self.expected):
            raise Error("browser delivery acknowledgement is stale")
        model.acknowledge_effects(self.generation, len(self.items))


@inline(.never)
def prepare_browser_delivery(
    pane: UInt64, model: BrowserModel, host: BrowserHost
) raises -> BrowserDeliveryPlan:
    model.validate()
    if not model.pane or model.pane.value() != pane:
        raise Error("browser delivery belongs to another pane")
    var items = List[BrowserDeliveryItem]()
    for effect in model.effects:
        if effect.kind > 12:
            raise Error("browser delivery has no consumer for this effect")
        if (effect.kind == 4 or effect.kind == 6) and not effect.source_bound:
            raise Error(
                "browser native effect has no event-time source admission"
            )
        var source = effect.source.copy()
        if source and source.value().pane != pane:
            raise Error("browser native effect belongs to another pane")
        var dispatch = effect.kind != 4 and effect.kind != 6 or Bool(source)
        items.append(BrowserDeliveryItem(effect.copy(), source^, dispatch))
    return BrowserDeliveryPlan(
        model.generation,
        model.identity(),
        model.actor.copy(),
        host.actor.copy(),
        host.generation,
        items^,
    )
