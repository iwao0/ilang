// App-extension / share-sheet bindings. `NSItemProvider` is the
// generic data-handle Foundation uses for drag-and-drop / share
// sheet payloads; `NSExtensionContext` / `NSExtensionItem` are
// the runtime surface an app extension (Share / Action / Today
// widget) operates against.

use core { NSObject, NSString, NSArray, NSDictionary, NSError, NSData }
use network { NSURL }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSItemProvider` — generic payload handle: a value tagged
    /// with one or more UTI type identifiers. Used by drag /
    /// drop, the share sheet, NSUserActivity, and Handoff. Build
    /// with `initWithObject:` for an arbitrary NSItemProviderWriting
    /// value, or hand-load type-specific data through the
    /// `registerDataRepresentationForTypeIdentifier:visibility:loadHandler:`
    /// family.
    @objc pub class NSItemProvider : NSObject {
        @objc("alloc")
        pub static alloc(): NSItemProvider
        @objc("init")
        pub init(): NSItemProvider
        @objc("initWithObject:")
        pub initWithObject(obj: NSObject): NSItemProvider
        @objc("initWithContentsOfURL:")
        pub initWithContentsOfURL(url: NSURL): NSItemProvider

        @objc("registeredTypeIdentifiers")
        pub get registeredTypeIdentifiers(): NSArray
        @objc("hasItemConformingToTypeIdentifier:")
        pub hasItemConformingToTypeIdentifier(uti: NSString): bool
        @objc("canLoadObjectOfClass:")
        pub canLoadObjectOfClass(cls: NSObject): bool

        @objc("suggestedName")
        pub get suggestedName(): NSString
        @objc("setSuggestedName:")
        pub set suggestedName(s: NSString)

        @objc("loadItemForTypeIdentifier:options:completionHandler:")
        pub loadItemForTypeIdentifier(
            uti: NSString, options: NSDictionary, completionHandler: i64
        )
        @objc("loadDataRepresentationForTypeIdentifier:completionHandler:")
        pub loadDataRepresentationForTypeIdentifier(
            uti: NSString, completionHandler: i64
        )
    }

    /// `NSExtensionContext` — the runtime handed to an app
    /// extension when it loads. Read incoming items off
    /// `inputItems`; once you're done call either
    /// `completeRequestReturningItems:completionHandler:` (extension
    /// completed successfully) or `cancelRequestWithError:` (user
    /// aborted, or the work failed).
    @objc pub class NSExtensionContext : NSObject {
        @objc("inputItems")
        pub get inputItems(): NSArray

        @objc("completeRequestReturningItems:completionHandler:")
        pub completeRequestReturningItems(
            items: NSArray, completionHandler: i64
        )
        @objc("cancelRequestWithError:")
        pub cancelRequestWithError(error: NSError)
        @objc("openURL:completionHandler:")
        pub openURL(url: NSURL, completionHandler: i64)
    }

    /// `NSExtensionItem` — single item flowing through an
    /// extension's input / output array. `attributedTitle` and
    /// `attributedContentText` describe the human-readable
    /// surface; `attachments` is an array of NSItemProvider data
    /// payloads.
    @objc pub class NSExtensionItem : NSObject {
        @objc("alloc")
        pub static alloc(): NSExtensionItem
        @objc("init")
        pub init(): NSExtensionItem

        @objc("attributedTitle")
        pub get attributedTitle(): NSObject
        @objc("setAttributedTitle:")
        pub set attributedTitle(s: NSObject)
        @objc("attributedContentText")
        pub get attributedContentText(): NSObject
        @objc("setAttributedContentText:")
        pub set attributedContentText(s: NSObject)
        @objc("attachments")
        pub get attachments(): NSArray
        @objc("setAttachments:")
        pub set attachments(arr: NSArray)
        @objc("userInfo")
        pub get userInfo(): NSDictionary
        @objc("setUserInfo:")
        pub set userInfo(info: NSDictionary)
    }
}
