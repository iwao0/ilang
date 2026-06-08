// Reassigning a let-bound variable from outer scope after a
// closure has captured it pinning the cell. The capture forces
// the binding into `Binding::Cell` (the cell shared between the
// outer scope and the closure), so the outer-scope reassignment
// goes through `BodyCx::assign_var`'s Cell branch.
//
// Pre-fix `assign_var`'s Cell branch did its own
// release-old / retain-new unconditionally, AND
// `ExprKind::Assign`'s outer cascade — after the rc-slot
// widening landed — did the same release/retain in the caller.
// For Cell bindings this double-accounted: the prior value was
// released twice (UAF on the second), and a borrowed rhs was
// retained twice (leak). The contract is now caller-delegated:
// `assign_var` only stores, and the Assign cascade issues the
// single retain / release pair, with `value_is_fresh` so a
// fresh rhs doesn't leak its +1.
//
// Probe: outer scope owns `xs: Tag[]`, a closure captures `xs`
// (forcing the cell), then outer reassigns `xs = [...]` in a
// loop. With the double-Release bug the second free would crash
// or scribble the alloc list; with double-Retain on borrowed
// rhs the prior array's Tag elements would leak.
//
// mir-jit only — liveAllocCount isn't wired into AOT.
// aot: skip
use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

fn round() {
    let xs: Tag[] = [new Tag(0)]
    // Pin the cell: the closure captures `xs`, forcing it from
    // a plain Local into a Cell binding shared with the outer
    // scope.
    let read = fn(): i64 {
        let s = xs
        s[0].n
    }
    let i: i64 = 0
    while i < 20 {
        xs = [new Tag(i), new Tag(i + 1)]
        i = i + 1
    }
    let _ = read()
}

round()
let base = test.liveAllocCount()
for _ in 0..50 { round() }
test.expect(test.liveAllocCount() - base, 0)
