// `@extern("lib", owned_return, free_with.<fn>)` — direct ilang
// at the owned_return cleanup site to call a library-specific
// deallocator instead of `libc::free`.
//
// Many libraries ship their own allocator (sqlite3_free, xmlFree,
// OPENSSL_free, ...) and crashing if you mix them with libc::free.
// We simulate the pattern with `test.countedFree`, a Rust-side
// stub that calls libc::free *and* bumps a counter; the test
// asserts the counter advances exactly once per `strdup` cleanup,
// proving the custom free fn was the one selected.
// interp: skip
use test

@extern("c", owned_return, free_with.test.countedFree)
fn strdup(s: string): string

let before = test.countedFreeCount()

let copy1 = strdup("hello")
test.expectStr(copy1, "hello")
test.expect((test.countedFreeCount() - before) as i64, 1)

let copy2 = strdup("world")
test.expectStr(copy2, "world")
test.expect((test.countedFreeCount() - before) as i64, 2)
