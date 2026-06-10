// A self-recursive closure without a fn-type annotation can't be
// checked (the recursive type isn't inferrable). This used to
// surface as a bare "undefined function" — it's a targeted
// diagnostic now.
//
// expect-error: needs an explicit fn-type annotation
fn host(): i64 {
    let fib = fn(n: i64): i64 {
        if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
    }
    fib(10)
}
console.log(host())
