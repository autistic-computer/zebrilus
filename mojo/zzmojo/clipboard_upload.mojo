from .protocol import (
    Decoder,
    append_uint,
    append_text,
    append_bytes,
    envelope,
    validate_envelope,
)
from .tree_wire import read_enum, read_uint, read_text, read_count

comptime MAX_PASTE_UPLOAD_BYTES: Int = 6 * 1024 * 1024
comptime MAX_PASTE_UPLOAD_CHUNK_BYTES: Int = 1024 * 1024
comptime MAX_PASTE_UPLOADS: Int = 2


@fieldwise_init
struct PasteUploadFrame(Movable):
    var kind: UInt8
    var id: UInt64
    var pane: UInt64
    var purpose: UInt8
    var extension: String
    var total: UInt32
    var bytes: List[UInt8]


def valid_upload(extension: String, purpose: UInt8, total: UInt32) -> Bool:
    if purpose > 1 or total == 0 or total > UInt32(MAX_PASTE_UPLOAD_BYTES):
        return False
    if extension.byte_length() == 0 or extension.byte_length() > 8:
        return False
    for byte in extension.as_bytes():
        if not (byte >= 97 and byte <= 122) and not (byte >= 48 and byte <= 57):
            return False
    return (
        purpose == 0
        or extension == "png"
        or extension == "jpg"
        or extension == "jpeg"
        or extension == "gif"
        or extension == "webp"
    )


def paste_upload_begin(
    id: UInt64,
    pane: UInt64,
    extension: String,
    total: UInt32,
    purpose: UInt8 = 0,
) raises -> List[UInt8]:
    if not valid_upload(extension, purpose, total):
        raise Error("paste upload is malformed")
    var bytes: List[UInt8] = [15]
    append_uint(bytes, id)
    append_uint(bytes, pane)
    append_uint(bytes, UInt64(purpose))
    append_text(bytes, extension)
    append_uint(bytes, UInt64(total))
    return envelope(Span(bytes))


def paste_upload_chunk(id: UInt64, data: Span[UInt8, _]) raises -> List[UInt8]:
    if len(data) > MAX_PASTE_UPLOAD_CHUNK_BYTES:
        raise Error("paste upload chunk exceeds byte limit")
    var bytes: List[UInt8] = [16]
    append_uint(bytes, id)
    append_bytes(bytes, data)
    return envelope(Span(bytes))


def decode_paste_upload(var bytes: List[UInt8]) raises -> PasteUploadFrame:
    if validate_envelope(Span(bytes)) != 0:
        raise Error("paste upload has no control envelope")
    var reader = Decoder(bytes^)
    var kind = read_enum(reader, 75)
    var id = reader.uint()
    if kind == 15:
        var pane = reader.uint()
        var purpose = read_enum(reader, 1)
        var extension = read_text(reader, 8)
        var total = UInt32(read_uint(reader, 4294967295))
        if not valid_upload(extension, purpose, total):
            raise Error("paste upload is malformed")
        return PasteUploadFrame(
            kind, id, pane, purpose, extension^, total, List[UInt8]()
        )
    if kind != 16:
        raise Error("frame is not a paste upload")
    var count = read_count(reader, MAX_PASTE_UPLOAD_CHUNK_BYTES)
    var data = List[UInt8](capacity=count)
    for _ in range(count):
        data.append(reader.byte())
    return PasteUploadFrame(kind, id, 0, 0, String(), 0, data^)
