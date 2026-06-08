// 入れ子 `match`: outer の some arm が inner の `match` を
// scrutinee として持つ。 outer の some binding `v` (Box?) を
// inner の match に渡し、 inner の some binding `b` (Box) を
// tail で返す。 PatternBinding の `needs_retain_on_tail` が
// inner / outer それぞれの scrut_is_fresh を正しく追跡している
// かを確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

fn make_nested(): Box?? { some(some(new Box(13))) }

fn through_nested(): Box {
    match make_nested() {
        some(v) {
            match v {
                some(b) { b }
                none    { new Box(0) }
            }
        }
        none { new Box(0) }
    }
}

through_nested()
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = through_nested()
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
