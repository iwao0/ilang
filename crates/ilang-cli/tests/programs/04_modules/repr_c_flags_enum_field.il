// `@flags` enums hold OR-combined bit values of the underlying
// integer type, so any bit pattern the C side writes into a
// `@flags`-typed CRepr struct field is a valid value. Reading
// such a field must not abort with "unknown discriminant" even
// when the value doesn't match a single declared variant —
// otherwise round-tripping bitflag fields (e.g. wgpu's
// `WGPUSurfaceCapabilities.usages`) is impossible.
//
// Companion to `repr_c_enum_field_unknown_aborts.il`, which
// covers the non-`@flags` case where the abort is the desired
// behavior.
use std.test as test

@flags
pub enum Perm: u32 {
    none  = 0x00
    read  = 0x01
    write = 0x02
    exec  = 0x04
}

@extern(C) {
    struct Slot {
        bits: Perm
    }
}

// `read | exec` = 0x05 — no single declared variant matches
// this value, but it is a valid bit pattern. The combined
// expression goes through the `@flags` bitwise-OR lowering,
// then the result is stored into the CRepr struct field.
let s = Slot { bits: Perm.read | Perm.exec }

// Reading the field back must not abort. The repr-natural
// load + unit-cell wrap produces an enum value whose
// underlying integer carries the combined bits.
test.expect((s.bits as u32) as i64, 0x05)

// A plain single-variant value still round-trips.
s.bits = Perm.write
test.expect((s.bits as u32) as i64, 0x02)

// Storing a fully-combined pattern through the field works too.
s.bits = Perm.read | Perm.write | Perm.exec
test.expect((s.bits as u32) as i64, 0x07)
