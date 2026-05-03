// Variadic native extern: `int snprintf(char *buf, size_t n,
// const char *fmt, ...)`. The fixed prefix is `(buf, n, fmt)`;
// the trailing args supply format specifier values. The JIT
// builds a per-call Cranelift signature including each extra
// arg's actual JIT type so the System V variadic ABI works,
// and on Apple AArch64 the signature is padded with dummy regs
// so the variadic args spill to the stack as required by the
// platform variadic ABI.
// interp: skip
use test

@extern("c", variadic) fn snprintf(buf: u8[], n: u64, fmt: string): i32

let buf: u8[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]

// Single integer.
let n1 = snprintf(buf, 32, "n=%d", 42)
test.expect(n1 as i64, 4)
test.expect(buf[0] as i64, 110)   // 'n'
test.expect(buf[1] as i64, 61)    // '='
test.expect(buf[2] as i64, 52)    // '4'
test.expect(buf[3] as i64, 50)    // '2'
test.expect(buf[4] as i64, 0)     // NUL

// Two args: integer + string.
let n2 = snprintf(buf, 32, "%d-%s", 7, "hi")
test.expect(n2 as i64, 4)
test.expect(buf[0] as i64, 55)    // '7'
test.expect(buf[1] as i64, 45)    // '-'
test.expect(buf[2] as i64, 104)   // 'h'
test.expect(buf[3] as i64, 105)   // 'i'

// f64 arg with %.2f.
let n3 = snprintf(buf, 32, "%.2f", 3.14)
test.expect(n3 as i64, 4)
test.expect(buf[0] as i64, 51)    // '3'
test.expect(buf[1] as i64, 46)    // '.'
test.expect(buf[2] as i64, 49)    // '1'
test.expect(buf[3] as i64, 52)    // '4'

// No variadic args at all (only fixed prefix).
let n4 = snprintf(buf, 32, "abc")
test.expect(n4 as i64, 3)
test.expect(buf[0] as i64, 97)    // 'a'
test.expect(buf[1] as i64, 98)    // 'b'
test.expect(buf[2] as i64, 99)    // 'c'
