// `union` declarations are reserved for `@extern(C) {}` blocks —
// they describe a C ABI layout (overlapping fields), and have no
// meaning outside an FFI struct definition. Writing `pub union U
// { … }` at module scope must surface as a parse error.
//
// expect-error: expected 'fn', 'class', 'enum', or 'use' after attributes

pub union Bare {
    x: i32
    y: f32
}
