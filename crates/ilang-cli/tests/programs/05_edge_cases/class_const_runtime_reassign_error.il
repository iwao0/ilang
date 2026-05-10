// `const` static fields are still one-time-assigned even when
// the initializer is a runtime expression — explicit
// reassignment from user code is rejected.
// expect-error: cannot assign to const static field

fn three(): i64 { 3 }
class C {
    init() {}
    const y: i64 = three()
}
C.y = 99
