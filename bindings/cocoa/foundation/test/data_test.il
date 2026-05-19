// NSData — bytes round-trip + length + base64.

use test
use foundation { NSData, NSDataBase64EncodingOptions, NSDataBase64DecodingOptions,
            makeRange, nsstr, ilstr }

// Empty buffer.
let empty: u8[] = []
let nd0 = NSData.fromBytes(empty)
test.expect(nd0.length, 0)

// Round-trip a small byte buffer.
let bytes: u8[] = [104 as u8, 105 as u8, 33 as u8]  // "hi!"
let nd = NSData.fromBytes(bytes)
test.expect(nd.length, 3)
let back = nd.bytes()
test.expect(back.length as i64, 3)
test.expect(back[0] as i64, 104)
test.expect(back[1] as i64, 105)
test.expect(back[2] as i64, 33)

// Base64 round-trip.
let b64 = nd.base64EncodedString(NSDataBase64EncodingOptions.none)
test.expectStr(ilstr(b64), "aGkh")
let decoded = NSData.alloc().initWithBase64EncodedString(
    b64, NSDataBase64DecodingOptions.none
)
test.expect(decoded.length, 3)
let decBytes = decoded.bytes()
test.expect(decBytes[0] as i64, 104)
test.expect(decBytes[2] as i64, 33)

// subdataWithRange + isEqualToData.
let big = NSData.fromBytes([0 as u8, 1 as u8, 2 as u8, 3 as u8, 4 as u8])
let mid = big.subdataWithRange(makeRange(1 as u64, 3 as u64))
test.expect(mid.length, 3)
test.expect(mid.bytes()[0] as i64, 1)
test.expect(mid.bytes()[2] as i64, 3)

let twin = NSData.fromBytes([1 as u8, 2 as u8, 3 as u8])
test.expectTrue(mid.isEqualToData(twin))
test.expectFalse(mid.isEqualToData(big))

0
