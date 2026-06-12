// Fixed-length arrays with heap elements (`Box[2]`) may stand alone
// as a class field, a local binding, or a fn parameter — but not
// sit inside another composite type: container cell cascades have
// no representation for a header-less buffer-with-static-length.
// One representative reject (composite component); returns,
// reassignment, captures, and literal-inference paths carry the
// same family of errors.
//
// expect-error: as a component of another type

class Box {
    n: i64
    init(x: i64) { this.n = x }
}
let t: (Box[2], i64) = ([new Box(1), new Box(2)], 3)
