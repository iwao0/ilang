// 同じ block 内で複数の CRepr struct discard が連続。 各 discard
// で sret pre-alloc → call → __mir_free のペアが順序を保って揃う
// かを pin。 ペアがズレると 50 round で `liveAllocCount` が累積で
// 大きくなる。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

fn make_box(n: i32): Box {
    let b = new Box()
    b.seq = n
    b
}

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = make_box(i as i32)
    let _ = make_box((i + 100) as i32)
    let _ = make_box((i + 200) as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
