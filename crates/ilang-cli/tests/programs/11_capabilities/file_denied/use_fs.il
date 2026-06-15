// A std.fs sink with no `file` capability granted (the sibling
// ilang.toml grants only `os`). The call must be refused: under JIT
// the cap_require_file gate aborts at runtime, and under AOT the
// build fails before a binary is produced. Both arms surface the
// same "capability 'file'" diagnostic.
//
// expect-error: capability 'file'
use std.fs as fs
let _ = fs.exists("/tmp")
