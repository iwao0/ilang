// CRepr struct を closure で capture して、 closure body 内で
// inline enum field を書き換える path。 captures_in_scope 経路の
// AssignField 判定 (838d2dc4 の lower_match_optional / lower_if_let
// 修正を経由した) が、 `is_arc_slot(CReprEnum) = false` を活かして
// retain/release を skip すべき。
//
// 直接的な leak は無いはず — CRepr struct 自体が is_arc_slot=false
// なので closure cell の rc 管理対象外。 capture cell の lifetime
// だけ確認する。

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

let b = new Box()
b.kind = Mode.idle
b.seq = 0

let flip = fn() {
    b.kind = Mode.active
    b.seq = b.seq + 1
}

flip()
test.expect((b.kind as u16) as i64, 0x02)
test.expect(b.seq as i64, 1)

flip()
test.expect(b.seq as i64, 2)
