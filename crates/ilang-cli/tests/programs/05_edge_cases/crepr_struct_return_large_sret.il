// 80 byte の CRepr struct を internal fn で return。 chunks 上限
// (IL_BYVAL_CHUNK_MAX = 64) を超えるので、 sret 強制以前から sret
// 経路に乗っている path。 caller pre-alloc → callee memcpy → free
// が 50 round 回しても 1 alloc / 1 free のペアで揃うことを pin。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct BigBox {
        a: i64
        b: i64
        c: i64
        d: i64
        e: i64
        f: i64
        g: i64
        h: i64
        i: i64
        j: i64
    }
}

fn make_big(n: i64): BigBox {
    let bb = new BigBox()
    bb.a = n
    bb.j = n * 10
    bb
}

let bb = make_big(7)
test.expect(bb.a, 7)
test.expect(bb.j, 70)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = make_big(i)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
