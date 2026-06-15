// A fresh strong downgraded into a weak CONSUMER slot — a field, an
// array element (push or literal), a tuple element, or a `T.weak`
// function parameter — must take the weak share before the source's
// strong +1 is dropped, or the box frees under the live weak (UAF).
// Mirrors the let/reassign/return fixes for the remaining consumer
// sites. Borrowed sources already worked; a fresh source (no other
// strong holder) is the one that frees early.
//
// Deterministic in normal mode: make all five dangling weaks first,
// then a single live filler burst recycles the freed slots with a
// positive rc, so a surviving UAF upgrades to a bogus `some`.
//
// expect: field none
// expect: push none
// expect: literal none
// expect: tuple none
// expect: arg none

class Tag {
    v: i64
    init(x: i64) { this.v = x }
}
class Holder {
    w: Tag.weak
    init() {}
}

fn mkField(): Holder {
    let h = new Holder()
    h.w = new Tag(1)
    h
}
fn mkPush(): Tag.weak[] {
    let a: Tag.weak[] = []
    a.push(new Tag(2))
    a
}
fn mkLit(): Tag.weak[] {
    let a: Tag.weak[] = [new Tag(3)]
    a
}
fn mkTuple(): (Tag.weak, i64) {
    let t: (Tag.weak, i64) = (new Tag(4), 0)
    t
}
fn stash(h: Holder, w: Tag.weak) { h.w = w }
fn mkArg(): Holder {
    let h = new Holder()
    stash(h, new Tag(5))
    h
}

let hf = mkField()
let ap = mkPush()
let al = mkLit()
let at = mkTuple()
let ha = mkArg()

// One filler burst, kept alive across every `.get()` below, so the
// recycled slots hold a positive rc when a dangling weak reads them.
let filler: Tag[] = []
let i = 0
while i < 32 {
    filler.push(new Tag(999))
    i = i + 1
}

match hf.w.get() {
    some(t) { console.log(`field BAD ${t.v}`) }
    none { console.log("field none") }
}
match ap[0].get() {
    some(t) { console.log(`push BAD ${t.v}`) }
    none { console.log("push none") }
}
match al[0].get() {
    some(t) { console.log(`literal BAD ${t.v}`) }
    none { console.log("literal none") }
}
match at[0].get() {
    some(t) { console.log(`tuple BAD ${t.v}`) }
    none { console.log("tuple none") }
}
match ha.w.get() {
    some(t) { console.log(`arg BAD ${t.v}`) }
    none { console.log("arg none") }
}
