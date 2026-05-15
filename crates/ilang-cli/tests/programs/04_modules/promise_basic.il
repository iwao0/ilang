// Promise<T> — Promise.resolve, .then chaining (with value
// transformation), .catch on a rejected executor, async value
// flow through the work-stealing pool. The runtime drains
// pending continuations at main exit so callbacks always fire
// before the process leaves.
//
// Output order is deterministic only within a single chain, so
// the fixture stitches everything onto one chain.
//
// expect: hello, ilang
// expect: 42
// expect: recovered: oops
// expect: done

Promise.resolve("ilang")
    .then(fn(name: string): string {
        let s = "hello, " + name
        console.log(s)
        // Switch to an i64 chain by constructing the next promise
        // explicitly inside the next then.
        ""
    })
    .then(fn(_s: string): i64 {
        let n = 21 * 2
        console.log(n.toString())
        n
    })
    .then(fn(_n: i64): string {
        // Trigger a rejection and recover.
        let _p = new Promise<string>(fn(_r: fn(string), reject: fn(string)) {
            reject("oops")
        }).catch(fn(msg: string): string {
            let s = "recovered: " + msg
            console.log(s)
            s
        }).then(fn(_s: string): string {
            console.log("done")
            ""
        })
        ""
    })

// Suppress the CLI's auto-print of the trailing top-level expression
// (a Promise pointer would otherwise hit stdout).
let _ = 0
