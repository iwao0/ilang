// Regression fixture for the entry-stmt duplication bug fixed in
// 207c4b12. Before the fix, a side-effect statement (`console.log`,
// `test.expect`, …) in the entry would run *twice* when the entry
// sat in a circular `use` graph: once unprefixed via
// `merged.stmts.extend(entry_stmts)` and once prefixed via
// `apply_use`'s nested call back into the entry. The harness runs
// every `.il` fixture as an entry, so both this file and its lib
// partner cover the cycle from each direction:
//
//   - when this file is the entry, `apply_use` sees the entry's
//     `canon == entry_canon` and skips its stmt-merge.
//   - when `lib_circular_log_other.il` is the entry, the same
//     `console.log` reaches `merged.stmts` exactly once through
//     `apply_use(use_circular_log)`.
//
// expect: hello
use lib_circular_log_other

console.log("hello")
