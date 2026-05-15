// `Promise<UserEnum>` (non-generic) — payload variants flow back
// through await and match on the resolved value.
//
// Note: `Promise<Result<T, E>>` / `Promise<Box<T>>` (built-in or
// user-defined generic enum payload) is currently rejected by MIR
// monomorphize as "unsupported in M1: user-defined generic types".
// That's a pre-existing M1 gap unrelated to the v2 state-machine
// rewrite; this fixture sticks to a non-generic user enum.
//
// expect: R
// expect: G
// expect: B

enum Tag {
    red
    green
    blue
}

fn pick(n: i64): Promise<Tag> {
    if n == 0 {
        Promise.resolve(Tag.red)
    } elif n == 1 {
        Promise.resolve(Tag.green)
    } else {
        Promise.resolve(Tag.blue)
    }
}

async fn run(): i64 {
    let a: Tag = await pick(0)
    match a {
        red { console.log("R") }
        green { console.log("G") }
        blue { console.log("B") }
    }
    let b: Tag = await pick(1)
    match b {
        red { console.log("R") }
        green { console.log("G") }
        blue { console.log("B") }
    }
    let c: Tag = await pick(2)
    match c {
        red { console.log("R") }
        green { console.log("G") }
        blue { console.log("B") }
    }
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
