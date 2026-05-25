// Variadic native extern: `int snprintf(char *buf, size_t n,
// const char *fmt, ...)`. The fixed prefix is `(buf, n, fmt)`;
// the trailing args supply format specifier values. The JIT
// builds a per-call Cranelift signature including each extra
// arg's actual JIT type so the System V variadic ABI works,
// and on Apple AArch64 the signature is padded with dummy regs
// so the variadic args spill to the stack as required by the
// platform variadic ABI.
//
// Raw `char *` / `*const char` stay inside the `@extern(C)` block;
// callers see ordinary ilang types via per-shape wrappers.
use std.test as test
use std.ffi { cstrFromString }

@extern(C) {
    @lib("c") fn snprintf(buf: *u8, n: size_t, fmt: *const char, ...): i32

    // Same C symbol via @symbol, but with the C-accurate `*char` /
    // `*const char` shape on the ilang side. Verifies that two
    // declarations can resolve to the same dlsym symbol with
    // different ilang signatures.
    @lib("c") @symbol("snprintf")
    fn snprintfC(buf: *char, n: size_t, fmt: *const char, ...): i32

    fn formatI64(buf: u8[], fmt: string, x: i64): i32 {
        snprintf(buf, buf.length as u64, cstrFromString(fmt), x)
    }
    fn formatI64Str(buf: u8[], fmt: string, a: i64, b: string): i32 {
        snprintf(buf, buf.length as u64, cstrFromString(fmt), a, cstrFromString(b))
    }
    fn formatF64(buf: u8[], fmt: string, x: f64): i32 {
        snprintf(buf, buf.length as u64, cstrFromString(fmt), x)
    }
    fn formatPlain(buf: u8[], fmt: string): i32 {
        snprintf(buf, buf.length as u64, cstrFromString(fmt))
    }

    // *char-shaped wrapper: u8[] → *u8 → i64 → *char. char and u8 are
    // both 1 byte at the ABI; the cast through i64 is the bridge.
    fn formatI64ViaChar(buf: u8[], fmt: string, x: i64): i32 {
        let p_u8: *u8 = buf
        let p_char: *char = (p_u8 as i64) as *char
        snprintfC(p_char, buf.length as u64, cstrFromString(fmt), x)
    }
}

let buf: u8[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]

// Single integer.
let n1 = formatI64(buf, "n=%d", 42)
test.expect(n1 as i64, 4)
test.expect(buf[0] as i64, 110)   // 'n'
test.expect(buf[1] as i64, 61)    // '='
test.expect(buf[2] as i64, 52)    // '4'
test.expect(buf[3] as i64, 50)    // '2'
test.expect(buf[4] as i64, 0)     // NUL

// Two args: integer + string.
let n2 = formatI64Str(buf, "%d-%s", 7, "hi")
test.expect(n2 as i64, 4)
test.expect(buf[0] as i64, 55)    // '7'
test.expect(buf[1] as i64, 45)    // '-'
test.expect(buf[2] as i64, 104)   // 'h'
test.expect(buf[3] as i64, 105)   // 'i'

// f64 arg with %.2f.
let n3 = formatF64(buf, "%.2f", 3.14)
test.expect(n3 as i64, 4)
test.expect(buf[0] as i64, 51)    // '3'
test.expect(buf[1] as i64, 46)    // '.'
test.expect(buf[2] as i64, 49)    // '1'
test.expect(buf[3] as i64, 52)    // '4'

// No variadic args at all (only fixed prefix).
let n4 = formatPlain(buf, "abc")
test.expect(n4 as i64, 3)
test.expect(buf[0] as i64, 97)    // 'a'
test.expect(buf[1] as i64, 98)    // 'b'
test.expect(buf[2] as i64, 99)    // 'c'

// *char-shaped declaration (same C symbol via @symbol). Same
// behaviour as the *u8 version since char and u8 are both 1-byte
// at the C ABI.
let n5 = formatI64ViaChar(buf, "n=%d", 42)
test.expect(n5 as i64, 4)
test.expect(buf[0] as i64, 110)   // 'n'
test.expect(buf[1] as i64, 61)    // '='
test.expect(buf[2] as i64, 52)    // '4'
test.expect(buf[3] as i64, 50)    // '2'
