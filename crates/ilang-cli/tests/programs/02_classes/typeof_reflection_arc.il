// ARC balance for reflection reads. `typeof(x).name` retains a
// registry string, `.fields` / `.methods` / `.typeArgs` mint fresh
// arrays, `.parent` mints a fresh Optional cell, `.kind` returns an
// interned unit-enum box (release is a no-op) — all owned, so the
// reads classify as fresh and the consumer drops the share. Three
// pinned bugs: the reads were classified borrowed (one array / one
// string leaked per read), new_string_array stamped its header with
// PK_STR (11) — which the release cascade reads as KIND_WEAK, so
// element strings never released — and __type_parent tagged its
// cell kind=Object around a bare TypeHandle id (a release would
// have cascaded __release_object onto a small integer).

use std.test as test

class A {
    init() {}
}
class Pet: A {
    name: string
    init() { super(); this.name = "p" }
    speak(): i64 { 1 }
}

let p = new Pet()
let base = test.liveAllocBytes()
let sbase = test.liveStringCount()
let i = 0
while i < 200 {
    let t = typeof(p)
    test.expectStr(t.name, "Pet")
    test.expect(t.fields.length, 1)
    test.expectStr(t.fields[0], "name")
    test.expectTrue(t.methods.length >= 1)
    if let some(pt) = t.parent { test.expectStr(pt.name, "A") }
    let _ = t.kind
    let _ = t.typeArgs
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)
test.expectTrue(test.liveStringCount() - sbase < 16)
