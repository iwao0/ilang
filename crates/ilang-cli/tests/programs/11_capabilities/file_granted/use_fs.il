// The granted counterpart to file_denied: with `file` in the
// sibling manifest the std.fs sink is allowed and runs to completion.
//
// expect: ok
use std.fs as fs
let _ = fs.exists("/tmp")
console.log("ok")
