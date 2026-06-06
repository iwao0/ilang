// Cycle partner for `use_circular_log.il`. Has no body of its own
// — its only role is to close the circular `use` graph so the
// duplication-prevention path in `apply_use` (canon == entry_canon
// ⇒ skip stmt-merge) is exercised from both load directions.
//
// When this file is the harness entry, `apply_use(use_circular_log)`
// runs the partner's `console.log("hello")` exactly once.
//
// expect: hello
use use_circular_log
