// 80 byte の CRepr struct を by-value param で渡す。 chunks 上限
// 超えなので caller 側 `calls.rs::struct_byval_size_with_max` の
// memcpy scratch 経路に乗る (Cranelift の `StructArgument` purpose
// が AArch64 では未実装なので手で scratch StackSlot に memcpy する
// path)。 callee 内で field 読み + struct mutation が正しく動く
// + caller の値が変わらない (= by-value セマンティクス) を pin。
// 50 round 回して leak ないことも確認。
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

fn sum_first_and_last(bb: BigBox): i64 {
    bb.a + bb.j
}

fn mutate_a(bb: BigBox): i64 {
    // by-value なので callee 側の bb.a 書き換えは caller に伝わらない
    bb.a = 999
    bb.a
}

let bb = new BigBox()
bb.a = 3
bb.j = 11

test.expect(sum_first_and_last(bb), 14)
test.expect(mutate_a(bb), 999)
// by-value 性確認: caller 側の bb.a は 3 のまま
test.expect(bb.a, 3)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = sum_first_and_last(bb)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
