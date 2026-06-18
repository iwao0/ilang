// An integer or bool `match` arm that diverges (`return` / `break` /
// `todo()` / an all-paths-diverging if) must be skipped from the
// match's value join — like the enum and string match paths already
// do. `lower_match_int` / `lower_match_bool` pushed EVERY arm to the
// join unconditionally, so a diverging arm's dead block jumped to the
// join with a `()` placeholder and MIR codegen aborted with a
// Cranelift verifier error ("mismatched argument count for jump")
// instead of compiling. Even the plain `match s { 0 { 9 } _ { return
// 1 } }` crashed. (The string-match path was already correct.)
//
// expect: 9
// expect: i64-return=1
// expect: heap=9
// expect: bool=1
// expect: churn deinits=100

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// int match, scalar result, diverging wildcard arm
fn pick(s: i64): i64 {
    match s {
        0 { 9 }
        _ { return 1 }
    }
}

// int match, heap result, diverging wildcard arm
fn pickBox(s: i64): Box {
    match s {
        0 { new Box(9) }
        _ { return new Box(7) }
    }
}

// bool match, heap result, diverging arm
fn pickBool(b: bool): Box {
    match b {
        true { new Box(1) }
        false { return new Box(2) }
    }
}

console.log(pick(0).toString())                     // 9 (diverging arm not taken)
console.log("i64-return=" + pick(5).toString())     // 1 (diverging arm reached → return)
console.log("heap=" + pickBox(0).n.toString())      // 9
console.log("bool=" + pickBool(true).n.toString())  // 1

// Churn the diverging (reached) path: each call mints one Box and
// returns it, the temporary is dropped each iteration — deinit exactly
// 100, no leak / over-release.
let d0 = deinits[0]
let i = 0
while i < 100 { let _b = pickBox(5); i = i + 1 }
console.log("churn deinits=" + (deinits[0] - d0).toString())
