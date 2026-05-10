// `pub` on class fields gates cross-module read access. A
// non-pub field (default — module-private) defined in module
// `lib_acl` is unreachable from the entry module's code; the
// type checker rejects the field expression with a localised
// error.
//
// expect-error: is module-private (defined in module `lib_acl`)
use lib_acl

let f = new lib_acl.Foo()
let _ = f.secret
