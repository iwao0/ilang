// A `pub enum E: T` can hold a value cast in from an out-of-range int
// (`n as E`). A `match` that covers every declared variant is exhaustive
// to the checker, so it has no wildcard — but at runtime that cast-in
// value matches no arm. The synthesised default used a
// `Terminator::Unreachable`, which codegens to an illegal-instruction trap
// (SIGILL), not a graceful failure.
//
// Now the no-wildcard default emits a clean runtime panic. (For a non-repr
// enum the default is genuinely unreachable, so this is dead code there.)
// The idiomatic safe form — a `_` wildcard, as in repr_enum_patterns.il —
// is unaffected.

// expect-error: no matching enum variant

enum Msg: u32 {
    quit = 16
    close = 18
}

fn classify(raw: u32): i64 {
    match raw as Msg {
        quit { 100 }
        close { 200 }
    }
}

console.log("before")
console.log(classify(99).toString())     // 99 is no declared variant -> panic
