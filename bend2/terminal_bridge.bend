import Base
import ./core.bend as Core
import ./terminal.bend as Terminal

type Request is Data:
  Request{viewport: Terminal.Viewport, patch: Terminal.Patch}

def read_bool.choose(value: U32, zero: Bool, one: Bool) -> Maybe<&2, Bool>:
  match zero one:
    case True{} _:
      Some{False{}}
    case False{} True{}:
      Some{True{}}
    case False{} False{}:
      None{}

def read_bool(text: String) -> Maybe<&2, Bool>:
  do Maybe<&2, Bool>:
    +value : U32 <- U32.read(text)
    read_bool.choose(value, U32.is_eq(value, 0), U32.is_eq(value, 1))

def read_spans_fold(args: List<&2, String>, reversed: List<&2, Terminal.Span>) -> Maybe<&2, List<&2, Terminal.Span>>:
  match args:
    case Nil{}:
      Some{Terminal.reverse_spans(reversed, Nil{})}
    case row <> start <> length <> clear <> tail:
      do Maybe<&2, List<&2, Terminal.Span>>:
        r : U32 <- U32.read(row)
        s : U32 <- U32.read(start)
        l : U32 <- U32.read(length)
        c : Bool <- read_bool(clear)
        read_spans_fold(tail, Terminal.Span{r, s, l, c} <> reversed)
    case _:
      None{}

def read_spans(args: List<&2, String>) -> Maybe<&2, List<&2, Terminal.Span>>:
  read_spans_fold(args, Nil{})

def read_args(args: List<&2, String>) -> Maybe<&2, Request>:
  match args:
    case _ <> "terminal" <> cgh <> cgl <> cvh <> cvl <> cd <> cc <> cr <> cs <> cg <> cb <> pbh <> pbl <> pbvh <> pbvl <> pgh <> pgl <> pvh <> pvl <> pd <> pc <> pr <> sb <> sa <> gb <> ga <> ba <> positive <> shift <> cells <> tail:
      do Maybe<&2, Request>:
        c_gen_hi : U32 <- U32.read(cgh)
        c_gen_lo : U32 <- U32.read(cgl)
        c_view_hi : U32 <- U32.read(cvh)
        c_view_lo : U32 <- U32.read(cvl)
        c_dict : U32 <- U32.read(cd)
        c_columns : U32 <- U32.read(cc)
        c_rows : U32 <- U32.read(cr)
        c_styles : U32 <- U32.read(cs)
        c_graphemes : U32 <- U32.read(cg)
        c_bytes : U32 <- U32.read(cb)
        p_base_hi : U32 <- U32.read(pbh)
        p_base_lo : U32 <- U32.read(pbl)
        p_base_view_hi : U32 <- U32.read(pbvh)
        p_base_view_lo : U32 <- U32.read(pbvl)
        p_gen_hi : U32 <- U32.read(pgh)
        p_gen_lo : U32 <- U32.read(pgl)
        p_view_hi : U32 <- U32.read(pvh)
        p_view_lo : U32 <- U32.read(pvl)
        p_dict : U32 <- U32.read(pd)
        p_columns : U32 <- U32.read(pc)
        p_rows : U32 <- U32.read(pr)
        style_base : U32 <- U32.read(sb)
        added_styles : U32 <- U32.read(sa)
        grapheme_base : U32 <- U32.read(gb)
        added_graphemes : U32 <- U32.read(ga)
        added_bytes : U32 <- U32.read(ba)
        scroll_positive : Bool <- read_bool(positive)
        scroll_rows : U32 <- U32.read(shift)
        cell_count : U32 <- U32.read(cells)
        spans : List<&2, Terminal.Span> <- read_spans(tail)
        return Request{Terminal.Viewport{Core.Id{c_gen_hi, c_gen_lo}, Core.Id{c_view_hi, c_view_lo}, Terminal.Grid{c_columns, c_rows}, Terminal.Dictionary{c_dict, c_styles, c_graphemes, c_bytes}}, Terminal.Patch{Core.Id{p_base_hi, p_base_lo}, Core.Id{p_base_view_hi, p_base_view_lo}, Core.Id{p_gen_hi, p_gen_lo}, Core.Id{p_view_hi, p_view_lo}, p_dict, Terminal.Grid{p_columns, p_rows}, style_base, added_styles, grapheme_base, added_graphemes, added_bytes, scroll_positive, scroll_rows, cell_count, spans}}
    case _:
      None{}

def request_status(request: Request) -> U32:
  match request:
    case Request{viewport, patch}:
      Terminal.status(viewport, patch)

def validate_args(args: List<&2, String>) -> Maybe<&2, U32>:
  do Maybe<&2, U32>:
    request : Request <- read_args(args)
    return request_status(request)
