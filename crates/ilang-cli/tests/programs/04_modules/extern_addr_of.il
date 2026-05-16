// `&local` inside @extern(C): take the address of a stack-bound
// scalar so a C-style out-pointer call can write through it. This
// replaces the older idiom of allocating a 1-element ilang array
// as a stand-in for `*T` storage — the array decays to a data
// pointer at the ABI but allocates on the heap and pays ARC.
//
// Here we use a small C helper (`set_via_ptr`) compiled into the
// test runtime that writes its second arg through the first.
//
// jit: skip
use test

@extern(C) {
    fn set_via_ptr(out: *i32, value: i32)
    fn set_via_ptr_f64(out: *f64, value: f64)

    fn read_back_i32(value: i32): i32 {
        let x: i32 = 0
        set_via_ptr(&x, value)
        x
    }
    fn read_back_f64(value: f64): f64 {
        let x: f64 = 0.0
        set_via_ptr_f64(&x, value)
        x
    }
}

test.expect(read_back_i32(42 as i32) as i64, 42)
test.expect(read_back_i32(-7 as i32) as i64, -7)
test.expectF64(read_back_f64(3.5) - 3.5, 0.0)
test.expectF64(read_back_f64(-0.25) - (-0.25), 0.0)
