// A `loop` with NO `break` never falls through to a value — it exits
// only via `return` (or runs forever). It must type as the fn's return
// type (divergent), not `()`. Without this,
// `fn f(): i64 { loop { ...; return v } }` was rejected as
// "body produces ()". Mirrors the `return` / all-arms-return-match
// divergence rule, and matches Rust's `fn f() -> i32 { loop {} }`.
//
// expect: 2
// expect: 30
// expect: 7

// loop tail that exits only via `return`
fn firstEven(): i64 {
    let i = 1
    loop {
        if i % 2 == 0 { return i }
        i = i + 1
    }
}

// `return loop { ... }` where the loop itself breaks with a value still
// works (the loop is non-divergent there).
fn firstMul(): i64 {
    let i = 1
    return loop {
        if i % 5 == 0 { break i * 6 }
        i = i + 1
    }
}

// A break-less loop nested under other statements, returning early.
fn pick(target: i64): i64 {
    let acc = 0
    loop {
        acc = acc + 1
        if acc == target { return acc + 4 }
    }
}

console.log(firstEven().toString())
console.log(firstMul().toString())
console.log(pick(3).toString())
