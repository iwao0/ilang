// ネストされた CRepr struct: 外側 struct の field に別 CRepr struct
// を持ち、 内側 struct の enum field を read / write する。
// `decl/extern_c.rs` の layout pass は内側 struct の field type
// (含む `CReprEnum`) を見て size/align を計算するので、 ネスト
// レイアウトでも inline enum slot が正しく配置される。
//
// `inner` field 自体は `MirTy::Object(inner_cid)` で、 retain/release
// は CRepr Object 除外 (`is_arc_slot`) で skip される。 内側 struct
// の enum field は `CReprEnum` の通常 path。

use std.test as test

pub enum Tone: u8 {
    soft = 0x01
    loud = 0x02
}

@extern(C) {
    pub struct Inner {
        tone: Tone
        gain: i32
    }
    pub struct Outer {
        inner: Inner
        seq:   i32
    }
}

let o = new Outer()
o.inner.tone = Tone.loud
o.inner.gain = 9
o.seq = 100

test.expect((o.inner.tone as u8) as i64, 0x02)
test.expect(o.inner.gain as i64, 9)
test.expect(o.seq as i64, 100)

// 別 variant に書き戻し、 inner / outer の他 field を破壊して
// いないことを確認 (layout のずれ検出)。
o.inner.tone = Tone.soft
test.expect((o.inner.tone as u8) as i64, 0x01)
test.expect(o.inner.gain as i64, 9)
test.expect(o.seq as i64, 100)
