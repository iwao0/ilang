// `use module as _ { Item }` — namespace-suppressing selective
// import. Only the listed bare names become visible; no
// `module.X` / `alias.X` path is registered. Per syntax.md §13b.
use std.test as test
use lib_helpers as _ { double, triple }

test.expect(double(5), 10)
test.expect(triple(5), 15)
test.expect(double(triple(2)), 12)

// `lib_helpers.double` is NOT accessible here — only the bare
// names are. Naming `lib_helpers` would be a "no such module"
// error and this fixture would fail to type-check.
