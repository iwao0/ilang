// Mutual `use` between two sibling modules. `use_circular_pair`
// imports `lib_circular_other`, which `use`s `use_circular_pair`
// back. The loader's discover + parse split absorbs the cycle:
// the second leg sees the file already in `seen` and stops
// recursing, instead of erroring with `CircularImport`.
//
// Both files double as entry points (the harness runs every `.il`
// it finds). When `lib_circular_other` is the entry, this file's
// top-level still runs through `apply_use` ordering, so the
// assertion stays valid either way.
use std.test as test
use lib_circular_other

test.expect(lib_circular_other.peek() + 1, 42)

pub fn ping(): i64 { 41 }
