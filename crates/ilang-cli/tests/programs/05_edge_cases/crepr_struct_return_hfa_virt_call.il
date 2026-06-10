// 全 float の CRepr struct を class method 戻り値にして virt_call で
// 呼ぶ。 internal-fn 規約では HFA 経路を skip して sret 強制 (本
// 修正の趣旨) なので、 caller (VirtCall) と callee の signature が
// HFA-eligible なサイズでも合うかを pin。 HFA path がうっかり生きて
// いると caller は float-reg return を期待し、 callee は sret に
// 書く → 値が garbage で test.expectF64 が落ちる。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Pt {
        x: f32
        y: f32
    }
}

class Origin {
    init() {}
    pub at(dx: f32, dy: f32): Pt {
        let p = new Pt()
        p.x = dx
        p.y = dy
        p
    }
}

let o = new Origin()
let p = o.at(3.5_f32, 4.5_f32)
test.expectF64(p.x as f64, 3.5)
test.expectF64(p.y as f64, 4.5)

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = o.at(i as f32, i as f32)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
