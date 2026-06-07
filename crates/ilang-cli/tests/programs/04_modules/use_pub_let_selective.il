// Selective import of `pub let` (not fn / class / enum) — the
// bare name binds to the loader-renamed slot and reads land on
// the same storage the declaring module exports. Per syntax.md
// §13b "pub applies to top-level `let`".
use std.test as test
use lib_pub_let { GREETING, WIDTH }

test.expectStr(GREETING, "hello")
test.expect(WIDTH, 80)
// The module-qualified name also resolves.
test.expectStr(lib_pub_let.GREETING, "hello")
test.expect(lib_pub_let.WIDTH, 80)
