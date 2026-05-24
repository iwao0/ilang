// `host_array_push`'s realloc path doubles the data buffer when
// the capacity is exceeded. Pre-fix the OLD buffer was leaked on
// every grow (~log2(N) leaks per N pushes), so a long
// `xs.push(...)` loop accumulated ~2*N*stride unreachable bytes.
//

use std.test

let baseline = test.liveAllocBytes()

let xs: i64[] = []
let i: i64 = 0
while i < 1000 {
    xs.push(i)
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// 1000 i64 elements at 8 bytes = 8000 bytes for the live array,
// plus the 48-byte header, plus a small constant (rounded-up cap).
// Threshold of 16384 leaves headroom for the next-power-of-two
// capacity (1024 * 8 = 8192 bytes data + 48 header) without
// admitting log2(1000)*N-ish grow leaks (~30+ KB pre-fix).
test.expectTrue(grew < 16384)
