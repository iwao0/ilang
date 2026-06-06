// Other half of the circular `use` pair — see
// `use_circular_pair.il`. The `use` declaration here is what
// previously tripped `LoadError::CircularImport`; with the
// discover + parse split it just enqueues nothing new.
use use_circular_pair

pub fn peek(): i64 { use_circular_pair.ping() }
