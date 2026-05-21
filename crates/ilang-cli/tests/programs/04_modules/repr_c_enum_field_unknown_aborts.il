// Companion to `repr_c_enum_field.il`: when the C side has
// written a value into an enum field that doesn't correspond
// to any declared variant, ilang's CRepr-struct field load
// must abort with a clear diagnostic instead of silently
// minting a ghost variant cell. The contract mirrors Rust's
// `repr(C)` — reading an unknown discriminant is UB / crash —
// and the localised abort beats a delayed memory bug surfacing
// in a downstream `match`.
// expect-error: unknown discriminant

pub enum Tag16: u16 {
    a = 0x1234
    b = 0x5678
}

use libc { memcpy }

@extern(C) {
    struct Frame {
        tag: Tag16
    }

    pub fn fillFrame(f: Frame, bytes: u8[]) {
        let p: *u8 = bytes
        let _ = memcpy(&f, p as *const u8, bytes.length as u64)
    }
}

// 0x9999 is not Tag16.a (0x1234) nor Tag16.b (0x5678).
let bad: u8[] = [ 0x99 as u8, 0x99 as u8 ]

let f = new Frame()
fillFrame(f, bad)

// Touching the enum field forces the validating cell lookup —
// the runtime aborts here.
let _t = f.tag
