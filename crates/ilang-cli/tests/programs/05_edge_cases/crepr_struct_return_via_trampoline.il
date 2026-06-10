// CRepr struct return を trampoline closure 経由で呼んだとき、
// caller の sret pre-alloc → trampoline body → 本物の fn の direct
// call → trampoline body の sret memcpy という二段経路が ABI 整合を
// 保つことを pin。 trampoline は FunctionKind::Trampoline (is_c_abi
// == false) で force_internal_sret=true。 内部 fn 規約の sret 経路に
// 乗っていないと callee の sret 書き込みを caller が chunks として
// 読み、 NSRange virt_call と同型の SIGSEGV を踏む。
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

let f = make_box
let b = f(42)
test.expect((b.kind as u16) as i64, 0x02)
test.expect(b.seq as i64, 42)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = f(i as i32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
