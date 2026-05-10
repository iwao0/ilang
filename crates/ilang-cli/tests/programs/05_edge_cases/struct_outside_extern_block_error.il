// `struct` declarations are reserved for `@extern(C) {}` blocks —
// they describe a C ABI layout, not a regular ilang heap class.
// Writing `pub struct Foo { … }` at module scope must be a parse
// error so users don't accidentally create a non-CRepr struct
// (use `class` for that).
//
// expect-error: expected 'fn', 'class', 'enum', or 'use' after attributes

pub struct Bare {
    x: i32
}
