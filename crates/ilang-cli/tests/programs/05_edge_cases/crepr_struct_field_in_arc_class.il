// ARC 親 class の field に CRepr struct を持たせる: 親 class は
// 通常の heap object として retain/release されるが、 内側の
// CRepr struct field は inline 配置。 内側 struct の `Mode`
// (CReprEnum) field を read / write した結果が ARC 親の lifetime
// と独立して正しく動くことを確認。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Inner {
        kind: Mode
    }
}

pub class Holder {
    pub inner: Inner
    init() {
        this.inner = new Inner()
        this.inner.kind = Mode.idle
    }
}

let h = new Holder()
test.expect((h.inner.kind as u16) as i64, 0x01)

h.inner.kind = Mode.active
test.expect((h.inner.kind as u16) as i64, 0x02)
