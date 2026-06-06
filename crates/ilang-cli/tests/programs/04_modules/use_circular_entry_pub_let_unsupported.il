// Documents the one known shape that the circular-`use` support
// can't currently handle: an entry that itself declares
// `pub let X = ...` and lives in a `use` cycle where the partner
// reaches back through `<entry>.X`.
//
// Why it fails: `apply_use(entry)`'s stmt-merge is skipped (so
// `console.log` / `test.expect` in the entry don't run twice), and
// the `load_program_full` outer loop appends `entry_stmts`
// unprefixed. So `pub let X = 42` lands in `merged` as bare `X`,
// never as `use_circular_entry_pub_let_unsupported.X`. The dep's
// `read_x()` body references the dotted form, which the type
// checker can't resolve.
//
// Fixing this properly needs either parallel-binding semantics
// (where bare `X` and `<entry>.X` alias the same storage) or a
// rewrite of dep-side `<entry>.X` references to bare `X`. Both
// touch the qualify / prefix passes more broadly than the
// circular-use commit set warrants, so the limitation is recorded
// here as a regression fixture: if the loader's behaviour shifts
// in a way that lets the dotted form resolve, this test starts
// failing and forces a conscious update.
//
// Workarounds for users:
//   - Move the `pub let` into a sibling library file (gui_impl-
//     style `pub let WINDOWS = new Map<>()` is fine — the file
//     hosting the binding isn't the entry).
//   - Expose the value through a `pub fn get_x(): T { X }`
//     accessor; functions are items, get prefix-merged the normal
//     way, and round-trip cleanly across the cycle.
//
// expect-error: undefined variable
use lib_circular_pub_let_consumer

pub let X: i64 = 42

// Reference the partner's reader so the cycle has a runtime
// touch-point. Without this the entry could technically succeed
// even if `read_x` failed to compile in isolation.
console.log(lib_circular_pub_let_consumer.read_x())
