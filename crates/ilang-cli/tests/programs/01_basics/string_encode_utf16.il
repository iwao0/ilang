// `string.encodeUtf16(nulTerminated?)` — UTF-16 code units, with
// optional trailing 0x0000. The default is NUL-terminated so the
// result can be handed straight to Win32 W-suffix APIs through the
// implicit `u16[] → *const u16` coercion.
use std.test as test

// ASCII, NUL-terminated by default: 5 code units + the 0 = 6.
let h = "hello".encodeUtf16()
test.expect(h.length, 6)
test.expect(h[0] as i64, 104)   // 'h'
test.expect(h[4] as i64, 111)   // 'o'
test.expect(h[5] as i64, 0)     // trailing NUL

// Explicit `true` — same as the default.
let ht = "hello".encodeUtf16(true)
test.expect(ht.length, 6)
test.expect(ht[5] as i64, 0)

// `false` — code units only, no trailing NUL.
let h2 = "hello".encodeUtf16(false)
test.expect(h2.length, 5)
test.expect(h2[4] as i64, 111)

// BMP code point (U+3042 = "あ"). One UTF-16 unit. With NUL,
// length is 2. Without, length is 1.
let a = "あ".encodeUtf16(false)
test.expect(a.length, 1)
test.expect(a[0] as i64, 0x3042)
let an = "あ".encodeUtf16()
test.expect(an.length, 2)
test.expect(an[0] as i64, 0x3042)
test.expect(an[1] as i64, 0)

// Supplementary code point (U+20BB7 = "𠮷"). Encodes as a
// UTF-16 surrogate pair `D842 DFB7`. Two units, no NUL.
let sp = "𠮷".encodeUtf16(false)
test.expect(sp.length, 2)
test.expect(sp[0] as i64, 0xD842)
test.expect(sp[1] as i64, 0xDFB7)

// Empty string with default — just the NUL.
let e = "".encodeUtf16()
test.expect(e.length, 1)
test.expect(e[0] as i64, 0)

// Empty string with `false` — zero-length array.
let e0 = "".encodeUtf16(false)
test.expect(e0.length, 0)
