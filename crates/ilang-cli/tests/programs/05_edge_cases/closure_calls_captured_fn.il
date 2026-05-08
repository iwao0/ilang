// A closure body can call a captured fn-typed local by bare name.
// Pre-fix both the interp's free-variable scan and the JIT's Call
// arm only ever looked at top-level fns / `let` bindings — they
// never considered the active closure's captures, so
// `compose(f, g) { fn(x) { f(g(x)) } }` errored
// `undefined function "g"` (interp) or `unknown function "f"`
// (JIT) at the inner call.
//
// The interp now adds the callee name to the closure's free-var
// set (and the FnExpr capture step filters out top-level fn names
// automatically since they don't live in `vars`/`globals`). The
// JIT mirrors `Var`'s capture lookup in the Call arm so a fn-typed
// capture lowers to an indirect closure call.
//
// expect: 12
// expect: 30
// expect: 17

fn compose(f: fn(i64): i64, g: fn(i64): i64): fn(i64): i64 {
    fn(x: i64): i64 { f(g(x)) }
}

let plus1 = fn(n: i64): i64 { n + 1 }
let times2 = fn(n: i64): i64 { n * 2 }

let p1t2 = compose(times2, plus1)
console.log(p1t2(5))     // (5+1)*2 = 12

fn twice(f: fn(i64): i64, x: i64): i64 {
    f(f(x))
}
let add10 = fn(n: i64): i64 { n + 10 }
console.log(twice(add10, 10))   // 30

// Captured fn calls captured fn (two captures, both invoked).
fn pipeline(a: fn(i64): i64, b: fn(i64): i64, c: fn(i64): i64): fn(i64): i64 {
    fn(x: i64): i64 { c(b(a(x))) }
}
let p = pipeline(plus1, times2, plus1)
console.log(p(7))        // ((7+1)*2)+1 = 17
