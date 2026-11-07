from .protocol import (
    Decoder,
    append_uint,
    append_text,
    append_bytes,
    envelope,
    validate_envelope,
)
from .tree_wire import read_bool, read_enum, read_text, read_uint

comptime CLIENT_FILE_REQUEST: UInt64 = 34
comptime CLIENT_FILE_RESPONSE: UInt64 = 35
comptime MAX_CLIENT_FILE_BYTES = 32 * 1024 * 1024
comptime MAX_CLIENT_FILE_PATH = 16 * 1024
comptime MAX_CLIENT_FILE_ERROR = 64 * 1024


struct ClientFileRequest(Copyable, Movable):
    var request: UInt64
    var path: String
    var operation: UInt8
    var append: Bool
    var data: List[UInt8]
    var binary: Bool

    def __init__(out self):
        self.request = 0
        self.path = String()
        self.operation = 0
        self.append = False
        self.data = List[UInt8]()
        self.binary = False


@fieldwise_init
struct ClientFileResponse(Copyable, Movable):
    var request: UInt64
    var data: List[UInt8]
    var error: Optional[String]


def _payload(mut reader: Decoder) raises -> List[UInt8]:
    var count = reader.uint()
    if count > MAX_CLIENT_FILE_BYTES or count > UInt64(
        len(reader.data) - reader.offset
    ):
        raise Error("client file data exceeds its frame or byte limit")
    var data = List[UInt8](
        Span(reader.data)[reader.offset : reader.offset + Int(count)]
    )
    reader.offset += Int(count)
    return data^


def decode_file_request(
    var frame: List[UInt8],
) raises -> Optional[ClientFileRequest]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_uint(reader, 4294967295) != CLIENT_FILE_REQUEST:
        return None
    var request = ClientFileRequest()
    request.request = reader.uint()
    request.path = read_text(reader, MAX_CLIENT_FILE_PATH)
    request.operation = read_enum(reader, 3)
    if request.operation == 1:
        request.append = read_bool(reader)
        request.data = _payload(reader)
    elif request.operation == 2:
        request.binary = read_bool(reader)
    return request^


def encode_file_request(request: ClientFileRequest) raises -> List[UInt8]:
    if (
        request.path.byte_length() > MAX_CLIENT_FILE_PATH
        or len(request.data) > MAX_CLIENT_FILE_BYTES
        or request.operation > 3
    ):
        raise Error("client file request exceeds its protocol limits")
    var bytes = List[UInt8]()
    append_uint(bytes, CLIENT_FILE_REQUEST)
    append_uint(bytes, request.request)
    append_text(bytes, request.path)
    append_uint(bytes, UInt64(request.operation))
    if request.operation == 1:
        bytes.append(UInt8(request.append))
        append_bytes(bytes, Span(request.data))
    elif request.operation == 2:
        bytes.append(UInt8(request.binary))
    return envelope(Span(bytes))


def decode_file_response(
    var frame: List[UInt8],
) raises -> Optional[ClientFileResponse]:
    if validate_envelope(Span(frame)) != 0:
        return None
    var reader = Decoder(frame^)
    if read_uint(reader, 4294967295) != CLIENT_FILE_RESPONSE:
        return None
    var id = reader.uint()
    var data = _payload(reader)
    var error: Optional[String] = None
    if read_bool(reader):
        error = read_text(reader, MAX_CLIENT_FILE_ERROR)
    return ClientFileResponse(id, data^, error^)


def encode_file_response(response: ClientFileResponse) raises -> List[UInt8]:
    if len(response.data) > MAX_CLIENT_FILE_BYTES or (
        Bool(response.error)
        and response.error.value().byte_length() > MAX_CLIENT_FILE_ERROR
    ):
        raise Error("client file response exceeds its protocol limits")
    var bytes = List[UInt8]()
    append_uint(bytes, CLIENT_FILE_RESPONSE)
    append_uint(bytes, response.request)
    append_bytes(bytes, Span(response.data))
    bytes.append(UInt8(Bool(response.error)))
    if response.error:
        append_text(bytes, response.error.value())
    return envelope(Span(bytes))
