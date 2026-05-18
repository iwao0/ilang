// Spotlight metadata bindings — NSMetadataItem (one file's
// metadata) and NSMetadataQuery (the asynchronous query
// driving the Spotlight engine).
//
// Queries run on the user's metadata daemon (`mds`) so they
// respect every locally-indexed file. The query lifecycle is
// `setPredicate:` → `startQuery` → wait for the
// `NSMetadataQueryDidFinishGatheringNotification` notification
// → walk `results`.

use core { NSObject, NSString, NSArray, NSDictionary }
use predicate { NSPredicate }
use network { NSURL }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSMetadataItem` — wraps a single file's Spotlight
    /// attributes (display name, content type, last modified,
    /// keywords, …). Pull attribute values by key
    /// (`kMDItemDisplayName`, `kMDItemContentType`, etc.). The
    /// keys are NSString constants imported from CoreServices —
    /// pass them as ilang `NSString`s via `nsstr("kMDItemDisplayName")`.
    @objc pub class NSMetadataItem : NSObject {
        @objc("alloc")
        pub static alloc(): NSMetadataItem
        @objc("initWithURL:")
        pub initWithURL(url: NSURL): NSMetadataItem

        @objc("attributes")
        pub get attributes(): NSArray
        @objc("valueForAttribute:")
        pub valueForAttribute(key: NSString): NSObject
        @objc("valuesForAttributes:")
        pub valuesForAttributes(keys: NSArray): NSDictionary
    }

    /// `NSMetadataQuery` — asynchronous Spotlight query. Wire a
    /// predicate, set the search scopes (e.g.
    /// `NSMetadataQueryUbiquitousDocumentsScope`), start the
    /// query, and observe the results notification.
    @objc pub class NSMetadataQuery : NSObject {
        @objc("alloc")
        pub static alloc(): NSMetadataQuery
        @objc("init")
        pub init(): NSMetadataQuery

        @objc("predicate")
        pub get predicate(): NSPredicate
        @objc("setPredicate:")
        pub set predicate(p: NSPredicate)
        @objc("searchScopes")
        pub get searchScopes(): NSArray
        @objc("setSearchScopes:")
        pub set searchScopes(scopes: NSArray)
        @objc("sortDescriptors")
        pub get sortDescriptors(): NSArray
        @objc("setSortDescriptors:")
        pub set sortDescriptors(descs: NSArray)
        @objc("valueListAttributes")
        pub get valueListAttributes(): NSArray
        @objc("setValueListAttributes:")
        pub set valueListAttributes(attrs: NSArray)

        @objc("startQuery")
        pub startQuery(): bool
        @objc("stopQuery")
        pub stopQuery()
        @objc("disableUpdates")
        pub disableUpdates()
        @objc("enableUpdates")
        pub enableUpdates()
        @objc("isStarted")
        pub get isStarted(): bool
        @objc("isGathering")
        pub get isGathering(): bool
        @objc("isStopped")
        pub get isStopped(): bool

        @objc("results")
        pub get results(): NSArray
        @objc("resultCount")
        pub get resultCount(): u64
        @objc("resultAtIndex:")
        pub resultAtIndex(i: u64): NSObject
    }

    /// `NSMetadataQueryDelegate` — overrides result construction
    /// (`replacementObjectForResultObject:` lets you swap each
    /// raw NSMetadataItem for a richer wrapper) and value access
    /// (`replacementValueForAttribute:value:` rewrites the
    /// per-attribute values in flight).
    @objc pub interface NSMetadataQueryDelegate {
        @optional @objc("metadataQuery:replacementObjectForResultObject:")
        metadataQueryReplacementObjectForResultObject(
            query: NSMetadataQuery, result: NSMetadataItem
        ): NSObject
        @optional @objc("metadataQuery:replacementValueForAttribute:value:")
        metadataQueryReplacementValueForAttribute(
            query: NSMetadataQuery, attribute: NSString, value: NSObject
        ): NSObject
    }
}
