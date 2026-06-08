// CRepr struct (`@extern(C)` block 内) の unit-only enum field を
// `field = Enum.variant` で書き込み、 同じ field を読み戻して値が
// 一致することを確認。
//
// pre-fix (df51064a): `ExprKind::AssignField` の `is_heap` 判定に
// `MirTy::Enum(_)` を追加した結果、 CRepr 親の inline enum field
// (d65fec00 で repr 幅 layout 済み) に対しても retain / release が
// 走り、 `LoadField` + `__release_enum` が inline integer 値
// (例: 0x8010) を heap pointer として release してメモリが破壊された。
// `examples/sdl_breakout/main.il` が起動直後に
// `read CRepr struct field of enum sdl2.AudioFormat with unknown
// discriminant 0` で abort していた事象を最小化したテスト。
//
// fix: `expr.rs::AssignField` で親 class が CRepr / CPacked /
// CUnion なら Enum field を heap 扱いから除外し、 inline integer の
// raw store だけを発行する。

use std.test as test

pub enum Color: u16 {
    red   = 0x0010
    green = 0x0020
    blue  = 0x0040
}

@extern(C) {
    pub struct Pixel {
        kind: Color
        other: i32
    }
}

let p = new Pixel()
p.kind = Color.green
p.other = 7
test.expect((p.kind as u16) as i64, 0x20)
test.expect(p.other as i64, 7)

// 同じ field に別 variant を書き戻し、 raw store だけで cleanly
// 更新できることを確認。
p.kind = Color.blue
test.expect((p.kind as u16) as i64, 0x40)
