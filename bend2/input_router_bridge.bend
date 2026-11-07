import Base
import ./tree_codec.bend as Parser
import ./tree_encode.bend as Encode
import ./input_router_data.bend as D
import ./tree_word_io.bend as Words
import ./input_router_codec.bend as Codec
import ./input_router_transaction.bend as Transaction

def decoded_value(value: Maybe<&2, Transaction.Request>) -> IO(Transaction.Request):
  match value:
    case None{}:
      IO.die(Transaction.Request, 65, "invalid normalized input request")
    case Some{request}:
      IO.pure(Transaction.Request, request)

def decoded(packet: Words.Words) -> IO(Transaction.Request):
  match packet:
    case Words.Words{count, values}:
      decoded_value(Parser.complete(Transaction.Request, Codec.request()(Parser.Reader{values, count})))

def write_checked(path: String, packet: Words.Words, valid: Bool) -> IO(Unit):
  match packet valid:
    case _ False{}:
      IO.die(Unit, 65, "normalized input result exceeds budget")
    case Words.Words{_, values} True{}:
      do IO<Unit>:
        Words.write(path, values)
        IO.print("0")

def write_packet(path: String, packet: Words.Words) -> IO(Unit):
  match packet:
    case Words.Words{+count, values}:
      write_checked(path, Words.Words{count, values}, U32.is_le(count, 16384))

def write_outcome(path: String, outcome: Transaction.Outcome) -> IO(Unit):
  write_packet(path, Encode.words(Codec.outcome(outcome)))

def process_request(output: String, request: Transaction.Request, valid: Bool) -> IO(Unit):
  match valid:
    case False{}:
      IO.die(Unit, 65, "input transition has no native count capacity")
    case True{}:
      write_outcome(output, Transaction.apply(request))

def admit_request(output: String, +request: Transaction.Request) -> IO(Unit):
  match request:
    case Transaction.Request{D.State{_, _, _, _, _, _, _, D.Claims{held, local}, effects}, _}:
      process_request(output, request, U32.is_lt(Encode.count(D.Held, held), 256) && U32.is_lt(Encode.count(D.KeyToken, local), 256) && U32.is_lt(Encode.count(D.Effect, effects), 512))

def process(input: String, output: String) -> IO(Unit):
  do IO<Unit>:
    packet : Words.Words <- Words.read(input, 65536)
    request : Transaction.Request <- decoded(packet)
    admit_request(output, request)

def reply_args(args: List<&2, String>) -> IO(Unit):
  match args:
    case _ <> "input" <> input <> output <> Nil{}:
      process(input, output)
    case _:
      IO.die(Unit, 64, "usage: zz-bend-input input REQUEST RESULT")

def read_line(file: File, line: String, next: File -> IO(Unit)) -> IO(Unit):
  match line:
    case SNil{}:
      File.close(file)
    case SCon{head, tail}:
      do IO<Unit>:
        reply_args("zz-bend-input" <> String.split(String.trim(SCon{head, tail}), ' '))
        next(file)

def read_pair(pair: File & Result<&1, &1, U32 & String, String>, next: File -> IO(Unit)) -> IO(Unit):
  (file, result) = pair
  IO.bind(String, Unit, IO.pass(String, result), line => read_line(file, line, next))

def serve_lines(fuel: Nat, file: File) -> IO(Unit):
  match fuel:
    case 0n:
      IO.die(Unit, 75, "input worker exhausted its request budget")
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
