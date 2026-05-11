// An enum with a mix of unit + (i64, i64) tuple-payload + (f64,
// f64) tuple-payload variants. Per loop iter, one of the three
// is constructed and immediately consumed by a match. Cell drop
// has to handle all three payload shapes — the unit cache, the
// 2×i64 payload (no heap cascade), and the 2×f64 payload (no
// heap cascade either, but distinct kind tag on the cell).
//

use test

enum Pt {
    origin
    cart: (i64, i64)
    polar: (f64, f64)
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 300 {
    let p = if i % 3 == 0 {
        Pt.origin
    } elif i % 3 == 1 {
        Pt.cart(i, i * 2)
    } else {
        Pt.polar(1.0, 2.0)
    }
    let v = match p {
        origin { 0 }
        cart(x, y) { x + y }
        polar(_, _) { 99 }
    }
    let _ = v
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
