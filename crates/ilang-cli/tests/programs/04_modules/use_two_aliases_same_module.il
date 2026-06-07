use std.test as test
use lib_helpers as m1 { double }
use lib_helpers as m2 { triple }
test.expect(m1.double(5), 10)
test.expect(m2.triple(5), 15)
test.expect(double(5), 10)
test.expect(triple(5), 15)
