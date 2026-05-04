// By-value return for **structs > 16 B**: caller allocates the
// destination buffer and passes a hidden pointer to the C fn via
// the indirect-result register (X8 on AArch64, RDI on x86_64 SysV).
// Cranelift's `ArgumentPurpose::StructReturn` handles the slot
// assignment; the JIT pre-allocs an ilang instance, hands its user
// pointer over as the hidden first arg, and surfaces that same
// pointer as the call's result (rc=1, ready to bind).
//
// interp: skip
use test

@extern(C) {
    @repr(C) struct big32 {
        a: i64
        b: i64
        c: i64
        d: i64
    }

    fn make_big32(a: i64, b: i64, c: i64, d: i64): big32
}

let g = make_big32(10, 20, 30, 40)
test.expect(g.a, 10)
test.expect(g.b, 20)
test.expect(g.c, 30)
test.expect(g.d, 40)

// Multiple calls return independent fresh instances.
let h = make_big32(1, 2, 3, 4)
test.expect(h.a, 1)
test.expect(g.a, 10)
