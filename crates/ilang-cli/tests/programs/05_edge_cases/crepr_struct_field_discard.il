// fn の戻り値で CRepr struct を返し、 caller 側で `let _` で捨てる。
// caller-releases-fresh が `__release_object` を呼ばずに済むことを
// pin (CRepr Object は is_arc_slot=false のため heap release 対象外)。
//
// 50 round で `liveAllocCount` が増えないことで確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Box {
        kind: Mode
        seq:  i32
    }
}

fn make_box(n: i32): Box {
    let b = new Box()
    b.kind = Mode.active
    b.seq = n
    b
}

let _ = make_box(0)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = make_box(i as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
