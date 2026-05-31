// Enums with payload-carrying variants can't be Set elements yet
// — equality between two `Result.ok(5)` instances would need
// structural comparison the language doesn't expose for enums.
// Reject at type-check with the same hint Set/Map use for class
// types that haven't supplied the equality protocol.
//
// expect-error: set element type
enum Outcome {
    ok: (i64)
    err: (string)
}

let s = new Set<Outcome>()
