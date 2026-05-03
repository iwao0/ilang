// Selective import: `use module { name1, name2 }` brings only the
// listed names into the local scope, accessed without the module prefix.
use test
use lib_helpers { double, triple }

test.expect(double(5), 10)
test.expect(triple(5), 15)
test.expect(double(triple(2)), 12)

// Names NOT listed are not in scope: `square` and `TEN` aren't
// accessible here. (Verified implicitly — using them would be a
// "undefined function" / "undefined variable" error and this fixture
// would fail to type-check.)
