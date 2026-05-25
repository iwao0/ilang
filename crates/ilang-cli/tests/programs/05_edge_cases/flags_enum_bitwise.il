// `@flags` enum: `|`, `&`, `^` on enum values lower through the
// `__enum_box` host helper — extract each operand's tag, combine
// with the integer op, box the i64 result back into the same enum
// type. The result reads back as the combined bit pattern.
//
// Pattern lifted from breakout's `sdl.init(InitFlag.video |
// InitFlag.audio | InitFlag.gameController)` startup call.
use std.test as test
@flags
enum Perm: u32 {
    read = 1
    write = 2
    exec = 4
}

// OR — combine bits.
let r = Perm.read | Perm.write
test.expect((r as u32) as i64, 3)

let all = Perm.read | Perm.write | Perm.exec
test.expect((all as u32) as i64, 7)

// AND — mask down.
let masked = (Perm.read | Perm.write) & Perm.read
test.expect((masked as u32) as i64, 1)

// XOR — toggle.
let xor = Perm.read ^ Perm.write
test.expect((xor as u32) as i64, 3)

// Round-trip back into a flags enum value (ensures the boxed
// result is a proper enum that can re-enter bitwise ops).
let combo = (Perm.read | Perm.exec) | Perm.write
test.expect((combo as u32) as i64, 7)
