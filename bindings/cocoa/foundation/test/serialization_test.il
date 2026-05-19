// NSJSONSerialization + NSPropertyListSerialization — round-trip
// a dictionary through both encoders.

use test
use foundation { NSJSONSerialization, NSPropertyListSerialization,
                 NSMutableDictionary, NSMutableArray, NSNumber,
                 NSDictionary, NSArray, NSString, NSData,
                 JSONWritingOptions, JSONReadingOptions,
                 PropertyListFormat, nsstr, ilstr }

// Build a small JSON-friendly object graph: {"name": "ilang",
// "count": 42, "tags": ["a", "b"]}.
let dict = NSMutableDictionary.alloc().init()
dict.setObject(nsstr("ilang"), nsstr("name"))
dict.setObject(NSNumber.fromI64(42), nsstr("count"))
let tags = NSMutableArray.alloc().init()
tags.addObject(nsstr("a"))
tags.addObject(nsstr("b"))
dict.setObject(tags, nsstr("tags"))

// isValidJSONObject.
test.expectTrue(NSJSONSerialization.isValidJSONObject(dict))

// JSON encode → decode.
let data = NSJSONSerialization.dataWithJSONObject(
    dict, JSONWritingOptions.none, 0
)
test.expectTrue(data.length > 0)

let decoded = NSJSONSerialization.JSONObjectWithData(
    data, JSONReadingOptions.none, 0
)
let back = NSDictionary.__wrap_handle(decoded.handle)
test.expect(back.count, 3)

let name = NSString.__wrap_handle(back.objectForKey(nsstr("name")).handle)
test.expectStr(ilstr(name), "ilang")

let count = NSNumber.__wrap_handle(back.objectForKey(nsstr("count")).handle)
test.expect(count.i64Value, 42)

let backTags = NSArray.__wrap_handle(back.objectForKey(nsstr("tags")).handle)
test.expect(backTags.count, 2)

// NSPropertyListSerialization — XML format round-trip.
let plistData = NSPropertyListSerialization.dataWithPropertyList(
    dict, PropertyListFormat.xml, 0 as u64, 0
)
test.expectTrue(plistData.length > 0)

// Should start with `<?xml`.
let firstFive = plistData.bytes()
test.expect(firstFive[0] as i64, 60)  // '<'
test.expect(firstFive[1] as i64, 63)  // '?'

0
