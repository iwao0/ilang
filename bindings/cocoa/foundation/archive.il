/// Serialization / archiving bindings — NSCoder (abstract base
/// for read / write), NSKeyedArchiver / NSKeyedUnarchiver
/// (concrete plist-based encoder pair). Used by Cocoa for app
/// state restoration, NSDocument autosave, and any time you need
/// to ship an object graph through an `NSData`.
///
/// Secure coding (`requiresSecureCoding`) is on by default — the
/// `unarchivedObjectOfClass:fromData:error:` family is the
/// recommended one-shot read path; the older
/// `unarchiveObjectWithData:` calls into the insecure path and is
/// intentionally not bound.

use core { NSObject, NSString, NSData, NSError }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSCoding` — protocol every class that wants to survive
    /// `NSKeyedArchiver` participates in. Implement
    /// `encodeWithCoder:` (write your state) and `initWithCoder:`
    /// (read it back) — the archiver / unarchiver invoke them on
    /// each object it walks past.
    @objc pub interface NSCoding {
        @objc("encodeWithCoder:")
        encodeWithCoder(coder: NSCoder)
        @objc("initWithCoder:")
        initWithCoder(coder: NSCoder): NSObject
    }

    /// `NSSecureCoding` — refinement of `NSCoding` that asserts
    /// the class validates the encoded type before instantiating
    /// it. Required for any class that wants to round-trip
    /// through `NSKeyedUnarchiver.unarchivedObjectOfClass:fromData:`.
    /// `supportsSecureCoding` must return `true` on the class
    /// itself; the implementation typically just hard-codes it.
    @objc pub interface NSSecureCoding {
        @objc("supportsSecureCoding")
        supportsSecureCoding(): bool
    }

    /// `NSCopying` — `copyWithZone:` lets `[obj copy]` produce an
    /// independent value-copy of the receiver. The `zone:`
    /// parameter is a historical NSZone pointer that the modern
    /// runtime ignores; pass `cocoa.nil`.
    @objc pub interface NSCopying {
        @objc("copyWithZone:")
        copyWithZone(zone: NSObject): NSObject
    }

    /// `NSMutableCopying` — `mutableCopyWithZone:` for the
    /// "give me a mutable copy" pattern. Pair with `NSCopying` on
    /// the same class to support both `copy` and `mutableCopy`.
    @objc pub interface NSMutableCopying {
        @objc("mutableCopyWithZone:")
        mutableCopyWithZone(zone: NSObject): NSObject
    }

    /// `NSCoder` — abstract encode / decode interface. Both the
    /// archiver and unarchiver inherit from it; user code that
    /// implements `+(BOOL)supportsSecureCoding` /
    /// `-(void)encodeWithCoder:` / `-(id)initWithCoder:` works
    /// through this surface.
    @objc pub class NSCoder : NSObject {
        @objc("encodeObject:forKey:")
        pub encodeObject(obj: NSObject, forKey: NSString)
        @objc("encodeInteger:forKey:")
        pub encodeInteger(value: i64, forKey: NSString)
        @objc("encodeInt32:forKey:")
        pub encodeInt32(value: i32, forKey: NSString)
        @objc("encodeInt64:forKey:")
        pub encodeInt64(value: i64, forKey: NSString)
        @objc("encodeBool:forKey:")
        pub encodeBool(value: bool, forKey: NSString)
        @objc("encodeDouble:forKey:")
        pub encodeDouble(value: f64, forKey: NSString)
        @objc("encodeFloat:forKey:")
        pub encodeFloat(value: f32, forKey: NSString)
        @objc("encodeBytes:length:forKey:")
        pub encodeBytes(buf: *const u8, length: u64, forKey: NSString)
        @objc("encodeDataObject:")
        pub encodeDataObject(data: NSData)

        @objc("decodeObjectForKey:")
        pub decodeObjectForKey(key: NSString): NSObject
        @objc("decodeIntegerForKey:")
        pub decodeIntegerForKey(key: NSString): i64
        @objc("decodeInt32ForKey:")
        pub decodeInt32ForKey(key: NSString): i32
        @objc("decodeInt64ForKey:")
        pub decodeInt64ForKey(key: NSString): i64
        @objc("decodeBoolForKey:")
        pub decodeBoolForKey(key: NSString): bool
        @objc("decodeDoubleForKey:")
        pub decodeDoubleForKey(key: NSString): f64
        @objc("decodeFloatForKey:")
        pub decodeFloatForKey(key: NSString): f32
        @objc("decodeDataObject")
        pub decodeDataObject(): NSData

        @objc("containsValueForKey:")
        pub containsValueForKey(key: NSString): bool
        @objc("requiresSecureCoding")
        pub get requiresSecureCoding(): bool

        @objc("finishEncoding")
        pub finishEncoding()
        @objc("finishDecoding")
        pub finishDecoding()
    }

    /// `NSKeyedArchiver` — write side of the NSKeyedArchiver /
    /// NSKeyedUnarchiver pair. The one-shot
    /// `archivedDataWithRootObject:requiringSecureCoding:error:`
    /// factory is the simplest path; for piecewise encoding,
    /// `alloc().initRequiringSecureCoding(true)` opens an
    /// archiver you fill with `encodeObject:forKey:` calls and
    /// drain to bytes via `encodedData`.
    @objc pub class NSKeyedArchiver : NSCoder {
        @objc("archivedDataWithRootObject:requiringSecureCoding:error:")
        pub static archivedDataWithRootObject(
            obj: NSObject, requiringSecureCoding: bool, error: i64
        ): NSData

        @objc("alloc")
        pub static alloc(): NSKeyedArchiver
        @objc("init")
        pub init(): NSKeyedArchiver
        @objc("initRequiringSecureCoding:")
        pub initRequiringSecureCoding(secure: bool): NSKeyedArchiver

        @objc("encodedData")
        pub get encodedData(): NSData

        @objc("requiresSecureCoding")
        pub get requiresSecureCoding(): bool
        @objc("setRequiresSecureCoding:")
        pub set requiresSecureCoding(v: bool)

        @objc("outputFormat")
        pub get outputFormat(): u64
        @objc("setOutputFormat:")
        pub set outputFormat(v: u64)
    }

    /// `NSKeyedUnarchiver` — read side. The one-shot
    /// `unarchivedObjectOfClass:fromData:error:` factory is the
    /// secure-coding-aware path; pass the concrete root class so
    /// the unarchiver can validate the encoded type before
    /// instantiating it.
    @objc pub class NSKeyedUnarchiver : NSCoder {
        @objc("unarchivedObjectOfClass:fromData:error:")
        pub static unarchivedObjectOfClass(
            cls: NSObject, fromData: NSData, error: i64
        ): NSObject
        @objc("unarchivedObjectOfClasses:fromData:error:")
        pub static unarchivedObjectOfClasses(
            classes: NSObject, fromData: NSData, error: i64
        ): NSObject

        @objc("alloc")
        pub static alloc(): NSKeyedUnarchiver
        @objc("initForReadingFromData:error:")
        pub initForReadingFromData(data: NSData, error: i64): NSKeyedUnarchiver

        @objc("requiresSecureCoding")
        pub get requiresSecureCoding(): bool
        @objc("setRequiresSecureCoding:")
        pub set requiresSecureCoding(v: bool)
    }
}
