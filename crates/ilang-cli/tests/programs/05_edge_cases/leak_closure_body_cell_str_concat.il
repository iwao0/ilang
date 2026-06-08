// Reassigning a captured `string` cell from INSIDE the closure
// body, then returning the cell via a `Var` tail. Two paths
// have to agree:
//
//   - `ExprKind::Assign` → `captures_in_scope` cell-store
//     transfers a fresh rhs into the cell as the cell's +1
//     share (no second Retain). Pre-fix the unconditional
//     Retain left rc=2 on every reassignment with the second
//     share orphaned.
//
//   - `lower_block_hinted` tail Retain treats captured cells
//     the same as `Binding::Cell` — without this, a closure
//     body that returns the captured cell (`log = log + "x";
//     log`) would hand the cell's only share back to the
//     caller; the caller would release it on discard and the
//     cell would dangle on the next call.
//
// Probe: `f()` reassigns `log` with the result of a Binary
// str concat (`is_fresh_object_expr` flags it as fresh) and
// returns `log`. Without either fix, `liveStringCount` either
// grows unboundedly (leak) or the second call crashes on a
// freed cstring.
//
// mir-jit only — string registry isn't AOT-wired.
// aot: skip
use std.test as test

fn make(): fn(): string {
    let log = ""
    fn(): string {
        log = log + "x"
        log
    }
}

let f = make()
// Warm up — the very first `log = "" + "x"` may bump the
// per-process intern count by one for the static `""` slot.
let _ = f()
let _ = f()

let base = test.liveStringCount()
let i: i64 = 0
while i < 200 {
    let _ = f()
    i = i + 1
}
// liveStringCount grows by at most a small bounded amount per
// call (the new `log` string), but the prior `log` must drop
// each round; with the leak, it grows by ~200.
test.expectTrue(test.liveStringCount() - base < 32)
