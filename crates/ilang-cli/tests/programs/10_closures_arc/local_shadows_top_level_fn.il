// A `let f = closure` inside a fn body shadows a top-level `fn f`
// of the same name — the inner closure that references `f`
// resolves the local binding, not the global one. Per syntax.md
// §6 "A local `let f = some_fn` shadows a top-level `fn` of the
// same name."
use std.test as test

fn f(x: i64): i64 { x * 2 }

fn make(): fn(i64): i64 {
    // Inside `make`, `f` is rebound to a closure that ignores the
    // top-level `f`. The returned closure captures the local `f`
    // and calls it.
    let f = fn(x: i64): i64 { x + 100 }
    fn(y: i64): i64 { f(y) }
}

let g = make()
// Local closure is reached — not the top-level `* 2`.
test.expect(g(5), 105)
test.expect(g(0), 100)

// The top-level `f` is untouched at module scope.
test.expect(f(5), 10)
test.expect(f(0), 0)
