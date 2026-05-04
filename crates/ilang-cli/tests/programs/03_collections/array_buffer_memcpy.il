// Pass numeric arrays as `void *` buffers to a native extern.
// `memcpy(dst, src, n)` is the canonical exercise: it writes
// through the dst pointer and reads through the src pointer.
// ARC keeps both arrays alive across the call.
// interp: skip
use test

@extern(C) {
    @lib("c") fn memcpy(dst: *i64, src: *const i64, n: size_t): i64
    @lib("c") fn memset(buf: *u8, byte: i32, n: size_t): i64
}

// Source values; element width is 8 bytes (i64). Both arrays use
// the dynamic shape so the call signature `i64[]` matches without
// a fixed-vs-dynamic conversion.
let src: i64[] = [10, 20, 30, 40, 50]
let dst: i64[] = [0, 0, 0, 0, 0]

// 5 elements × 8 bytes = 40 bytes. memcpy returns dst.
let _ret = memcpy(dst, src, 40 as u64)

test.expect(dst[0], 10)
test.expect(dst[1], 20)
test.expect(dst[2], 30)
test.expect(dst[3], 40)
test.expect(dst[4], 50)

// Source array unchanged.
test.expect(src[0], 10)
test.expect(src[4], 50)

// Same idea on a byte buffer — `u8[]` maps to `unsigned char *`.
let bytes: u8[] = [1, 2, 3, 4, 5, 6, 7, 8]
let _ = memset(bytes, 65 as i32, 4 as u64)   // overwrite first 4 bytes with 'A' (65)
test.expect(bytes[0] as i64, 65)
test.expect(bytes[1] as i64, 65)
test.expect(bytes[2] as i64, 65)
test.expect(bytes[3] as i64, 65)
test.expect(bytes[4] as i64, 5)   // untouched
test.expect(bytes[7] as i64, 8)
