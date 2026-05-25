// Calling an `@lib` extern declaration from outside an `@extern(C)`
// block is rejected — the symbol only resolves through the
// extern-C codegen path, and ordinary code that called it would
// also let raw pointer return types (`*const char` here) escape
// into user code.
//
// expect-error: only callable inside an @extern
use std.test as test
@extern(C) {
    @lib("c") fn ghost_returning_cstr(): *const char
}

let p = ghost_returning_cstr()
test.expect(p as i64, 0)
