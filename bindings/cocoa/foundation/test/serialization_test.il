/// NSJSONSerialization + NSPropertyListSerialization.

use test
use foundation { NSJSONSerialization, NSPropertyListSerialization,
                 NSMutableDictionary, NSData,
                 NSJSONWritingOptions, NSJSONReadingOptions,
                 NSPropertyListFormat, nsstr }

// Build a small JSON-friendly object: {"name": "ilang"}.
let dict = NSMutableDictionary.alloc().init()
dict.setObject(nsstr("ilang"), nsstr("name"))

// isValidJSONObject — top-level dict is valid.
test.expectTrue(NSJSONSerialization.isValidJSONObject(dict))

// JSON encode produces non-empty bytes.
let data = NSJSONSerialization.dataWithJSONObject(
    dict, NSJSONWritingOptions.none, 0
)
test.expectTrue(data.length > 0)

// PropertyList XML encode produces non-empty bytes.
let plistData = NSPropertyListSerialization.dataWithPropertyList(
    dict, NSPropertyListFormat.xml, 0 as u64, 0
)
test.expectTrue(plistData.length > 0)

0
