// `Set<string>` locals must be released at scope exit. Pre-fix the
// scope-exit release pass (`release_top_scope_objects`) omitted
// `MirTy::Set`, so a set bound with `let s = new Set<...>()` was never
// released — leaking the whole set plus every string element. With Set
// added to the release set (and codegen dropping the fresh element
// transient `add` adopts), building and dropping string sets is leak-
// free.
use std.test as test

// Build and drop string sets with fresh (heap-built) elements, no
// clear(). The set-drop must release every adopted element.
let baseline = test.liveStringCount()
let n = 0
while n < 200 {
    let s = new Set<string>()
    let i = 0
    while i < 8 {
        s.add("elem" + i.toString())
        i = i + 1
    }
    n = n + 1
}
test.expectTrue(test.liveStringCount() - baseline < 64)

// add + delete churn must not accumulate element strings.
let base2 = test.liveStringCount()
let s2 = new Set<string>()
let j = 0
while j < 1000 {
    let e = "x" + j.toString()
    s2.add(e)
    let _ = s2.delete(e)
    j = j + 1
}
test.expect(s2.size(), 0)
test.expectTrue(test.liveStringCount() - base2 < 64)
