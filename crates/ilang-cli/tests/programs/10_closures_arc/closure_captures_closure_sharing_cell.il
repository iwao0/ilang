// A closure that captures another closure that itself mutates a
// shared cell — the outer closure sees the mutation through its
// own capture of the same name, because the mutable binding
// becomes a shared heap cell once a closure stores or mutates it.
// Per syntax.md §6 "Nested closures (closure-of-closure) are
// supported — the inner closure may re-capture from the outer's
// captured environment." + "shared cell" semantics.
use std.test as test

fn make(): fn(): i64 {
    let n = 0
    let inc = fn() { n = n + 1 }
    fn(): i64 {
        inc()
        n                 // reads through the same shared cell
    }
}

let f = make()
test.expect(f(), 1)
test.expect(f(), 2)
test.expect(f(), 3)

// Multiple `make()` calls don't share state.
let g = make()
test.expect(g(), 1)
test.expect(f(), 4)        // f's cell is independent
test.expect(g(), 2)
