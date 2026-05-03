// Fixed-length numeric array as a `@repr(C)` class field. Bytes
// are embedded inline (no heap header) so the layout matches the
// C `T arr[N];` declaration. Index access on the field reads /
// writes the embedded slot directly.
use test

@repr(C) class sockaddr_in {
    sin_family: u16
    sin_port: u16
    sin_addr: u32
    sin_zero: u8[8]
}

let s = new sockaddr_in()

// Sanity: zero-initialised.
test.expect(s.sin_family as i64, 0)
test.expect(s.sin_port as i64, 0)
test.expect(s.sin_zero[0] as i64, 0)
test.expect(s.sin_zero[7] as i64, 0)

// Set primitive fields.
s.sin_family = 2 as u16
s.sin_port = 8080 as u16
s.sin_addr = 0x7f000001 as u32   // 127.0.0.1
test.expect(s.sin_family as i64, 2)
test.expect(s.sin_port as i64, 8080)
test.expect(s.sin_addr as i64, 2130706433)

// Element-wise write to the embedded array.
s.sin_zero[0] = 11 as u8
s.sin_zero[1] = 22 as u8
s.sin_zero[7] = 99 as u8
test.expect(s.sin_zero[0] as i64, 11)
test.expect(s.sin_zero[1] as i64, 22)
test.expect(s.sin_zero[2] as i64, 0)   // untouched
test.expect(s.sin_zero[7] as i64, 99)

// Larger element types work too.
@repr(C) class sample {
    counts: i64[3]
    ratios: f64[2]
}

let sm = new sample()
sm.counts[0] = 100
sm.counts[1] = 200
sm.counts[2] = 300
sm.ratios[0] = 1.5
sm.ratios[1] = 2.5
test.expect(sm.counts[0], 100)
test.expect(sm.counts[1], 200)
test.expect(sm.counts[2], 300)
test.expectF64(sm.ratios[0], 1.5)
test.expectF64(sm.ratios[1], 2.5)
