// JSON / Property List serialization. The cousin `archive.il`
// covers the keyed-archiver pair for arbitrary NSCoding object
// graphs; this file handles the two text-bound formats
// Foundation ships out of the box.

use core { NSObject, NSData }

// NSJSONReadingOptions — bit flags controlling
// `JSONObjectWithData:options:error:`. `mutableContainers` /
// `mutableLeaves` ask for NSMutableArray / NSMutableString
// instances in the parsed tree; `fragmentsAllowed` permits the
// JSON to be a bare scalar instead of an array or object.
@flags
pub enum NSJSONReadingOptions: u64 {
    none              = 0x00
    mutableContainers = 0x01
    mutableLeaves     = 0x02
    fragmentsAllowed  = 0x04
}

// NSJSONWritingOptions — bit flags controlling
// `dataWithJSONObject:options:error:`. `prettyPrinted` emits
// indented output; `sortedKeys` orders dictionaries by their
// keys (useful for reproducible builds / diffing);
// `withoutEscapingSlashes` emits `/` as itself instead of `\/`.
@flags
pub enum NSJSONWritingOptions: u64 {
    none                  = 0x00
    prettyPrinted         = 0x01
    sortedKeys            = 0x02
    fragmentsAllowed      = 0x04
    withoutEscapingSlashes = 0x08
}

/// NSPropertyListFormat — output / input format for
/// NSPropertyListSerialization. `xml` is the human-readable form
/// macOS' `defaults` command emits; `binary` is the compact form
/// the system uses for in-memory representations.
pub enum NSPropertyListFormat: u64 {
    openStep = 1
    xml      = 100
    binary   = 200
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSJSONSerialization` — read / write JSON. The returned
    /// object is whatever the JSON's top-level value would have
    /// been: an `NSArray`, `NSDictionary`, `NSString`, `NSNumber`,
    /// or NSNull (handle == NSNull's singleton). For most call
    /// sites you'll narrow the result by inspecting the JSON
    /// shape — the static method's return is the un-narrowed
    /// `NSObject`, so cast or call `containsObject:` on it.
    /// Error handling uses the `NSError **` slot; pass `0` to
    /// ignore failure (you'll get a nil-handle wrapper back).
    @objc pub class NSJSONSerialization : NSObject {
        @objc("dataWithJSONObject:options:error:")
        pub static dataWithJSONObject(
            obj: NSObject, options: NSJSONWritingOptions, error: i64
        ): NSData
        @objc("JSONObjectWithData:options:error:")
        pub static JSONObjectWithData(
            data: NSData, options: NSJSONReadingOptions, error: i64
        ): NSObject
        @objc("isValidJSONObject:")
        pub static isValidJSONObject(obj: NSObject): bool
    }

    /// `NSPropertyListSerialization` — read / write Property List
    /// files (Apple's `.plist`, used for app preferences, the
    /// `Info.plist` bundle metadata, and AppleScript dictionaries).
    /// Same `NSError **` discipline as `NSJSONSerialization`.
    @objc pub class NSPropertyListSerialization : NSObject {
        @objc("dataWithPropertyList:format:options:error:")
        pub static dataWithPropertyList(
            obj: NSObject, format: NSPropertyListFormat, options: u64, error: i64
        ): NSData
        @objc("propertyListWithData:options:format:error:")
        pub static propertyListWithData(
            data: NSData, options: u64, format: *u64, error: i64
        ): NSObject
    }
}
