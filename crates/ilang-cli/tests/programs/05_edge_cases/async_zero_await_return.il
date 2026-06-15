// A zero-await `async fn` whose body exits via `return` (not a tail
// expression). The desugar wraps the body's value in `Promise.resolve`,
// but it used to wrap ONLY the tail — leaving `return X` statements
// handing back a bare `X` that mismatched the wrapped `Promise<T>` return
// type. `async fn one(): i64 { return 1 }` failed to type-check.
//
// The fix wraps every `return X` too (and a divergent tail isn't
// double-wrapped, so a generic `if c { Result.ok(..) } else {
// Result.err(..) }` value tail keeps its single join-level wrap).
//
// expect: bare=7
// expect: branch=neg
// expect: branch=zero
// expect: branch=pos
// expect: stmt=42

async fn bare(): i64 {
    return 7
}

async fn classify(n: i64): string {
    if n < 0 { return "neg" }
    match n {
        0 { return "zero" }
        _ { return "pos" }
    }
}

async fn viaStmt(): i64 {
    let x = 40
    let y = 2
    return x + y
}

// Bound to `let _` so the program's tail isn't a Promise (which would
// auto-print `<promise pending>`).
let _ = bare().then(fn(v: i64) { console.log(`bare=${v}`) })
let _ = classify(-1).then(fn(s: string) { console.log(`branch=${s}`) })
let _ = classify(0).then(fn(s: string) { console.log(`branch=${s}`) })
let _ = classify(5).then(fn(s: string) { console.log(`branch=${s}`) })
let _ = viaStmt().then(fn(v: i64) { console.log(`stmt=${v}`) })
