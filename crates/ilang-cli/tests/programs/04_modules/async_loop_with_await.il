// `loop { ...await...; break }` — Rust-style infinite loop with
// awaits. v2 pre-pass rewrites to `while true { ... }`, which the
// state-machine builder handles via its standard while path.
//
// expect: 21

async fn collect_three(p: Promise<i64>): i64 {
    let total = 0
    let i = 0
    loop {
        let v = await p
        total = total + v
        i = i + 1
        if i >= 3 { break }
    }
    total
}

let _ = collect_three(Promise.resolve(7)).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
