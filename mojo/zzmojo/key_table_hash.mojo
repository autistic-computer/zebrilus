def _rotl(value: UInt64, count: Int) -> UInt64:
    return value << UInt64(count) | value >> UInt64(64 - count)


def _round(mut state: SIMD[.uint64, 4]):
    state[0] += state[1]
    state[2] += state[3]
    state[1] = _rotl(state[1], 13) ^ state[0]
    state[3] = _rotl(state[3], 16) ^ state[2]
    state[0] = _rotl(state[0], 32)
    state[2] += state[1]
    state[0] += state[3]
    state[1] = _rotl(state[1], 17) ^ state[2]
    state[3] = _rotl(state[3], 21) ^ state[0]
    state[2] = _rotl(state[2], 32)


def key_table_hash(bytes: Span[UInt8, _]) -> UInt64:
    var state = SIMD[.uint64, 4](0x736F6D6570736575, 0x646F72616E646F6D, 0x6C7967656E657261, 0x7465646279746573)
    var offset = 0
    while offset + 8 <= len(bytes):
        var word: UInt64 = 0
        for index in range(8):
            word |= UInt64(bytes[offset + index]) << UInt64(index * 8)
        state[3] ^= word
        _round(state)
        state[0] ^= word
        offset += 8
    var tail = UInt64(len(bytes) & 255) << 56
    for index in range(len(bytes) - offset):
        tail |= UInt64(bytes[offset + index]) << UInt64(index * 8)
    state[3] ^= tail
    _round(state)
    state[0] ^= tail
    state[2] ^= 255
    for _ in range(3):
        _round(state)
    return state[0] ^ state[1] ^ state[2] ^ state[3]
