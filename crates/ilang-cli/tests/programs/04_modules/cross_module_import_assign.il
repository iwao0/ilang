// Regression: a write to a global imported via `use M { X }` from
// another module must resolve the same way a read does. The loader
// qualified `Var` reads to `M.X` but skipped the `Assign` target,
// so the write hit "undefined variable X" at type-check time even
// though reads of the same global worked. See loader/rename.rs.
// expect: 0
// expect: 7
use lib_shared_writer { add_to_count, read_count }

console.log(read_count())
add_to_count(7)
console.log(read_count())
