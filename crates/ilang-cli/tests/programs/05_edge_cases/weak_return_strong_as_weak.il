// `return strongExpr` from a `: T.weak` function downgrades the strong
// to a weak. The return path drops the source's strong +1, so it must
// take a weak share first (order matters — a fresh source has no other
// strong holder). Without it `return new T()` frees the box before the
// caller's `WeakUpgrade`, a use-after-free; a borrowed return whose
// object is still alive must keep resolving to `some`.
//
// expect: fresh none
// expect: alive 7

class Tag {
    v: i64
    init(x: i64) { this.v = x }
}

fn retFresh(): Tag.weak {
    return new Tag(1)
}
fn retBorrow(n: Tag): Tag.weak {
    return n
}

// Fresh: no strong holder, so the weak is immediately dead — but it must
// be `none`, not a use-after-free reading a recycled slot.
let wf = retFresh()
let filler: Tag[] = []
let i = 0
while i < 8 {
    filler.push(new Tag(999))
    i = i + 1
}
match wf.get() {
    some(t) { console.log(`fresh BAD some v=${t.v}`) }
    none { console.log("fresh none") }
}

// Borrowed: `keep` holds the strong, so the weak resolves.
let keep = new Tag(7)
let wb = retBorrow(keep)
match wb.get() {
    some(t) { console.log(`alive ${t.v}`) }
    none { console.log("alive BAD none") }
}
