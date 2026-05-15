// `Promise<Box<T>>` — a user-defined generic enum inside Promise.
// Previously failed with "unsupported in M1: user-defined generic
// types" because monomorphize_enums skipped FnExpr closure param
// types when rewriting `Type::Generic` → mangled `Type::Object`.
//
// expect: hold: hi
// expect: empty

enum Box<T> {
    hold: (T)
    empty
}

fn wrap(s: string, fill: bool): Promise<Box<string>> {
    if fill {
        let b: Box<string> = Box.hold(s)
        Promise.resolve(b)
    } else {
        let b: Box<string> = Box.empty
        Promise.resolve(b)
    }
}

async fn run(): i64 {
    let a: Box<string> = await wrap("hi", true)
    match a {
        hold(s) { console.log("hold: " + s) }
        empty { console.log("empty") }
    }
    let b: Box<string> = await wrap("ignored", false)
    match b {
        hold(s) { console.log("hold: " + s) }
        empty { console.log("empty") }
    }
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
