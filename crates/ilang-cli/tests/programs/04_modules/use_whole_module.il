// `use lib_helpers` brings the whole module in under its name; access
// each member via `lib_helpers.member`.
use test
use lib_helpers

test.expect(lib_helpers.double(21), 42)
test.expect(lib_helpers.triple(7), 21)
test.expect(lib_helpers.square(5), 25)

// Module-level const accessed the same way; the loader inlines it
// at the call site, so this is identical to writing 10 directly.
test.expect(lib_helpers.TEN, 10)
