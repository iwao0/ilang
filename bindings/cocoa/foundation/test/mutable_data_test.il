// NSMutableData — append + setLength + increaseLengthBy.

use test
use foundation { NSMutableData }

let m = NSMutableData.data()
test.expect(m.length, 0)

m.appendBytesFromBuffer([1 as u8, 2 as u8, 3 as u8])
test.expect(m.length, 3)
let b = m.bytes()
test.expect(b[0] as i64, 1)
test.expect(b[2] as i64, 3)

// Concat more.
m.appendBytesFromBuffer([4 as u8, 5 as u8])
test.expect(m.length, 5)
test.expect(m.bytes()[4] as i64, 5)

// setLength shrinks.
m.setLength(2 as u64)
test.expect(m.length, 2)
test.expect(m.bytes()[1] as i64, 2)

// increaseLengthBy pads with zeros.
m.increaseLengthBy(3 as u64)
test.expect(m.length, 5)
let post = m.bytes()
test.expect(post[2] as i64, 0)
test.expect(post[3] as i64, 0)
test.expect(post[4] as i64, 0)

0
