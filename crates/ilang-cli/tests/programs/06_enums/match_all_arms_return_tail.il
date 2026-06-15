// A `match` whose every arm diverges with `return`, sitting as the
// function tail, must type-check — exactly like `if { return } else
// { return }` does. The arms contribute no value to the join, so the
// match itself diverges and adopts the function's return type instead
// of `()`. Regression: previously the checker reported
// `function ... declared to return i64 but body produces ()`.
//
// Covers both the enum-match and Optional-match paths.
//
// expect: 1
// expect: 2
// expect: 7
// expect: -1

enum E { x, y }

fn fromEnum(e: E): i64 {
    match e {
        x { return 1 }
        y { return 2 }
    }
}

fn fromOpt(o: i64?): i64 {
    match o {
        some(v) { return v }
        none { return -1 }
    }
}

console.log(fromEnum(E.x).toString())
console.log(fromEnum(E.y).toString())
console.log(fromOpt(some(7)).toString())
console.log(fromOpt(none).toString())
