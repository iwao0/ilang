// Promise.resolve, .then chaining with value transformation,
// .then's running in registration order off the work-stealing
// pool. Single chain so the fixture's expected output is
// deterministic (multiple chain roots race on the pool).
//
// expect: hello, ilang
// expect: 42

let _p = Promise.resolve("ilang")
    .then(fn(name: string): string {
        let s = "hello, " + name
        console.log(s)
        ""
    })
    .then(fn(_s: string): i64 {
        let n = 21 * 2
        console.log(n.toString())
        n
    })

// Suppress the CLI's auto-print of the trailing top-level expression.
let _ = 0
