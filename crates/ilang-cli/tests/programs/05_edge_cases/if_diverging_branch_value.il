// An `if` used in value position where ONE branch diverges (`return`
// / `break` / `todo()` / a nested all-paths-diverging if) must take
// the join type from the live branch alone, and the diverging branch
// must not jump to the join with a `()` placeholder. `lower_if` picked
// the join type from both tails and emitted both branch jumps without
// consulting divergence, so the live branch mismatched the diverging
// branch's unit arg and MIR codegen aborted ("mismatched argument
// count for jump") — even the plain `let x = if c { 9 } else { return
// 1 }`. This is the if-expression analog of the int/bool/enum/string
// match value-join handling.
//
// expect: 109
// expect: i64-else-return=1
// expect: then-return=107
// expect: heap=1
// expect: heap-else=2
// expect: churn deinits=100

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// else branch diverges, then branch yields a value
fn elseReturns(c: bool): i64 {
    let x = if c { 9 } else { return 1 }
    x + 100
}

// then branch diverges, else branch yields a value
fn thenReturns(c: bool): i64 {
    let x = if c { return 1 } else { 7 }
    x + 100
}

// heap value join with a diverging else branch
fn heapElseReturns(c: bool): Box {
    let b: Box = if c { new Box(1) } else { return new Box(2) }
    b
}

console.log(elseReturns(true).toString())                       // 9 + 100
console.log("i64-else-return=" + elseReturns(false).toString()) // diverging else → return 1
console.log("then-return=" + thenReturns(false).toString())     // live else → 7 + 100
console.log("heap=" + heapElseReturns(true).n.toString())       // 1 (live then)
console.log("heap-else=" + heapElseReturns(false).n.toString()) // 2 (diverging else reached)

// Churn the diverging (reached) path: each call mints one Box and
// returns it, the temporary is dropped each iteration — deinit exactly
// 100, no leak / over-release.
let d0 = deinits[0]
let i = 0
while i < 100 { let _b = heapElseReturns(false); i = i + 1 }
console.log("churn deinits=" + (deinits[0] - d0).toString())
