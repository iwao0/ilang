// `*void` / `*const void` is the C "untyped pointer" and flows into
// any `*T` slot under the usual const-narrowing rule, without an
// explicit `as *T` cast. This is what `NULL` (typed as `*void`)
// relies on so Win32 / COM handle parameters take it directly.
//
// interp: skip

use std.test as test
@extern(C) {
    pub struct Foo {
        n: i32
    }

    pub fn run() {
        // `*void` flows into a `*Foo`-annotated let without `as`.
        let raw: *void = 0 as *void
        let typed: *Foo = raw
        test.expect(typed as i64, 0)

        // `*const void` → `*const Foo` likewise.
        let raw_const: *const void = 0 as *const void
        let typed_const: *const Foo = raw_const
        test.expect(typed_const as i64, 0)
    }
}

run()
