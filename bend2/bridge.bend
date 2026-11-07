import Base
import ./core.bend as Core
import ./terminal_bridge.bend as Terminal
import ./terminal_cells_bridge.bend as Cells
import ./tree_bridge.bend as Tree
import ./input_router_bridge.bend as Input

def validate_args(args: List<&2, String>) -> Maybe<&2, U32>:
  match args:
    case _ <> "validate" <> total <> declared <> lane <> flags <> version <> Nil{}:
      do Maybe<&2, U32>:
        t : U32 <- U32.read(total)
        d : U32 <- U32.read(declared)
        l : U32 <- U32.read(lane)
        f : U32 <- U32.read(flags)
        v : U32 <- U32.read(version)
        return Core.status_code(Core.validate_envelope(t, d, l, f, v))
    case rest:
      Terminal.validate_args(rest)

def reply(result: Maybe<&2, U32>) -> IO(Unit):
  match result:
    case None{}:
      IO.die(Unit, 64, "usage: zz-bend validate TOTAL DECLARED LANE FLAGS VERSION | terminal HEADER SPANS")
    case Some{code}:
      IO.print(U32.show(code))

def reply_command(args: List<&2, String>) -> IO(Unit):
  match args:
    case _ <> "input" <> current <> output <> Nil{}:
      Input.reply_args("zz-bend" <> "input" <> current <> output <> Nil{})
    case _ <> "tree" <> mode <> current <> update <> output <> Nil{}:
      Tree.reply_args("zz-bend" <> "tree" <> mode <> current <> update <> output <> Nil{})
    case _ <> "cells" <> current <> changed <> output <> rest:
      Cells.reply(current, changed, output, Terminal.read_args("zz-bend" <> "terminal" <> rest))
    case rest:
      reply(validate_args(rest))


def read_line(file: File, line: String, next: File -> IO(Unit)) -> IO(Unit):
  match line:
    case SNil{}:
      File.close(file)
    case SCon{head, tail}:
      do IO<Unit>:
        reply_command("zz-bend" <> String.split(String.trim(SCon{head, tail}), ' '))
        next(file)

def read_pair(pair: File & Result<&1, &1, U32 & String, String>, next: File -> IO(Unit)) -> IO(Unit):
  (input, result) = pair
  IO.bind(String, Unit, IO.pass(String, result), line => read_line(input, line, next))

def serve_lines(fuel: Nat, file: File) -> IO(Unit):
  match fuel:
    case 0n:
      IO.die(Unit, 75, "Bend validator exhausted its frame budget")
    case 1n+remaining:
      do IO<Unit>:
        pair : File & Result<&1, &1, U32 & String, String> <- File.read_line(file, 2097152)
        read_pair(pair, next => serve_lines(remaining, next))

def serve() -> IO(Unit):
  do IO<Unit>:
    input : File <- IO.try(File, File.open("/dev/stdin", "r"))
    serve_lines(4294967295n, input)

def copied_args(args: List<String>) -> List<&2, String>:
  match args:
    case Nil{}:
      Nil{}
    case head <> tail:
      head <> copied_args(tail)

def dispatch(args: List<String>) -> IO(Unit):
  match args:
    case _ <> "serve" <> Nil{}:
      serve()
    case rest:
      reply_command(copied_args(rest))

def main() -> IO(Unit):
  do IO<Unit>:
    args : List<String> <- IO.args()
    dispatch(args)
