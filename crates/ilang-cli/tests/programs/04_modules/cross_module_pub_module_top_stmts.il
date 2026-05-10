// A sub-module's top-level stmts may reference its own non-pub
// members. The visibility check must judge those accesses from
// the sub-module's perspective, not the entry's — without the
// `source_module` plumbing on `Stmt`, every merged stmt would
// be judged from `<entry>` and fail the cross-module rule on
// the very module that declared the member.
//
// This positive fixture relies on `lib_acl_internals.il` which
// has a non-pub static referenced by the module's own top-level
// `let`. If the visibility check fires falsely, the entry's
// trivial `console.log` would never run.
// expect: ok
use lib_acl_internals
console.log("ok")
