// FFI marshalling helpers (`stringFromCstr`, `cstrFromString`,
// `arrayFromCArray`, `errnoCheck`, ...) are only callable inside
// an `@extern(C) {}` block. `errnoCheck` takes / returns ordinary
// ilang types so the C-only-types rule alone wouldn't reject it —
// the helper-name list catches it explicitly.
//
// expect-error: FFI marshalling helper, only callable inside an @extern(C)
use std.test

let result = errnoCheck(0 as i32)
test.expect(0, 0)
