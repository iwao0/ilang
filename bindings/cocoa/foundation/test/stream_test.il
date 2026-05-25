/// NSInputStream — read bytes back from an NSData-backed stream.

use std.test as test
use foundation { NSInputStream, NSData, NSStreamStatus }

let nd = NSData.fromBytes([104 as u8, 105 as u8, 33 as u8])  // "hi!"
let s = NSInputStream.inputStreamWithData(nd)
test.expectFalse(s.handle == 0)

// Open the stream; status flips from `notOpen` (0) to `open` (1).
s.open()
test.expectTrue(s.hasBytesAvailable)

// Read up to 10 bytes — fewer come back since the data is 3
// bytes.
let buf = s.readBytes(10 as u64)
test.expect(buf.length as i64, 3)
test.expect(buf[0] as i64, 104)
test.expect(buf[2] as i64, 33)

// After consuming the whole data, hasBytesAvailable is false.
test.expectFalse(s.hasBytesAvailable)

s.close()

0
