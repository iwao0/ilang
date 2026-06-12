// Fixed-length arrays with heap elements have value semantics:
// every container cell takes a copy on store (fields, Optional,
// enum payloads, tuple slots, dynamic-array elements, Map values,
// closure captures, Promise values). What remains a type error is
// RETURNING one — the return would hand out a view with no
// counterpart rule for who copies; copy into a dynamic array
// instead. (Whole-binding reassignment and `Array.fill` with a
// fixed-array element are the other remaining rejects.)
//
// expect-error: can't be returned

class Box {
    n: i64
    init(x: i64) { this.n = x }
}
fn bad(): Box[2] {
    let a: Box[2] = [new Box(1), new Box(2)]
    a
}
