// generic identity fn を CRepr struct で monomorphize する。
// `fn id<T>(x: T): T { x }` が `T = Box` で展開されたとき、
// 引数 (by-value CRepr param) を tail で素通しする path が
// caller の sret pre-alloc と整合するか、 buffer ownership が
// 正しく caller に渡るかを pin。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

fn id<T>(x: T): T { x }

fn make_box(n: i32): Box {
    let b = new Box()
    b.seq = n
    b
}

let b = id(make_box(7))
test.expect(b.seq as i64, 7)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = id(make_box(i as i32))
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
