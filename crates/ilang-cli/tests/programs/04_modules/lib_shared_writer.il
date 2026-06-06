// Reads AND writes a global imported from another module. The
// loader's rename pass qualifies `Var` reads (`SHARED_COUNT` ->
// `lib_shared_state.SHARED_COUNT`) but used to skip the `Assign`
// target Symbol, so the write below failed type-checking with
// "undefined variable SHARED_COUNT". See loader/rename.rs.

use lib_shared_state { SHARED_COUNT }

pub fn add_to_count(k: i64) { SHARED_COUNT = SHARED_COUNT + k }
pub fn read_count(): i64 { SHARED_COUNT }
