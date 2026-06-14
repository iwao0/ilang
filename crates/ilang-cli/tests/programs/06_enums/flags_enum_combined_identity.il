// A `@flags` combined value (`read | write`, `~read`, a masked
// `f & X`, an `int as EnumName` cast) is produced by `$enum.box`.
// Pre-fix that boxer allocated a FRESH rc-less cell every call, so a
// combined value was NOT pointer-identical to a bit-equal sibling.
// Enum equality and Set / Map keying both ride on boxed-pointer
// identity, so three things broke at once:
//
//   * `(read | write) == (read | write)` compared two distinct
//     pointers and read FALSE — even though equality is defined over
//     the discriminant tag (docs/syntax.md). `!=` was inverted to
//     true. A masked compare `(f & write) == write` failed the same
//     way (box on the left, cached named variant on the right).
//   * a `Set<P>` / `Map<P, V>` keyed two bit-identical combined
//     values as DISTINCT elements (`Int` store keys on the raw cell,
//     i.e. the pointer), so `.size()` over-counted and `.has` /
//     lookup missed.
//   * each fresh box was rc=-1 and never freed — a per-combined-value
//     leak in any hot loop.
//
// Fix: `$enum.box` now routes through the same process-wide
// `(global_eid, disc)` intern cache as `EnumName.variant` literals
// (codegen reads the result enum type, remaps the local id through
// `enum_global`, and calls `__enum_unit_get`). A combined value is
// now the SAME canonical pointer as the equivalent named variant and
// as any other box of the same bits — so `read | read` equals `read`,
// equality / Set / Map all agree, and the allocation is bounded to
// one cell per distinct value.

// expect: flags combined identity ok

use std.test as test

@flags
enum P: u32 {
    read  = 1
    write = 2
    exec  = 4
}

// --- equality over combined values ---
let a = P.read | P.write
let b = P.read | P.write
test.expectTrue(a == b)              // same bits -> equal
test.expectTrue(!(a != b))
test.expectTrue((P.read | P.exec) != a)   // different bits -> not equal

// `read | read` (= read) must equal the named `read`: box vs cached
// named variant resolve to the same canonical pointer.
test.expectTrue((P.read | P.read) == P.read)

// masked compare — box on the left, cached named variant on the right.
let full = P.read | P.write | P.exec
test.expectTrue((full & P.write) == P.write)
test.expectTrue((full & (P.read | P.write)) == (P.read | P.write))
test.expectTrue(!((full & P.read) == P.write))

// `~` round-trips into an equatable value.
test.expectTrue((~P.read) == (~P.read))

// --- Set / Map dedup keys on the tag, not the pointer ---
let s = new Set<P>()
s.add(P.read | P.write)              // 3
s.add(P.read)                        // 1
s.add(P.read | P.write)              // 3 again -> collapses
test.expect(s.size(), 2)
test.expectTrue(s.has(P.read | P.write))
test.expectTrue(s.has(P.read))
test.expectTrue(!s.has(P.exec))

let m: Map<P, string> = {}
m[P.read | P.write] = "rw"
m[P.exec] = "x"
m[P.read | P.write] = "rw2"          // same key overwrites
test.expect(m.size(), 2)
let got = match m.get(P.read | P.write) {
    some(v) { v }
    none { "?" }
}
test.expectStr(got, "rw2")

// --- no per-box leak: churning a combined value interns one cell ---
// `acc` is allocated BEFORE the measurement window so its own bytes
// don't ride on the delta.
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + ((P.read | P.write | P.exec) as u32 as i64)
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)

console.log("flags combined identity ok")
