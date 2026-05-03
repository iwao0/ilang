// Static field initializers must be compile-time constant.
// expect-error: not allowed in `const`
fn three(): i64 { 3 }
class C {
    init() {}
    static x: i64 = three()
}
let _ = new C()
