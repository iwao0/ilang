// Identifier / sort-key bindings. Two unrelated but small
// classes sharing one file:
//
//   * NSUUID — RFC 4122 random identifiers, the default
//     "give me something globally unique" type.
//   * NSSortDescriptor — sort-key spec hand-off to NSArray's
//     `sortedArrayUsingDescriptors:` and friends.

use core { NSObject, NSString }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSUUID` — RFC 4122 UUID. The default initialiser
    /// (`UUID()`) mints a random v4 UUID; `initWithUUIDString:`
    /// parses a canonical string like
    /// `"550E8400-E29B-41D4-A716-446655440000"`. Use
    /// `UUIDString` to round-trip back to text.
    @objc pub class NSUUID : NSObject {
        @objc("UUID")
        pub static UUID(): NSUUID
        @objc("alloc")
        pub static alloc(): NSUUID
        @objc("init")
        pub init(): NSUUID
        @objc("initWithUUIDString:")
        pub initWithUUIDString(s: NSString): NSUUID

        @objc("UUIDString")
        pub get UUIDString(): NSString
        @objc("isEqual:")
        pub isEqual(other: NSObject): bool
        @objc("hash")
        pub get hash(): i64
    }

    /// `NSSortDescriptor` — single sort key + direction (and
    /// optionally a comparison selector). Hand a sorted array
    /// of descriptors to
    /// `[NSArray sortedArrayUsingDescriptors:]` to get a stable
    /// multi-key sort: the first descriptor breaks ties first,
    /// then the second, and so on. `reversedSortDescriptor` is
    /// the inverted-direction sibling for the same key, handy
    /// for "click the column header to flip the sort".
    @objc pub class NSSortDescriptor : NSObject {
        @objc("sortDescriptorWithKey:ascending:")
        pub static sortDescriptorWithKey(
            key: NSString, ascending: bool
        ): NSSortDescriptor
        @objc("sortDescriptorWithKey:ascending:selector:")
        pub static sortDescriptorWithKeyAndSelector(
            key: NSString, ascending: bool, selector: i64
        ): NSSortDescriptor

        @objc("key")
        pub get key(): NSString
        @objc("ascending")
        pub get ascending(): bool
        @objc("selector")
        pub get selector(): i64

        @objc("reversedSortDescriptor")
        pub get reversedSortDescriptor(): NSSortDescriptor
    }
}
