// `pub const NAME: T = expr` is allowed inside `@extern(C) { ... }`
// so binding modules can ship typed C-pointer / size_t constants
// (canonical example: `NULL`). The annotation and the cast on the
// RHS both honor the surrounding extern-C scope, which makes
// `*void` legal here even though it's rejected at module top
// level.
//
// interp: skip

use std.test as test
@extern(C) {
    pub const NULL: *void = 0 as *void

    pub fn run() {
        let p = NULL
        test.expectTrue((p as i64) == 0)
        // Re-cast to a different raw pointer type — the @extern(C)
        // scope on the use site lets the pointer flow through any
        // FFI-typed slot via an explicit `as`.
        let q = NULL as *const char
        test.expectTrue((q as i64) == 0)
    }
}

run()
