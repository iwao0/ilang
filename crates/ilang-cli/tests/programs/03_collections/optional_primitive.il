// Optional<T> with primitive T — JIT heap-boxes the payload so 0 can
// still be a valid Some(0) while None is its own state.
use std.test

// i64
let i_some: i64? = some(42)
let i_none: i64? = none
test.expectBool(i_some.isSome, true)
test.expect(i_some.unwrap(), 42)
test.expectBool(i_none.isNone, true)

// 0 is a valid Some — distinct from None.
let zero_some: i64? = some(0)
test.expectBool(zero_some.isSome, true)
test.expect(zero_some.unwrap(), 0)

// bool
let b_some: bool? = some(true)
test.expectBool(b_some.isSome, true)
test.expectBool(b_some.unwrap(), true)
let b_none: bool? = none
test.expectBool(b_none.isNone, true)

// false-some vs none.
let bf: bool? = some(false)
test.expectBool(bf.isSome, true)
test.expectBool(bf.unwrap(), false)

// f64
let f_some: f64? = some(3.14)
test.expectBool(f_some.isSome, true)
test.expectF64(f_some.unwrap(), 3.14)

// Auto-wrap.
let auto: i64? = 7
test.expect(auto.unwrap(), 7)
