// `@flags` 属性付き enum を CRepr struct の field として持つ。
// `@flags` enum は OR-combined bit pattern も valid 値として扱う
// (codegen が `__enum_unit_get` を呼ぶ、 checked 版ではなく)。
// LoadField codegen で `layout.is_flags` 分岐が正しく動くことを pin。

use std.test as test

@flags
pub enum Channel: u32 {
    left  = 0x01
    right = 0x02
    sub   = 0x04
    mid   = 0x08
}

@extern(C) {
    pub struct Mix {
        mask: Channel
    }
}

let m = new Mix()
m.mask = Channel.left
test.expect((m.mask as u32) as i64, 0x01)

// 単一 variant の書き換え。
m.mask = Channel.right
test.expect((m.mask as u32) as i64, 0x02)
