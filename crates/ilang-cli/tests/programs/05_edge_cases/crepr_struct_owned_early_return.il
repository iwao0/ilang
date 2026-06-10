// fn 内で CRepr struct を `new` し、 条件付きで `return` する。
// scope-exit に到達せず early return する path で、 owned CRepr
// local が `release_top_scope_objects` 経由で正しく free される
// か、 tail return value の CRepr struct (= 戻り値) が caller に
// 正しく渡るかを pin。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

fn maybe_early(n: i32): i64 {
    let b = new Box()
    b.seq = n
    if n > 0 {
        return b.seq as i64
    }
    let c = new Box()
    c.seq = -1
    c.seq as i64
}

test.expect(maybe_early(7), 7)
test.expect(maybe_early(0), -1)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = maybe_early((i - 25) as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
