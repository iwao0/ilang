// `&path` with a field chain: `&obj.field`, `&obj.f1.f2`. The MIR
// lowerer emits `UseLocal + LoadField* + AddrOfField`. For ARC
// class fields, `LoadField` returns the inner class pointer; for
// CRepr inline fields, it returns the inline base; either way the
// pointer is the right base for the next hop. `AddrOfField` adds
// the field's offset (ARC: `OBJECT_HEADER_BYTES + idx*8`; CRepr:
// `c_field_offsets[idx]`).
//
// jit: skip
use std.test

class Pt {
    pub x: i32
    pub y: i32
    pub init() { this.x = 0; this.y = 0 }
}

class Box {
    pub topLeft: Pt
    pub bottomRight: Pt
    pub init() {
        this.topLeft = new Pt()
        this.bottomRight = new Pt()
    }
}

@extern(C) {
    fn set_via_ptr(out: *i32, value: i32)

    fn fill_pt(p: Pt, vx: i32, vy: i32) {
        set_via_ptr(&p.x, vx)
        set_via_ptr(&p.y, vy)
    }

    fn fill_box_corners(b: Box, x1: i32, y1: i32, x2: i32, y2: i32) {
        // Nested chain: write through `&b.topLeft.x` etc. The MIR
        // emits LoadField(b, topLeft) to get the Pt pointer, then
        // AddrOfField for `x` on Pt.
        set_via_ptr(&b.topLeft.x, x1)
        set_via_ptr(&b.topLeft.y, y1)
        set_via_ptr(&b.bottomRight.x, x2)
        set_via_ptr(&b.bottomRight.y, y2)
    }
}

let p = new Pt()
fill_pt(p, 3 as i32, 5 as i32)
test.expect(p.x as i64, 3)
test.expect(p.y as i64, 5)

let b = new Box()
fill_box_corners(b, 10 as i32, 20 as i32, 30 as i32, 40 as i32)
test.expect(b.topLeft.x as i64, 10)
test.expect(b.topLeft.y as i64, 20)
test.expect(b.bottomRight.x as i64, 30)
test.expect(b.bottomRight.y as i64, 40)
