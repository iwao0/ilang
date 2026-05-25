// `pub use M` (without `{ * }`) keeps M's items namespaced under
// the umbrella — callers reach them as `<umbrella>.<M>.<item>`.
// Use the `as _ { * }` form when you want the flatten behaviour.
//
// Reaches a fn / const / class declared in a re-exported submodule
// via the namespaced path: `outer.inner.foo()`, `outer.inner.VAL`,
// `new outer.inner.Box(...)`.
use std.test as test
use outer

test.expectStr(outer.inner.greeting(), "hello from inner")
test.expect(outer.inner.MAGIC, 42)

let b = new outer.inner.Box(7)
test.expect(b.value, 7)
