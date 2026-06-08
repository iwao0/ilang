// `obj.field = match opt { some(v) { v } none { default } }` の
// 形。 RHS の match 結果が field-assign の retain 経路と
// PatternBinding の tail Retain の両者を通る。 二重 retain に
// ならないことを確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

class Holder {
    b: Box
    init(initial: Box) { this.b = initial }
}

fn make_opt(): Box? { some(new Box(21)) }

fn round() {
    let h = new Holder(new Box(0))
    h.b = match make_opt() {
        some(v) { v }
        none    { new Box(0) }
    }
    let _ = h
}

round()
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    round()
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
