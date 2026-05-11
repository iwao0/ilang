// Per-iteration enum value with a payload, then `match` extracts
// the inner. The match arm's binding must release the cell back
// down the cascade chain (a heap payload is retained on extract,
// then released when the binding drops; the cell itself drops
// when the scrutinee binding's last user releases it). Mixes
// payload variants AND the unit variant in one switch so the
// release path covers both.
//
// `leak_enum_payload_loop.il` covers the construct-then-discard
// shape; this fixture adds the match-binding extraction shape.
//

use test

enum E {
    a: (i64)
    b: (string)
    c
}

let baseline_b = test.liveAllocBytes()
let baseline_s = test.liveStringCount()

let i: i64 = 0
while i < 300 {
    let e: E = if i % 3 == 0 {
        E.a(i)
    } elif i % 3 == 1 {
        E.b("payload")
    } else {
        E.c
    }
    let v = match e {
        a(n) { n }
        b(_) { 0 }
        c { -1 }
    }
    let _ = v
    i = i + 1
}

let after_b = test.liveAllocBytes()
let after_s = test.liveStringCount()
test.expectTrue(after_b - baseline_b < 65536)
test.expectTrue(after_s - baseline_s < 50)
