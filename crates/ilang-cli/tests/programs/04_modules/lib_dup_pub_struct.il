// Umbrella module that re-exports both siblings. Each sibling
// declares `pub struct Shared`, so after this umbrella merges them
// into a single namespace the loader's `dup_pub` pass sees two
// `pub struct lib_dup_pub_struct.Shared` and errors out.

pub use lib_dup_pub_struct_a.*
pub use lib_dup_pub_struct_b.*
