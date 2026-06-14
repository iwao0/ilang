// Passing a FRESH heap value as an argument to a virtually-dispatched
// call must release the caller's transient +1 after the call, exactly
// like the direct (concrete-receiver) method path does.
//
// Before: `lower_iface_dispatch` and `lower_fn_field_call` lowered their
// args but never released a fresh heap arg, so `iface.method(new Box())`
// and `obj.fnField(new Box())` leaked one cell per call (deinit count
// short by one; live bytes grew linearly under churn). The concrete
// path (`lower_class_method_call`) already released fresh args; the two
// indirect paths now mirror it. A borrowed (variable) arg is still left
// untouched — no over-release.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface Reader { read(b: Box): i64 }
interface Reader2: Reader { tag(): i64 }   // child interface

class Impl: Reader2 {
    read(b: Box): i64 { b.n }              // borrows the arg
    tag(): i64 { 7 }
}

class FnHolder {
    f: fn(Box): i64
    init() { this.f = fn(b: Box): i64 { b.n } }
}

// (1) fresh heap arg to an interface-dispatched method
fn viaIface(): i64 {
    let r: Reader = new Impl()
    r.read(new Box(5))
}
// (2) fresh heap arg through a CHILD-interface receiver — `read` is the
// inherited slot, dispatched after walking the parent chain
fn viaChildIface(): i64 {
    let r: Reader2 = new Impl()
    r.read(new Box(6))
}
// (3) fresh heap arg to a closure stored in a field
fn viaFnField(): i64 {
    let h = new FnHolder()
    h.f(new Box(7))
}
// (4) borrowed (non-fresh) arg through interface dispatch — must NOT be
// released by the caller; `b` stays alive past the call
fn viaBorrowed(): i64 {
    let r: Reader = new Impl()
    let b = new Box(9)
    let got = r.read(b)
    got + b.n
}

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
test.expect(viaIface(), 5)
test.expect(viaChildIface(), 6)
test.expect(viaFnField(), 7)
test.expect(viaBorrowed(), 18)
// four helper calls, each makes and frees exactly one Box
test.expect(deinits[0] - bd, 4)
test.expect(test.liveAllocBytes() - d0, 0)

// churn: zero leak, exactly one deinit per Box per call
let bd1 = deinits[0]
let d1 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + viaIface() + viaChildIface() + viaFnField() + viaBorrowed()
    i = i + 1
}
test.expect(deinits[0] - bd1, 400)
test.expect(test.liveAllocBytes() - d1, 0)
console.log("ok")
