from std.math import floor
from std.utils.numerics import isfinite


def grid_dimension(available: Float32, cell: Float32, scale: Float32) -> UInt16:
    var cells = floor((available + (Float32(1) / scale) * Float32(0.01)) / cell)
    if not isfinite(cells):
        return 1
    return UInt16(cells.clamp(1, 65535))


@fieldwise_init
struct GridSize(TrivialRegisterPassable):
    var columns: UInt16
    var rows: UInt16
    var cell_width: UInt32
    var cell_height: UInt32
    var generation: UInt64

    def same(self, other: Self) -> Bool:
        return (
            self.columns == other.columns
            and self.rows == other.rows
            and self.cell_width == other.cell_width
            and self.cell_height == other.cell_height
            and self.generation == other.generation
        )
