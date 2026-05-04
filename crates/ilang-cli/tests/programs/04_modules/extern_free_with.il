// Library-specific deallocator. Many libraries ship their own
// allocator (sqlite3_free, xmlFree, OPENSSL_free, ...) and
// crashing if you mix them with libc::free. The new pattern is
// fully explicit: call the C fn, marshal the returned cstr into
// an ilang string, then hand the raw pointer back to whichever
// free fn the library wants.
//
// We simulate the pattern with `test.countedFree`, a host-side
// stub that calls libc::free *and* bumps a counter; the test
// asserts the counter advances exactly once per `strdup` cleanup.
// interp: skip
use test

@extern(C) {
    @lib("c") fn strdup(s: *const char): *const char
}

let before = test.countedFreeCount()

let raw1 = strdup(cstrFromString("hello"))
let copy1 = stringFromCstr(raw1)
test.countedFree(raw1 as i64)
test.expectStr(copy1, "hello")
test.expect((test.countedFreeCount() - before) as i64, 1)

let raw2 = strdup(cstrFromString("world"))
let copy2 = stringFromCstr(raw2)
test.countedFree(raw2 as i64)
test.expectStr(copy2, "world")
test.expect((test.countedFreeCount() - before) as i64, 2)
