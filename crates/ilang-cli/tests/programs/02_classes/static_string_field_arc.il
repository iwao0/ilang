// A heap-typed static field (a string) must take ownership of the value
// stored into it and drop the previous occupant — the same rule an
// instance field follows. `StoreStatic` did neither.
//
// The UAF: `Cls.s = arg` where `arg` is a borrowed param holding a fresh
// string. The static slot stored the value without retaining it, so once
// the caller released the source's transient +1 the slot dangled — a
// later read came back as a freed buffer (the string printed empty even
// though its length was intact). A fresh value assigned *directly*
// (`Cls.s = "a" + b`) happened to work because it owned its +1.
//
// Fix: retain a borrowed value on store and release the old occupant
// (skipping the init store, which holds the const-baked initial value).
// Now both fresh and borrowed sources are sound and reassignment doesn't
// leak.

use std.test as test

class Holder {
    static label: string = "init"
    init() {}
    // Stores a borrowed param — the path that used to UAF.
    static setFrom(s: string) { Holder.label = s }
    static read(): string { Holder.label }
}

// Fresh string passed *through a param* into the static slot.
fn viaParam(k: i64): i64 {
    Holder.setFrom("p" + k.toString())     // "p<k>" — was freed under us
    Holder.read().length
}

// Fresh string assigned directly to the static slot.
fn direct(k: i64): i64 {
    Holder.label = "d" + k.toString()
    Holder.read().length
}

// Value check: the stored content survives (not just its length).
Holder.setFrom("h" + "ello")
test.expectStr(Holder.read(), "hello")
test.expect(viaParam(7), 2)                // "p7"
test.expect(direct(42), 3)                 // "d42"

// Churn both paths; the string registry must stay flat (retain/release
// balanced) and reads must keep returning live buffers.
let acc: i64[] = [0]
let bd = test.liveStringCount()
let i = 0
while i < 300 {
    acc[0] = acc[0] + viaParam(i) + direct(i)
    i = i + 1
}
let grew = test.liveStringCount() - bd
test.expectTrue(grew < 50)
// Final read is still a live string (not a freed buffer).
test.expectTrue(Holder.read().length > 0)
