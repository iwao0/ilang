// while ループ内で CRepr struct を `new` し、 一定回数で `break`
// する。 scope-exit に到達せず break で抜ける path で、 owned CRepr
// local が `release_top_scope_objects` 経由で正しく free されるか
// を pin。 break で抜ける round と通常 round を 50 round 混在させ
// て leak がないことを確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

fn run_one(limit: i32): i32 {
    let i: i32 = 0
    let last: i32 = 0
    while i < 100 {
        let b = new Box()
        b.seq = i
        last = b.seq
        if i == limit {
            break
        }
        i = i + 1
    }
    last
}

test.expect(run_one(5) as i64, 5)
test.expect(run_one(0) as i64, 0)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = run_one((i % 10) as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
