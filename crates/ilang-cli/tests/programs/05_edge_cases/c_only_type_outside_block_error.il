// Calling an FFI fn that returns a C-only type from outside the
// `@extern(C)` block is rejected even when the call itself looks
// well-typed — this stops raw pointers from escaping into user code
// via FFI return values.
//
// expect-error: only allowed inside an @extern(C)
use test

@extern(C) {
    @lib("c") fn ghost_returning_cstr(): *const char
}

let p = ghost_returning_cstr()
test.expect(p as i64, 0)
