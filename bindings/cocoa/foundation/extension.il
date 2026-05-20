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

        /// Both completion handlers receive `(id, NSError *)`. The
        /// ObjCBlock typed slot uses `(NSObject?, NSError?)` so MIR
        /// picks the `(id, id)`-encoded trampoline; pass a closure
        /// whose params are NSObject-typed (raw `fn(i64, i64)` would
        /// dispatch to the `(void *, size_t)` shape instead — wrong
        /// NSMethodSignature).
        @objc("loadItemForTypeIdentifier:options:completionHandler:")
        pub loadItemForTypeIdentifier(
            uti: NSString, options: NSDictionary,
            completionHandler: ObjCBlock<fn(NSObject?, NSError?)>
        )
        @objc("loadDataRepresentationForTypeIdentifier:completionHandler:")
        pub loadDataRepresentationForTypeIdentifier(
            uti: NSString, completionHandler: ObjCBlock<fn(NSData?, NSError?)>
        )
    }

    /// `NSItemProviderReading` — protocol any class that wants
    /// to be reconstructed from an NSItemProvider's raw bytes
    /// conforms to. The class methods declare the UTIs it
    /// accepts; the factory walks one of them back into a
    /// concrete instance.
    @objc pub interface NSItemProviderReading {
        @objc("readableTypeIdentifiersForItemProvider")
        readableTypeIdentifiersForItemProvider(): NSArray
        @objc("objectWithItemProviderData:typeIdentifier:error:")
        objectWithItemProviderData(
            data: NSData, typeIdentifier: NSString, error: i64
        ): NSObject
    }

    /// `NSItemProviderWriting` — symmetric write protocol. The
    /// class declares its writable UTIs; the instance method
    /// serialises into the matching bytes for each.
    @objc pub interface NSItemProviderWriting {
        @objc("writableTypeIdentifiersForItemProvider")
        writableTypeIdentifiersForItemProvider(): NSArray
        @objc("loadDataWithTypeIdentifier:forItemProviderCompletionHandler:")
        loadDataWithTypeIdentifier(
            typeIdentifier: NSString,
            forItemProviderCompletionHandler: ObjCBlock<fn(NSData?, NSError?)>
        ): NSObject
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
            items: NSArray, completionHandler: ObjCBlock<fn(bool)>
        )
        @objc("cancelRequestWithError:")
        pub cancelRequestWithError(error: NSError)
        @objc("openURL:completionHandler:")
        pub openURL(url: NSURL, completionHandler: ObjCBlock<fn(bool)>)
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
