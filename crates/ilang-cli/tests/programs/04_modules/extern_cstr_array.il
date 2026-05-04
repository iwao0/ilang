// `cstrArray` flag: C-side `char**` (NUL-terminated array of
// NUL-terminated strings) marshals to ilang `string[]`. The JIT
// walks the result until it hits a NULL pointer, copies each
// `char*` into a fresh `StringRc`, and packs them into a heap
// array — classic shape for `environ` / `g_strsplit` / `argv`-like
// APIs without ever exposing a raw pointer to ilang code.
//
// JIT-only.
// interp: skip
use test

@extern(cstrArray) fn get_cstr_array(): string[]
@extern(cstrArray) fn get_empty_cstr_array(): string[]

let xs = get_cstr_array()
test.expect(xs.length as i64, 3)
test.expectStr(xs[0], "first")
test.expectStr(xs[1], "second")
test.expectStr(xs[2], "third")

// Empty array (just a NULL terminator) → length-0 ilang array.
let empty = get_empty_cstr_array()
test.expect(empty.length as i64, 0)

// Each call produces an independent copy — mutating one doesn't
// affect another.
let xs2 = get_cstr_array()
xs2[0] = "patched"
test.expectStr(xs[0], "first")
test.expectStr(xs2[0], "patched")
