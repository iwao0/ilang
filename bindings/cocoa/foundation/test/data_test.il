// NSData — bytes round-trip + length + base64.

use test
use foundation { NSData, DataBase64EncodingOptions, DataBase64DecodingOptions,
            nsstr, ilstr }

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
let b64 = nd.base64EncodedString(DataBase64EncodingOptions.none)
test.expectStr(ilstr(b64), "aGkh")
let decoded = NSData.alloc().initWithBase64EncodedString(
    b64, DataBase64DecodingOptions.none
)
test.expect(decoded.length, 3)
let decBytes = decoded.bytes()
test.expect(decBytes[0] as i64, 104)
test.expect(decBytes[2] as i64, 33)

0
