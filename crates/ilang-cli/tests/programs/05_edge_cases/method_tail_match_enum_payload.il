// `match` arm whose tail is the pattern-bound payload variable of a
// tuple-payload enum variant. The payload binding looks like a bare
// `Var(b)` to lower; it must NOT be misidentified as an implicit
// `this.field` ref (the heuristic only fires when env / captures
// don't resolve the name, but a match-arm pattern *does* bind `b`
// in env). Verifies the borrow-tail retain doesn't fire for the
// payload-bound case.
use std.test as test

class Box { n: i64; init(x: i64) { this.n = x } }

enum Holder {
    boxed: (Box)
    empty
}

fn unwrap_box(h: Holder): Box {
    match h {
        boxed(b) { b }
        empty { new Box(0) }
    }
}

let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let inner = new Box(i + 1)
    let h = Holder.boxed(inner)
    let r = unwrap_box(h)
    test.expect(r.n, i + 1)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
