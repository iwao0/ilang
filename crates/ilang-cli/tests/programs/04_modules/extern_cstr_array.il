// C-side `char**` (NUL-terminated array of NUL-terminated strings)
// marshals to ilang `string[]` via the `cstrArrayToStrings(p)`
// helper. The helper walks the input until it hits a NULL pointer,
// copies each `char*` into a fresh `StringRc`, and packs them into
// a heap array — classic shape for `environ` / `g_strsplit` /
// `argv`-like APIs without ever exposing a raw pointer to ilang
// code outside the @extern(C) block.
//
// JIT-only.
// jit: skip
use std.test as test
use std.ffi { cstrArrayToStrings }

@extern(C) {
    fn get_cstr_array(): *const *const char
    fn get_empty_cstr_array(): *const *const char

    fn collectStrings(): string[] {
        cstrArrayToStrings(get_cstr_array())
    }
    fn collectEmpty(): string[] {
        cstrArrayToStrings(get_empty_cstr_array())
    }
}

let xs = collectStrings()
test.expect(xs.length as i64, 3)
test.expectStr(xs[0], "first")
test.expectStr(xs[1], "second")
test.expectStr(xs[2], "third")

// Empty array (just a NULL terminator) → length-0 ilang array.
let empty = collectEmpty()
test.expect(empty.length as i64, 0)

// Each call produces an independent copy — mutating one doesn't
// affect another.
let xs2 = collectStrings()
xs2[0] = "patched"
test.expectStr(xs[0], "first")
test.expectStr(xs2[0], "patched")
