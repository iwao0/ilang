// `async fn` with a `while` loop whose body contains awaits. The
// BlockBuilder emits:
//   - Block 0: pre-loop stmts, Jump to header
//   - Block 1 (header): Branch(cond, body_first, after)
//   - Block 2..k (body): per-state chunks ending in Suspend, last
//                        chunk Jumps back to header
//   - Block (after): tail expression Settle
//
// expect: 21

async fn collectAsync(p: Promise<i64>, n: i64): i64 {
    let i = 0
    let total = 0
    while i < n {
        let v = await p
        total = total + v
        i = i + 1
    }
    total
}

let _ = collectAsync(Promise.resolve(7), 3).then(fn(s: i64) {
    console.log(s.toString())
})

let _ = 0
