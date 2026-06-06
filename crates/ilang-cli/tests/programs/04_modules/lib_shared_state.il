// Sibling module exporting a mutable top-level `let`. The write
// lives in `lib_shared_writer.il` (a different module), so the
// loader has to qualify the Assign target across the `use`-import
// rename boundary.

pub let SHARED_COUNT: i64 = 0
