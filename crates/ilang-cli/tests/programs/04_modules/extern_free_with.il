// Library-specific deallocator. Many libraries ship their own
// allocator (sqlite3_free, xmlFree, OPENSSL_free, ...) and crash
// if you mix them with libc::free. The new pattern keeps the raw
// pointer entirely inside the `@extern(C)` block — the user-facing
// fn returns an owned ilang `string` and runs the library's own
// free for cleanup before yielding the copy.
//
// We simulate the pattern with `test.countedFree`, a host-side
// stub that calls libc::free *and* bumps a counter; the test
// asserts the counter advances exactly once per `dupCounted`.
// interp: skip
use test

@extern(C) {
    @lib("c") fn strdup(s: *const char): *const char

    // strdup → ilang string copy → library-specific cleanup.
    // The raw `*const char` never escapes this block.
    fn dupCounted(s: string): string {
        let raw = strdup(cstrFromString(s))
        let copy = stringFromCstr(raw)
        test.countedFree(raw as i64)
        copy
    }
}

let before = test.countedFreeCount()

let copy1 = dupCounted("hello")
test.expectStr(copy1, "hello")
test.expect((test.countedFreeCount() - before) as i64, 1)

let copy2 = dupCounted("world")
test.expectStr(copy2, "world")
test.expect((test.countedFreeCount() - before) as i64, 2)
