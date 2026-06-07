use std.test as test
use lib_helpers as h { double, triple }
// bare name (selective): works
test.expect(double(5), 10)
// alias.name: works
test.expect(h.triple(5), 15)
// module.name: works
test.expect(lib_helpers.double(5), 10)
