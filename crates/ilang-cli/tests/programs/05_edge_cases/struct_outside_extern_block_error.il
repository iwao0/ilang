// Top-level `struct` (declared outside `@extern(C) { ... }`) cannot
// mention C-only types: `char`, `void`, `size_t`, `ssize_t`, or raw
// pointers. The check is transitive — referencing another struct
// that contains such a type is rejected too.
//
// expect-error: top-level `struct Bad`

pub struct Bad {
    c: char
}
