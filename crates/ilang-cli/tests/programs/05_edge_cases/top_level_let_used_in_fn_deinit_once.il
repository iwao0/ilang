// A top-level `let` whose value is referenced from a named fn body
// (so the loader promotes it to a host slot) must still run its
// `deinit` exactly once at program exit. Every read inside the fn
// is a borrow of the slot, not a fresh +1 — the slot itself is
// what owns the rc.
//
// Pre-fix every slot-fallback `Var` read inside `lookup_var`
// emitted a redundant `Inst::Retain`, so calling the fn N times
// left the rc at roughly `N + initial`. The exit-time slot release
// could only knock it down by 1, leaving rc > 0 → the array
// never dropped, its `T(42)` element's deinit never fired, and
// long-running game loops (breakout's per-frame
// `consts.*`-style reads) leaked silently per iteration.
//
//
// expect: --- start ---
// expect: --- end ---
// expect: deinit 42

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

// Class-typed top-level lets currently can't be referenced from
// named fn bodies (typechecker limitation), so we wrap T in a
// heap container. `g` is `T[]`, the slot-promotion + slot-read
// path is the same.
let g: T[] = [new T(42)]

fn read(): i64 { g[0].n }

console.log("--- start ---")
// Multiple reads through the slot — each one previously
// over-retained `g`. The exact count doesn't matter for
// correctness; we just need ≥1 to exercise the slot-load path.
let _ = read()
let _ = read()
let _ = read()
let _ = read()
let _ = read()
console.log("--- end ---")
// Program exit — the slot release must drive `g`'s rc to 0 so the
// array drops, cascading deinit on T(42). The fixture passes only
// when the trace contains exactly one "deinit 42" line.
