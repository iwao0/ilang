// Pass a top-level ilang fn as a C callback. `test.applyI32Cb` is
// a host extern declared as
//   extern "C" fn(cb: extern "C" fn(i64, i64) -> i32, a: i64, b: i64) -> i32
// on the Rust side. It calls the callback directly, so this test
// actually exercises the cross-ABI invocation rather than just the
// pointer marshalling.
use test

fn cmp(a: i64, b: i64): i32 {
    if a < b { -1 as i32 }
    elif a > b { 1 as i32 }
    else { 0 as i32 }
}

fn add_i32(a: i64, b: i64): i32 {
    (a + b) as i32
}

// Compare 5 < 10 → -1
test.expect(test.applyI32Cb(cmp, 5, 10) as i64, -1)
// Compare 10 < 5 → 1
test.expect(test.applyI32Cb(cmp, 10, 5) as i64, 1)
// Compare 7 == 7 → 0
test.expect(test.applyI32Cb(cmp, 7, 7) as i64, 0)
// A different callback to confirm we pass distinct addresses.
test.expect(test.applyI32Cb(add_i32, 3, 4) as i64, 7)
