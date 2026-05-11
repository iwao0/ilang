// HFA (Homogeneous Floating-point Aggregate): a struct of 1..=4
// fields all of the same float type passes via FP registers
// (V0..V3 on AArch64 AAPCS64; XMM0..XMM3 on x86_64 SysV) instead
// of GPRs. The JIT spreads each field into its own AbiParam(F32/
// F64) so Cranelift's calling-conv assigner picks the right
// register slot.
//
// jit: skip
use test

@extern(C) {
    struct vec3f {
        x: f32
        y: f32
        z: f32
    }

    struct pair64 {
        a: f64
        b: f64
    }

    fn vec3f_dot(a: vec3f, b: vec3f): f32
    fn vec3f_make(x: f32, y: f32, z: f32): vec3f
    fn pair64_sum(p: pair64): f64
}

let a = new vec3f()
a.x = 1.0 as f32
a.y = 2.0 as f32
a.z = 3.0 as f32

let b = new vec3f()
b.x = 4.0 as f32
b.y = 5.0 as f32
b.z = 6.0 as f32

// Dot product: 1*4 + 2*5 + 3*6 = 32
test.expectF64(vec3f_dot(a, b) as f64, 32.0)

// HFA struct return: f32x3 reassembled into an ilang Object.
let v = vec3f_make(7.0 as f32, 8.0 as f32, 9.0 as f32)
test.expectF64(v.x as f64, 7.0)
test.expectF64(v.y as f64, 8.0)
test.expectF64(v.z as f64, 9.0)

// f64 HFA (16 B but uses FP regs because all-f64).
let p = new pair64()
p.a = 1.5
p.b = 2.5
test.expectF64(pair64_sum(p), 4.0)
