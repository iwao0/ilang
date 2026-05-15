// `async fn` with `match` at body tail position, awaits inside
// arm bodies. The BlockBuilder emits a MatchDispatch terminator
// whose pattern arms save any pattern bindings into state fields
// before jumping to the target state.
//
// Two variants exercised:
//   - enum dispatch with `await` inside one arm
//   - Optional `i64?` with `some(v)` binding capture
//
// expect: 42
// expect: 100
// expect: 0
// expect: 42
// expect: 105

enum Color { red, green, blue }

async fn dispatch(c: Color, p: Promise<i64>): i64 {
    match c {
        red {
            let v = await p
            v * 2
        }
        green { 100 }
        blue { 0 }
    }
}

async fn handleAsync(o: i64?, fallback: Promise<i64>): i64 {
    match o {
        some(v) { v * 2 }
        none {
            let f = await fallback
            f + 100
        }
    }
}

async fn run(): i64 {
    let a = await dispatch(Color.red, Promise.resolve(21))
    let b = await dispatch(Color.green, Promise.resolve(0))
    let c = await dispatch(Color.blue, Promise.resolve(0))
    let d = await handleAsync(some(21), Promise.resolve(0))
    let e = await handleAsync(none, Promise.resolve(5))
    console.log(a.toString())
    console.log(b.toString())
    console.log(c.toString())
    console.log(d.toString())
    console.log(e.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
