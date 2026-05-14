// Top-level `union` (declared outside `@extern(C) { ... }`) inherits
// the same field-type restriction as a top-level `struct`: no
// `char`, `void`, `size_t`, `ssize_t`, or raw pointers, transitively.
//
// expect-error: top-level `union Bad`

pub union Bad {
    p: *const i32
    n: i64
}
