// `string.fromUtf16(units: u16[]): string` — UTF-16 → UTF-8.
// The whole buffer is consumed; invalid sequences (unpaired
// surrogates) become U+FFFD.
use std.test as test

// Round-trip with the no-NUL form is exact.
let s = "hello"
test.expectStr(string.fromUtf16(s.encodeUtf16(false)), "hello")

// BMP code point (U+3042).
test.expectStr(string.fromUtf16("あ".encodeUtf16(false)), "あ")

// Supplementary code point (U+20BB7) — encodes / decodes via
// a surrogate pair.
test.expectStr(string.fromUtf16("𠮷".encodeUtf16(false)), "𠮷")

// Manually constructed buffer.
let buf: u16[] = [104, 105]
test.expectStr(string.fromUtf16(buf), "hi")

// Empty buffer → empty string.
let empty: u16[] = []
test.expectStr(string.fromUtf16(empty), "")

// Invalid UTF-16 (lone low surrogate without high) → U+FFFD.
// 0xDC00 in isolation is unpaired and gets replaced.
let bad: u16[] = [0xDC00 as u16]
test.expectStr(string.fromUtf16(bad), "\u{FFFD}")
