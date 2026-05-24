// Optional<Optional<T>>. The interpreter handles this via
// Box<Option<...>>; the JIT's nullable-pointer Optional layout can't
// distinguish Some(none) from none for the same inner pointer width,
// so this fixture is interpreter-only for now.
use std.test

// Optional<Optional<string>> — both layers are heap (Str / Optional<Str>).
let outer_some_inner_some: string?? = some(some("hi"))
let outer_some_inner_none: string?? = some(none)
let outer_none: string?? = none

test.expectBool(outer_some_inner_some.isSome, true)
let inner1 = outer_some_inner_some.unwrap()
test.expectBool(inner1.isSome, true)
test.expectStr(inner1.unwrap(), "hi")

test.expectBool(outer_some_inner_none.isSome, true)
let inner2 = outer_some_inner_none.unwrap()
test.expectBool(inner2.isNone, true)

test.expectBool(outer_none.isNone, true)
