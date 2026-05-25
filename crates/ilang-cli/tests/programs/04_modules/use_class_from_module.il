// Instantiating + invoking methods on a class declared in another
// module. The qualified `new lib_counter.Counter(...)` form is what
// the loader rewrites class references to.
use std.test as test
use lib_counter

let c = new lib_counter.Counter(10)
test.expect(c.get(), 10)
c.bump()
c.bump()
c.add(8)
test.expect(c.get(), 20)
