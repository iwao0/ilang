// Consumer half of the known-limitation pair (see
// `use_circular_entry_pub_let_unsupported.il`). When this file is
// the harness entry, the partner is *not* the entry, so its
// `pub let X` does flow through `apply_use`'s stmt-merge — the
// dotted lookup resolves, the partner's
// `console.log(consumer.read_x())` runs through the merged stmt
// stream, and "42" reaches stdout.
//
// expect: 42
use use_circular_entry_pub_let_unsupported

pub fn read_x(): i64 { use_circular_entry_pub_let_unsupported.X }
