// When two branch classes share MORE THAN ONE common interface, the
// join is ambiguous — `common_object_join` can't pick one without an
// expected type at the join point, so it falls through to a mismatch
// error. (A single common interface joins cleanly; see
// branch_join_common_interface.il.) Annotate or restructure to resolve.
//
// expect-error: type mismatch

interface A { fa(): i64 }
interface B { fb(): i64 }

class P: A, B {
    init() {}
    fa(): i64 { 1 }
    fb(): i64 { 2 }
}

class Q: A, B {
    init() {}
    fa(): i64 { 3 }
    fb(): i64 { 4 }
}

fn mk(k: i64): A {
    if k == 0 { new P() } else { new Q() }
}

let _ = mk(1).fa()
