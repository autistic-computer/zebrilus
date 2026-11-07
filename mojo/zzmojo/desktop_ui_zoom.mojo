from std.math import isfinite


def zoom_for_percent(percent: Float32) -> Float32:
    return percent.clamp(50, 300) / Float32(100)


def percent_for_zoom(zoom: Float32) -> Float32:
    return zoom * Float32(100)


struct DesktopUiZoom(Copyable, Movable):
    var zoom: Float32

    def __init__(out self):
        self.zoom = 1

    def set_percent(mut self, percent: Float32) raises -> Bool:
        if not isfinite(percent):
            raise Error("UI zoom percent must be finite")
        var next = zoom_for_percent(percent)
        if next == self.zoom:
            return False
        self.zoom = next
        return True

    def action(mut self, kind: UInt8) raises -> Bool:
        if kind == 1:
            return self.set_percent(percent_for_zoom(self.zoom) + Float32(10))
        if kind == 2:
            return self.set_percent(percent_for_zoom(self.zoom) - Float32(10))
        if kind == 3:
            return self.set_percent(100)
        raise Error("unknown UI zoom action")
