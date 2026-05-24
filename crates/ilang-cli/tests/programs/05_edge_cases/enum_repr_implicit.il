// `pub enum E: T { ... }` values flow into a `T`-typed slot
// without an explicit `as T` cast. Covers assignment positions
// (function args / struct-field init) and `==` / `!=` comparisons
// (`msg == WindowMessage.destroy` where `msg: u32`).
//
// Restricted to the enum's declared repr — a `u32`-repr enum
// stays distinct from `i64` etc., to avoid surprising widening.

use std.test

pub enum Code: u32 {
    ok      = 0
    notFound = 1
    forbidden = 2
}

fn classify(c: u32): i64 {
    if c == Code.ok { 0 }
    elif c == Code.notFound { 1 }
    else { 2 }
}

fn take(x: u32): u32 { x }

test.expect(classify(0 as u32), 0)
test.expect(classify(1 as u32), 1)
test.expect(classify(2 as u32), 2)

// Enum → repr in an arg slot, no cast.
test.expect(take(Code.notFound) as i64, 1)
