// Passing an ilang `T[]` (where `T` is a CRepr struct) into a C
// function that takes `*T` must hand the C side the element data
// pointer, not the ilang array header. Mirrors the wgpu pattern
// where `WGPURenderPassDescriptor.colorAttachments` is typed
// `*WGPURenderPassColorAttachment` but the ilang side builds a
// `WGPURenderPassColorAttachment[]` and assigns it directly.
//
// jit: skip
use std.test as test

@extern(C) {
    struct PairI32 {
        a: i32
        b: i32
    }

    fn pair_first_a(p: *PairI32): i32
    fn pair_first_b(p: *PairI32): i32
    fn pair_nth_a(p: *PairI32, n: i64): i32
    fn pair_nth_b(p: *PairI32, n: i64): i32

    // 1. Direct `T[] as *T` cast at a call site.
    fn check_direct(xs: PairI32[]): i64 {
        let a = pair_first_a(xs as *PairI32)
        let b = pair_first_b(xs as *PairI32)
        (a as i64) * 1000 + (b as i64)
    }

    // 2. Indirect — `T[]` assigned to a `*T`-typed field of a
    //    descriptor struct (the wgpu shape). Field assignment
    //    must apply the same data-pointer extraction.
    struct PairArrayDesc {
        count: u64
        items: *PairI32
    }
    fn check_via_field(xs: PairI32[]): i64 {
        let d = PairArrayDesc {
            count: xs.length as u64,
            items: xs,
        }
        let a = pair_first_a(d.items)
        let b = pair_first_b(d.items)
        (a as i64) * 1000 + (b as i64)
    }

    fn check_nth(xs: PairI32[], n: i64): i64 {
        let p = xs as *PairI32
        let a = pair_nth_a(p, n)
        let b = pair_nth_b(p, n)
        (a as i64) * 1000 + (b as i64)
    }
}

let xs: PairI32[] = [
    PairI32 { a: 7  as i32, b: 13 as i32 },
    PairI32 { a: 21 as i32, b: 34 as i32 },
    PairI32 { a: 55 as i32, b: 89 as i32 },
]

// 1. Direct cast.
test.expect(check_direct(xs), 7013)

// 2. Through a `*T` struct field.
test.expect(check_via_field(xs), 7013)

// 3. Element stride is correct — second / third elements line up.
test.expect(check_nth(xs, 0), 7013)
test.expect(check_nth(xs, 1), 21034)
test.expect(check_nth(xs, 2), 55089)
