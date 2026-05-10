// Static field initializers accept any expression — when the
// RHS isn't compile-time constant the loader demotes it to a
// runtime init that runs once at program startup. The slot
// itself stays declared at the class level; the synthetic
// write fills in the real value before any user code reads
// it. Applies to both `static` and `const` static fields.
// expect: 3
// expect: 3
fn three(): i64 { 3 }
class C {
    init() {}
    static x: i64 = three()
    const y: i64 = three()
}
let _ = new C()
console.log(C.x)
console.log(C.y)
