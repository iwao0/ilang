// `pub` on class methods gates cross-module method calls. A
// non-pub method on a `pub` class is unreachable from outside
// the declaring module.
//
// expect-error: method `lib_acl.Foo.inner` is module-private
use lib_acl

let f = new lib_acl.Foo()
let _ = f.inner()
