// Same-class access through `pub` members is reachable from
// the entry module — sanity counterpart to the
// `cross_module_pub_*_access` fixtures.
use lib_acl
use test

let f = new lib_acl.Foo()
test.expect(f.visible as i64, 2)
test.expect(f.outer() as i64, 2)
