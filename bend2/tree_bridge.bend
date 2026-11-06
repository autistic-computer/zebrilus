import Base
import ./tree_codec.bend as Codec
import ./tree_data.bend as Tree
import ./tree_encode.bend as Encode
import ./tree_reducer.bend as Reducer
import ./tree_word_io.bend as WordIO

type Mode is Data:
  DeltaMode{}
  OverlayMode{}
  ViewMode{}

def word_count(packet: WordIO.Words) -> U32:
  match packet:
    case WordIO.Words{count, _}:
      count

def decoded(-A: Data, value: Maybe<&2, A>) -> IO(A):
  match value:
    case None{}:
      IO.die(A, 65, "malformed normalized tree packet")
    case Some{result}:
      IO.pure(A, result)

def snapshot(packet: WordIO.Words) -> IO(Tree.TreeSnapshot):
  match packet:
    case WordIO.Words{count, words}:
      decoded(Tree.TreeSnapshot, Codec.complete(Tree.TreeSnapshot, Codec.snapshot()(Codec.Reader{words, count})))

def updated(mode: Mode, snapshot: Tree.TreeSnapshot, packet: WordIO.Words) -> IO(Tree.Outcome):
  match mode packet:
    case DeltaMode{} WordIO.Words{count, words}:
      do IO<Tree.Outcome>:
        value : Tree.Delta <- decoded(Tree.Delta, Codec.complete(Tree.Delta, Codec.delta()(Codec.Reader{words, count})))
        return Reducer.apply(snapshot, value)
    case OverlayMode{} WordIO.Words{count, words}:
      do IO<Tree.Outcome>:
        value : List<&2, Tree.Op> <- decoded(List<&2, Tree.Op>, Codec.complete(List<&2, Tree.Op>, Codec.operations()(Codec.Reader{words, count})))
        return Reducer.overlay(snapshot, value)
    case ViewMode{} WordIO.Words{count, words}:
      do IO<Tree.Outcome>:
        value : Tree.ClientView <- decoded(Tree.ClientView, Codec.complete(Tree.ClientView, Codec.view()(Codec.Reader{words, count})))
        return Reducer.client_view(snapshot, value)

def write_packet(path: String, code: U32, packet: WordIO.Words, within_limit: Bool) -> IO(Unit):
  match packet within_limit:
    case _ False{}:
      IO.die(Unit, 65, "normalized tree output exceeds its byte budget")
    case WordIO.Words{_, words} True{}:
      do IO<Unit>:
        WordIO.write(path, words)
        IO.print(U32.show(code))

def reply_outcome(path: String, +outcome: Tree.Outcome) -> IO(Unit):
  +packet = Encode.words(Encode.packet(outcome))
  write_packet(path, Encode.status(Reducer.outcome_error(outcome)), packet, U32.is_le(word_count(packet), 16777216))

def process(mode: Mode, current: String, update: String, output: String) -> IO(Unit):
  do IO<Unit>:
    +input : WordIO.Words <- WordIO.read(current, 67108800)
    size : U32 = U32.mul(word_count(input), 4)
    original : Tree.TreeSnapshot <- snapshot(input)
    patch : WordIO.Words <- WordIO.read(update, U32.sub(67108800, size))
    result : Tree.Outcome <- updated(mode, original, patch)
    reply_outcome(output, result)

def reply_args(args: List<&2, String>) -> IO(Unit):
  match args:
    case _ <> "tree" <> "delta" <> current <> update <> output <> Nil{}:
      process(DeltaMode{}, current, update, output)
    case _ <> "tree" <> "overlay" <> current <> update <> output <> Nil{}:
      process(OverlayMode{}, current, update, output)
    case _ <> "tree" <> "view" <> current <> update <> output <> Nil{}:
      process(ViewMode{}, current, update, output)
    case _:
      IO.die(Unit, 64, "usage: zz-bend-tree tree MODE CURRENT UPDATE OUTPUT")

def read_line(file: File, line: String, next: File -> IO(Unit)) -> IO(Unit):
  match line:
    case SNil{}:
      File.close(file)
    case SCon{head, tail}:
      do IO<Unit>:
        reply_args("zz-bend-tree" <> String.split(String.trim(SCon{head, tail}), ' '))
        next(file)

def read_pair(pair: File & Result<&1, &1, U32 & String, String>, next: File -> IO(Unit)) -> IO(Unit):
  (file, result) = pair
  IO.bind(String, Unit, IO.pass(String, result), line => read_line(file, line, next))

def serve_lines(fuel: Nat, file: File) -> IO(Unit):
  match fuel:
    case 0n:
      IO.die(Unit, 75, "tree worker exhausted its request budget")
    case 1n+remaining:
      do IO<Unit>:
        pair : File & Result<&1, &1, U32 & String, String> <- File.read_line(file, 16384)
        read_pair(pair, next => serve_lines(remaining, next))

def serve() -> IO(Unit):
  do IO<Unit>:
    file : File <- IO.try(File, File.open("/dev/stdin", "r"))
    serve_lines(4294967295n, file)

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
      reply_args(copied_args(rest))

def main() -> IO(Unit):
  do IO<Unit>:
    args : List<String> <- IO.args()
    dispatch(args)
