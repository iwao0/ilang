// 匿名関数 (closure) の body の tail に CRepr struct の `new` を
// 置き、 closure 経由で discard を回す。 closure body の Return が
// sret 経路に乗り、 release_value=true で callee buffer が回収され
// るかを pin。 `Inst::CallIndirect` の sret 経路と、 closure 本体
// (Local kind) の Return の release_value 立ち上がりが両輪で動か
// ないと leak / SIGSEGV する。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

let make: fn(): Box = fn(): Box {
    let b = new Box()
    b.seq = 7
    b
}

let b = make()
test.expect(b.seq as i64, 7)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = make()
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
