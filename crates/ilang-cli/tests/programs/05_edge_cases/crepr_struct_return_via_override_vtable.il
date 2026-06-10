// 親クラスの CRepr struct return メソッドを子クラスが @override し、
// 親型変数経由で呼ぶ (= virt_call で dispatch される) ことを pin。
// 子の実装が呼ばれる + sret 経路が vtable slot にまたがって一貫する +
// liveAllocCount がフラットなことを 50 round で確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

class Base {
    init() {}
    pub make(n: i32): Box {
        let b = new Box()
        b.seq = n
        b
    }
}

class Child: Base {
    init() { super() }
    pub override make(n: i32): Box {
        let b = new Box()
        b.seq = n * 10
        b
    }
}

let parent: Base = new Child()
let b = parent.make(5)
test.expect(b.seq as i64, 50)

let base_alloc = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = parent.make(i as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base_alloc < 8)
