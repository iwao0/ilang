// `match` の結果をそのまま関数引数として渡すパターン。
// caller-releases-fresh と PatternBinding tail-Var Retain の
// 連携を確認: arm の some(v) { v } で v は borrowed (PatternBinding
// で needs_retain_on_tail = true → Retain で +1 されて caller に
// 渡る)。 callee は受け取った v をそのまま捨てる (caller-side で
// 解放) ことを期待。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

fn make_opt(): Box? { some(new Box(11)) }

fn take(_b: Box) {}

fn round() {
    take(match make_opt() {
        some(v) { v }
        none    { new Box(0) }
    })
}

round()
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    round()
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
