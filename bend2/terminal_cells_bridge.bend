import Base
import ./terminal.bend as Terminal
import ./terminal_bridge.bend as Bridge
import ./terminal_cells.bend as Cells
import ./terminal_cells_io.bend as CellIO

def bounded(request: Bridge.Request) -> Bool:
  match request:
    case Bridge.Request{Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _}, Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, cells, _}}:
      U32.is_le(U32.mul(columns, rows), 8388608) && U32.is_le(cells, 8388608)

def input_count(request: Bridge.Request) -> U32:
  match request:
    case Bridge.Request{Terminal.Viewport{_, _, Terminal.Grid{columns, rows}, _}, _}:
      U32.mul(columns, rows)

def changed_count(request: Bridge.Request) -> U32:
  match request:
    case Bridge.Request{_, Terminal.Patch{_, _, _, _, _, _, _, _, _, _, _, _, _, cells, _}}:
      cells

def apply(request: Bridge.Request, cells: List<&2, Cells.Cell>, changed: List<&2, Cells.Cell>) -> Cells.Outcome:
  match request:
    case Bridge.Request{viewport, patch}:
      Cells.apply(cells, changed, viewport, patch)

def result(output: String, code: U32, cells: List<&2, Cells.Cell>, accepted: Bool) -> IO(Unit):
  match accepted:
    case False{}:
      IO.print(U32.show(code))
    case True{}:
      do IO<Unit>:
        CellIO.write(output, cells)
        IO.print("0")

def reply_result(output: String, outcome: Cells.Outcome) -> IO(Unit):
  match outcome:
    case Cells.Outcome{+code, cells}:
      result(output, code, cells, U32.is_eq(code, 0))

def process(current: String, changed: String, output: String, +request: Bridge.Request, within_limit: Bool) -> IO(Unit):
  match within_limit:
    case False{}:
      IO.print("4")
    case True{}:
      do IO<Unit>:
        cells : List<&2, Cells.Cell> <- CellIO.read(current, input_count(request))
        payload : List<&2, Cells.Cell> <- CellIO.read(changed, changed_count(request))
        reply_result(output, apply(request, cells, payload))

def preflight(current: String, changed: String, output: String, +request: Bridge.Request, code: U32, accepted: Bool) -> IO(Unit):
  match accepted:
    case False{}:
      IO.print(U32.show(code))
    case True{}:
      process(current, changed, output, request, bounded(request))

def reply(current: String, changed: String, output: String, request: Maybe<&2, Bridge.Request>) -> IO(Unit):
  match request:
    case None{}:
      IO.die(Unit, 64, "usage: zz-bend cells CURRENT CHANGED OUTPUT HEADER SPANS")
    case Some{+value}:
      +code = Bridge.request_status(value)
      preflight(current, changed, output, value, code, U32.is_eq(code, 0))
