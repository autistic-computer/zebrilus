import Base
import ./terminal_cells.bend as Cells

def word(a: U32, b: U32, c: U32, d: U32) -> U32:
  U32.or(U32.or(a, U32.shln(b, 8n)), U32.or(U32.shln(c, 16n), U32.shln(d, 24n)))

def decoded_done(cells: List<&2, Cells.Cell>, valid: Bool) -> Maybe<&2, List<&2, Cells.Cell>>:
  match valid:
    case False{}:
      None{}
    case True{}:
      Some{cells}

def decode(bytes: List<&2, U32>, remaining: U32, reversed: List<&2, Cells.Cell>) -> Maybe<&2, List<&2, Cells.Cell>>:
  match bytes:
    case Nil{}:
      decoded_done(reversed, U32.is_eq(remaining, 0))
    case a <> b <> c <> d <> e <> f <> g <> h <> rest:
      decode(rest, U32.sub(remaining, 1), Cells.Cell{word(a, b, c, d), word(e, f, g, h)} <> reversed)
    case _:
      None{}

def decoded(value: Maybe<&2, List<&2, Cells.Cell>>) -> IO(List<&2, Cells.Cell>):
  match value:
    case None{}:
      IO.die(List<&2, Cells.Cell>, 65, "terminal cell input changed size while reading")
    case Some{cells}:
      IO.pure(List<&2, Cells.Cell>, cells)

def read_piece(pair: File & Result<&1, &1, U32 & String, List<&2, U32>>, count: U32, remaining: U32, reversed: List<&2, Cells.Cell>, next: File -> U32 -> List<&2, Cells.Cell> -> IO(List<&2, Cells.Cell>)) -> IO(List<&2, Cells.Cell>):
  (file, result) = pair
  do IO<List<&2, Cells.Cell>>:
    bytes : List<&2, U32> <- IO.pass(List<&2, U32>, result)
    cells : List<&2, Cells.Cell> <- decoded(decode(bytes, count, reversed))
    next(file, remaining, cells)

def read_step(file: File, +remaining: U32, reversed: List<&2, Cells.Cell>, next: File -> U32 -> List<&2, Cells.Cell> -> IO(List<&2, Cells.Cell>), empty: Bool) -> IO(List<&2, Cells.Cell>):
  match empty:
    case True{}:
      do IO<List<&2, Cells.Cell>>:
        File.close(file)
        return Cells.reversed(reversed)
    case False{}:
      do IO<List<&2, Cells.Cell>>:
        +count : U32 = U32.min(remaining, 8192)
        pair : File & Result<&1, &1, U32 & String, List<&2, U32>> <- File.read_bytes(file, U32.mul(count, 8))
        read_piece(pair, count, U32.sub(remaining, count), reversed, next)

def read_loop(fuel: Nat, file: File, +remaining: U32, reversed: List<&2, Cells.Cell>) -> IO(List<&2, Cells.Cell>):
  match fuel:
    case 0n:
      IO.die(List<&2, Cells.Cell>, 65, "terminal cell input exceeds its read budget")
    case 1n+rest:
      read_step(file, remaining, reversed, f => n => cells => read_loop(rest, f, n, cells), U32.is_eq(remaining, 0))

def read_sized(file: File, count: U32, valid: Bool) -> IO(List<&2, Cells.Cell>):
  match valid:
    case False{}:
      IO.die(List<&2, Cells.Cell>, 65, "terminal cell input byte count does not match")
    case True{}:
      read_loop(1026n, file, count, Nil{})

def read_size(pair: File & Result<&1, &1, U32 & String, U32>, +count: U32) -> IO(List<&2, Cells.Cell>):
  (file, result) = pair
  do IO<List<&2, Cells.Cell>>:
    size : U32 <- IO.pass(U32, result)
    read_sized(file, count, U32.is_eq(size, U32.mul(count, 8)))

def read(path: String, +count: U32) -> IO(List<&2, Cells.Cell>):
  do IO<List<&2, Cells.Cell>>:
    file : File <- IO.try(File, File.open(path, "r"))
    pair : File & Result<&1, &1, U32 & String, U32> <- File.size(file)
    read_size(pair, count)

type Chunk is Data:
  Chunk{remaining: List<&2, Cells.Cell>, bytes: List<&2, U32>}

def reverse_bytes(bytes: List<&2, U32>, acc: List<&2, U32>) -> List<&2, U32>:
  match bytes:
    case Nil{}:
      acc
    case head <> tail:
      reverse_bytes(tail, head <> acc)

def reversed_word(+word: U32, bytes: List<&2, U32>) -> List<&2, U32>:
  U32.and(U32.shrn(word, 24n), 255) <> U32.and(U32.shrn(word, 16n), 255) <> U32.and(U32.shrn(word, 8n), 255) <> U32.and(word, 255) <> bytes

def encode(cells: List<&2, Cells.Cell>, count: Nat, bytes: List<&2, U32>) -> Chunk:
  match cells count:
    case rest 0n:
      Chunk{rest, reverse_bytes(bytes, Nil{})}
    case Nil{} 1n+_:
      Chunk{Nil{}, reverse_bytes(bytes, Nil{})}
    case Cells.Cell{glyph, style_flags} <> tail 1n+remaining:
      encode(tail, remaining, reversed_word(style_flags, reversed_word(glyph, bytes)))

def written(pair: File & Result<&1, &1, U32 & String, Unit>, cells: List<&2, Cells.Cell>, next: File -> List<&2, Cells.Cell> -> IO(Unit)) -> IO(Unit):
  (file, result) = pair
  do IO<Unit>:
    _ : Unit <- IO.pass(Unit, result)
    next(file, cells)

def write_piece(file: File, chunk: Chunk, next: File -> List<&2, Cells.Cell> -> IO(Unit)) -> IO(Unit):
  match chunk:
    case Chunk{remaining, bytes}:
      do IO<Unit>:
        pair : File & Result<&1, &1, U32 & String, Unit> <- File.write_bytes(file, bytes)
        written(pair, remaining, next)

def write_loop(fuel: Nat, file: File, cells: List<&2, Cells.Cell>) -> IO(Unit):
  match fuel cells:
    case _ Nil{}:
      File.close(file)
    case 0n _:
      IO.die(Unit, 65, "terminal cell output exceeds its write budget")
    case 1n+rest head <> tail:
      write_piece(file, encode(head <> tail, 8192n, Nil{}), f => remaining => write_loop(rest, f, remaining))

def write(path: String, cells: List<&2, Cells.Cell>) -> IO(Unit):
  do IO<Unit>:
    file : File <- IO.try(File, File.open(path, "w"))
    write_loop(1026n, file, cells)
