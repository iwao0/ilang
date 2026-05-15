// Tail-position `match` with awaits in arms — including an arm
// that binds a payload AND awaits inside its body. The v2
// MatchT terminator emits a per-arm transition; bindings are
// stored as `Type::I64` placeholders in the destination variant's
// payload (matching legacy's pattern-binding policy: ABI-safe for
// pointer/numeric values).
//
// expect: 42
// expect: 117
// expect: 7

async fn pick(o: i64?, p: Promise<i64>): i64 {
    match o {
        some(v) {
            let n = await p
            v + n
        }
        none {
            let m = await p
            m + 100
        }
    }
}

async fn run(): i64 {
    let a: i64 = await pick(some(35), Promise.resolve(7))
    let b: i64 = await pick(none,     Promise.resolve(17))
    let c: i64 = await pick(some(0),  Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    console.log(c.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
