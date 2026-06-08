// Array / Tuple / Optional literal element retain must cover
// every kind whose runtime cascade releases the slot. Pre-fix
// the five `is_heap` lists in `lower/literals.rs` (two array
// literal paths, two tuple literal paths, plus the `some(...)`
// inner) only matched Object / Array / Tuple / Map / Optional /
// Fn / Str — Enum / Set / Promise / Weak were silently missing.
//
// The shape was symmetric with the runtime cascade for Map (the
// host helper retains internally, so map literals are unaffected),
// but Array / Tuple / Optional all left non-fresh enum / set /
// weak / promise elements at the source's rc=1 share. When the
// literal's container dropped its cascade decremented the slot
// to rc=0 and freed it; the source binding's scope-exit release
// then tried to release a freed pointer → SIGABRT.
//
// Minimal repro before the fix:
//   enum E { v: (i64) }
//   let e = E.v(7)
//   let t = (e, 1)              // exit 134 (SIGABRT)
// or
//   let xs: E[] = [e]           // exit 139 (SIGSEGV)
//
// Each round here builds the literal AND keeps the source
// binding alive so the double-release race fires; the
// `liveAllocCount` guard catches the leak in the rare case
// where the bad free silently succeeds instead of trapping.

use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

enum E {
    v: (i64)
}

fn round_array_enum() {
    let e = E.v(7)
    let xs: E[] = [e]
    let _ = xs
    let _ = e
}

fn round_tuple_enum() {
    let e = E.v(7)
    let t = (e, 1)
    let _ = t
    let _ = e
}

fn round_optional_enum() {
    let e = E.v(7)
    let o: E? = some(e)
    let _ = o
    let _ = e
}

fn round_array_set() {
    let s = new Set<Tag>()
    s.add(new Tag(1))
    let xs: Set<Tag>[] = [s]
    let _ = xs
    let _ = s
}

fn round_tuple_set() {
    let s = new Set<Tag>()
    s.add(new Tag(2))
    let t = (s, 0)
    let _ = t
    let _ = s
}

round_array_enum()
round_tuple_enum()
round_optional_enum()
round_array_set()
round_tuple_set()

let baseline = test.liveAllocCount()
for _ in 0..50 {
    round_array_enum()
    round_tuple_enum()
    round_optional_enum()
    round_array_set()
    round_tuple_set()
}
test.expect(test.liveAllocCount() - baseline, 0)
