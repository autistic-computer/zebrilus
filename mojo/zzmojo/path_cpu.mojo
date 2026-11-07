from std.sys import CompilationTarget, inlined_assembly


@fieldwise_init
struct PathCpuRegisters(TrivialRegisterPassable):
    var eax: UInt32
    var ebx: UInt32
    var ecx: UInt32
    var edx: UInt32


def _cpuid(leaf: UInt32, subleaf: UInt32) -> PathCpuRegisters:
    return inlined_assembly[
        "cpuid",
        PathCpuRegisters,
        constraints="={ax},={bx},={cx},={dx},0,2",
        has_side_effect=True,
    ](leaf, subleaf)


@inline(.never)
def path_vector_bytes() -> Int:
    comptime if CompilationTarget.is_x86():
        var maximum = _cpuid(0, 0).eax
        var first = _cpuid(1, 0)
        var extended = SIMD[.uint32, 4](0)
        if maximum >= 7:
            var seventh = _cpuid(7, 0)
            extended[1] = seventh.ebx
        var xcr0: UInt32 = 0
        if first.ecx & (1 << 27) != 0:
            xcr0 = inlined_assembly[
                "xgetbv",
                UInt32,
                constraints="={ax},{cx},~{dx}",
                has_side_effect=True,
            ](UInt32(0))
        var avx = first.ecx & (1 << 28) != 0 and xcr0 & 0x06 == 0x06
        var avx2 = avx and extended[1] & (1 << 5) != 0
        if (
            avx2
            and xcr0 & 0xE6 == 0xE6
            and extended[1] & (1 << 16) != 0
            and extended[1] & (1 << 30) != 0
        ):
            return 64
        if avx2:
            return 32
    return 16
