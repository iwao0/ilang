// Dynamic `Quad[]` built via `push` then dropped at function exit
// — exercises the release path that previously crashed with a
// misaligned-pointer dereference (`__release_array` treated each
// inline-struct cell as a heap object pointer and tried to read
// its rc out of the struct's payload bytes).
//
// Also asserts that the inline cells line up through `xs[i].field`,
// confirming both the codegen (stride = c_size, KIND_NONE tag)
// and the runtime (memcpy push, address-return load) agree on the
// new layout.

use std.test as test

pub struct Quad {
    r: f64
    g: f64
    b: f64
    a: f64
}

fn build_and_drop(): i64 {
    let xs: Quad[] = []
    xs.push(Quad { r: 1.0, g: 2.0, b: 3.0, a: 4.0 })
    xs.push(Quad { r: 5.0, g: 6.0, b: 7.0, a: 8.0 })
    let sum = xs[0].r + xs[0].g + xs[1].r + xs[1].g
    sum as i64
    // xs drops here — release_array walks the inline cells with
    // KIND_NONE, no per-cell rc touched.
}

test.expect(build_and_drop(), 14)
