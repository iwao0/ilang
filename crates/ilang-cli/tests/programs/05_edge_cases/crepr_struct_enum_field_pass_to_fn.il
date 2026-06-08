// CRepr struct の inline enum field を fn 引数として渡し、
// caller-releases-fresh path が `__release_enum` を inline integer
// に対して呼ばないことを pin する。
//
// `pkt.kind` は `LoadField` で `MirTy::CReprEnum` から `MirTy::Enum`
// に昇格して codegen が `__enum_unit_get_checked` で heap-box pointer
// に変換するので、 fn 側は通常の heap enum cell として受け取る。
// caller-releases-fresh は SSA 値の MirTy (= Enum) を見るので
// `__release_enum` が呼ばれるが、 これは heap-box pointer に対する
// release で正しい挙動。
//
// pre-CReprEnum 時代 (df51064a 後) は AssignField でしか除外して
// いなかったので、 fn 引数経路は別途 papering over が必要だった。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Packet {
        kind: Mode
    }
}

fn describe(m: Mode): string {
    match m {
        idle   { "idle" }
        active { "active" }
    }
}

let pkt = new Packet()
pkt.kind = Mode.active

let label = describe(pkt.kind)
test.expectStr(label, "active")

// loop で繰り返しても enum-cell 漏れが発生しないことを
// liveAllocCount で確認 (heap-box pointer は intern キャッシュなので
// alloc されないことが期待値)。
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = describe(pkt.kind)
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
