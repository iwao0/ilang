/// AppleScript / Apple Events bindings — the legacy macOS
/// automation surface that lets external scripts drive your app
/// (or your app drive other apps). Modern equivalents are
/// Shortcuts.app, Intents, and NSUserActivity, but the
/// AppleScript pipe is still wired through Foundation on every
/// shipping macOS release.
///
/// What you actually need depends on which side you're on:
///   * Sending AppleScript: NSAppleScript (compile + run a
///     `.scpt`) and the NSAppleEventDescriptor reply tree.
///   * Receiving AppleScript: subclass-and-register through
///     NSScriptCommand / NSScriptObjectSpecifier and friends.

use core { NSObject, NSString, NSArray, NSDictionary, NSError, NSData }
use datetime { NSDate }
use network { NSURL }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSAppleScript` — compile and execute an AppleScript
    /// source string or `.scpt` file. `executeAndReturnError:`
    /// runs the script and hands back the result as an
    /// `NSAppleEventDescriptor`; the `error:` slot fills in
    /// dictionary form on failure (NSAppleScriptErrorNumber /
    /// ErrorMessage / ErrorAppName etc.). Each instance is
    /// stateful — compiling once and executing many times is
    /// faster than re-compiling per run.
    @objc pub class NSAppleScript : NSObject {
        @objc("alloc")
        pub static alloc(): NSAppleScript
        @objc("initWithSource:")
        pub initWithSource(source: NSString): NSAppleScript
        @objc("initWithContentsOfURL:error:")
        pub initWithContentsOfURL(url: NSURL, error: i64): NSAppleScript

        @objc("source")
        pub get source(): NSString
        @objc("isCompiled")
        pub get isCompiled(): bool
        @objc("compileAndReturnError:")
        pub compileAndReturnError(error: i64): bool
        @objc("executeAndReturnError:")
        pub executeAndReturnError(error: i64): NSAppleEventDescriptor
        @objc("executeAppleEvent:error:")
        pub executeAppleEvent(
            event: NSAppleEventDescriptor, error: i64
        ): NSAppleEventDescriptor
    }

    /// `NSAppleEventDescriptor` — the universal Apple Event
    /// value box. Carries booleans, integers, strings, four-char
    /// type / enum codes, raw data, lists, records, and nested
    /// AppleEvent objects. Build with one of the typed
    /// `descriptorWith…` factories or compose programmatically
    /// from `listDescriptor` / `recordDescriptor`.
    @objc pub class NSAppleEventDescriptor : NSObject {
        @objc("appleEventWithEventClass:eventID:targetDescriptor:returnID:transactionID:")
        pub static appleEventWithEventClass(
            eventClass: u32, eventID: u32,
            targetDescriptor: NSAppleEventDescriptor,
            returnID: i16, transactionID: i32
        ): NSAppleEventDescriptor
        @objc("descriptorWithBoolean:")
        pub static descriptorWithBoolean(b: bool): NSAppleEventDescriptor
        @objc("descriptorWithEnumCode:")
        pub static descriptorWithEnumCode(c: u32): NSAppleEventDescriptor
        @objc("descriptorWithInt32:")
        pub static descriptorWithInt32(v: i32): NSAppleEventDescriptor
        @objc("descriptorWithString:")
        pub static descriptorWithString(s: NSString): NSAppleEventDescriptor
        @objc("descriptorWithTypeCode:")
        pub static descriptorWithTypeCode(c: u32): NSAppleEventDescriptor
        @objc("descriptorWithDescriptorType:bytes:length:")
        pub static descriptorWithDescriptorType(
            t: u32, bytes: *const u8, length: u64
        ): NSAppleEventDescriptor
        @objc("listDescriptor")
        pub static listDescriptor(): NSAppleEventDescriptor
        @objc("nullDescriptor")
        pub static nullDescriptor(): NSAppleEventDescriptor
        @objc("recordDescriptor")
        pub static recordDescriptor(): NSAppleEventDescriptor

        @objc("descriptorType")
        pub get descriptorType(): u32
        @objc("booleanValue")
        pub get booleanValue(): bool
        @objc("enumCodeValue")
        pub get enumCodeValue(): u32
        @objc("int32Value")
        pub get int32Value(): i32
        @objc("stringValue")
        pub get stringValue(): NSString
        @objc("typeCodeValue")
        pub get typeCodeValue(): u32
        @objc("data")
        pub get data(): NSData
        @objc("dateValue")
        pub get dateValue(): NSDate

        @objc("numberOfItems")
        pub get numberOfItems(): i64
        @objc("descriptorAtIndex:")
        pub descriptorAtIndex(i: i64): NSAppleEventDescriptor
        @objc("insertDescriptor:atIndex:")
        pub insertDescriptor(d: NSAppleEventDescriptor, atIndex: i64)
        @objc("removeDescriptorAtIndex:")
        pub removeDescriptorAtIndex(i: i64)

        @objc("attributeDescriptorForKeyword:")
        pub attributeDescriptorForKeyword(keyword: u32): NSAppleEventDescriptor
        @objc("paramDescriptorForKeyword:")
        pub paramDescriptorForKeyword(keyword: u32): NSAppleEventDescriptor
        @objc("setParamDescriptor:forKeyword:")
        pub setParamDescriptor(d: NSAppleEventDescriptor, forKeyword: u32)
        @objc("setAttributeDescriptor:forKeyword:")
        pub setAttributeDescriptor(d: NSAppleEventDescriptor, forKeyword: u32)
    }

    /// `NSAppleEventManager` — process-wide Apple Event router.
    /// Register handlers via
    /// `setEventHandler:andSelector:forEventClass:andEventID:`
    /// to receive specific four-char-code events; the manager
    /// invokes your selector with the incoming event descriptor
    /// and a reply descriptor you fill in.
    @objc pub class NSAppleEventManager : NSObject {
        @objc("sharedAppleEventManager")
        pub static shared(): NSAppleEventManager

        @objc("setEventHandler:andSelector:forEventClass:andEventID:")
        pub setEventHandler(
            handler: NSObject, andSelector: i64,
            forEventClass: u32, andEventID: u32
        )
        @objc("removeEventHandlerForEventClass:andEventID:")
        pub removeEventHandlerForEventClass(eventClass: u32, andEventID: u32)
        @objc("currentAppleEvent")
        pub get currentAppleEvent(): NSAppleEventDescriptor
        @objc("currentReplyAppleEvent")
        pub get currentReplyAppleEvent(): NSAppleEventDescriptor
        @objc("suspendCurrentAppleEvent")
        pub suspendCurrentAppleEvent(): NSObject
    }

    /// `NSScriptCommand` — one invocation of an AppleScript
    /// command coming into your app. Subclass it (or use a
    /// stock subclass like NSCreateCommand / NSCloseCommand) to
    /// implement `executeCommand`; the framework dispatches
    /// based on the registered NSScriptCommandDescription.
    @objc pub class NSScriptCommand : NSObject {
        @objc("alloc")
        pub static alloc(): NSScriptCommand
        @objc("initWithCommandDescription:")
        pub initWithCommandDescription(
            d: NSScriptCommandDescription
        ): NSScriptCommand

        @objc("commandDescription")
        pub get commandDescription(): NSScriptCommandDescription
        @objc("directParameter")
        pub get directParameter(): NSObject
        @objc("setDirectParameter:")
        pub set directParameter(v: NSObject)
        @objc("receiversSpecifier")
        pub get receiversSpecifier(): NSScriptObjectSpecifier
        @objc("setReceiversSpecifier:")
        pub set receiversSpecifier(s: NSScriptObjectSpecifier)
        @objc("evaluatedReceivers")
        pub get evaluatedReceivers(): NSObject

        @objc("arguments")
        pub get arguments(): NSDictionary
        @objc("setArguments:")
        pub set arguments(args: NSDictionary)
        @objc("evaluatedArguments")
        pub get evaluatedArguments(): NSDictionary

        @objc("isWellFormed")
        pub get isWellFormed(): bool
        @objc("executeCommand")
        pub executeCommand(): NSObject

        @objc("scriptErrorNumber")
        pub get scriptErrorNumber(): i64
        @objc("setScriptErrorNumber:")
        pub set scriptErrorNumber(n: i64)
        @objc("scriptErrorString")
        pub get scriptErrorString(): NSString
        @objc("setScriptErrorString:")
        pub set scriptErrorString(s: NSString)
        @objc("scriptErrorOffendingObjectDescriptor")
        pub get scriptErrorOffendingObjectDescriptor(): NSAppleEventDescriptor
    }

    /// `NSScriptCommandDescription` — metadata describing one
    /// command in a scriptable app's sdef: name, command class,
    /// argument list, return type.
    @objc pub class NSScriptCommandDescription : NSObject {
        @objc("alloc")
        pub static alloc(): NSScriptCommandDescription
        @objc("initWithSuiteName:commandName:dictionary:")
        pub initWithSuiteName(
            suiteName: NSString, commandName: NSString, dictionary: NSDictionary
        ): NSScriptCommandDescription

        @objc("suiteName")
        pub get suiteName(): NSString
        @objc("commandName")
        pub get commandName(): NSString
        @objc("appleEventCode")
        pub get appleEventCode(): u32
        @objc("appleEventClassCode")
        pub get appleEventClassCode(): u32
        @objc("commandClassName")
        pub get commandClassName(): NSString
        @objc("argumentNames")
        pub get argumentNames(): NSArray
        @objc("typeForArgumentWithName:")
        pub typeForArgumentWithName(name: NSString): NSString
    }

    /// `NSScriptObjectSpecifier` — abstract base for the
    /// "where does this object live" tree AppleScript hands you.
    /// Concrete subclasses below carry the actual disambiguation
    /// strategy (property / index / range / unique-id / whose
    /// clause).
    @objc pub class NSScriptObjectSpecifier : NSObject {
        @objc("alloc")
        pub static alloc(): NSScriptObjectSpecifier
        @objc("objectsByEvaluatingSpecifier")
        pub objectsByEvaluatingSpecifier(): NSObject
        @objc("containerSpecifier")
        pub get containerSpecifier(): NSScriptObjectSpecifier
        @objc("setContainerSpecifier:")
        pub set containerSpecifier(s: NSScriptObjectSpecifier)
        @objc("containerIsObjectBeingTested")
        pub get containerIsObjectBeingTested(): bool
        @objc("containerIsRangeContainerObject")
        pub get containerIsRangeContainerObject(): bool
        @objc("key")
        pub get key(): NSString
        @objc("setKey:")
        pub set key(s: NSString)
        @objc("keyClassDescription")
        pub get keyClassDescription(): NSScriptClassDescription
        @objc("setKeyClassDescription:")
        pub set keyClassDescription(c: NSScriptClassDescription)
        @objc("descriptor")
        pub get descriptor(): NSAppleEventDescriptor
    }

    /// `NSPropertySpecifier` — "the `<key>` of `<container>`".
    @objc pub class NSPropertySpecifier : NSScriptObjectSpecifier {}

    /// `NSIndexSpecifier` — "the i-th element of `<container>`".
    @objc pub class NSIndexSpecifier : NSScriptObjectSpecifier {
        @objc("index")
        pub get index(): i64
        @objc("setIndex:")
        pub set index(i: i64)
    }

    /// `NSPositionalSpecifier` — "the position N relative to
    /// some other object".
    @objc pub class NSPositionalSpecifier : NSObject {
        @objc("position")
        pub get position(): u64
        @objc("objectSpecifier")
        pub get objectSpecifier(): NSScriptObjectSpecifier
    }

    /// `NSRangeSpecifier` — "items from `<start>` to `<end>`".
    @objc pub class NSRangeSpecifier : NSScriptObjectSpecifier {
        @objc("startSpecifier")
        pub get startSpecifier(): NSScriptObjectSpecifier
        @objc("setStartSpecifier:")
        pub set startSpecifier(s: NSScriptObjectSpecifier)
        @objc("endSpecifier")
        pub get endSpecifier(): NSScriptObjectSpecifier
        @objc("setEndSpecifier:")
        pub set endSpecifier(s: NSScriptObjectSpecifier)
    }

    /// `NSRelativeSpecifier` — "the item just before / after
    /// `<base>`".
    @objc pub class NSRelativeSpecifier : NSScriptObjectSpecifier {
        @objc("relativePosition")
        pub get relativePosition(): u64
        @objc("setRelativePosition:")
        pub set relativePosition(p: u64)
        @objc("baseSpecifier")
        pub get baseSpecifier(): NSScriptObjectSpecifier
        @objc("setBaseSpecifier:")
        pub set baseSpecifier(s: NSScriptObjectSpecifier)
    }

    /// `NSUniqueIDSpecifier` — "the item with this unique ID".
    @objc pub class NSUniqueIDSpecifier : NSScriptObjectSpecifier {
        @objc("uniqueID")
        pub get uniqueID(): NSObject
        @objc("setUniqueID:")
        pub set uniqueID(v: NSObject)
    }

    /// `NSWhoseSpecifier` — "the item whose `<test>` is true".
    /// Internally an NSPredicate-driven walk of the candidate
    /// objects.
    @objc pub class NSWhoseSpecifier : NSScriptObjectSpecifier {
        @objc("test")
        pub get test(): NSObject
        @objc("setTest:")
        pub set test(t: NSObject)
        @objc("startSubelementIdentifier")
        pub get startSubelementIdentifier(): u64
        @objc("endSubelementIdentifier")
        pub get endSubelementIdentifier(): u64
    }

    /// `NSScriptClassDescription` — sdef metadata for one
    /// scriptable Cocoa class: the attribute / relationship
    /// keys it exposes, the commands it supports, etc.
    @objc pub class NSScriptClassDescription : NSObject {
        @objc("classDescriptionForClass:")
        pub static classDescriptionForClass(cls: NSObject): NSScriptClassDescription

        @objc("suiteName")
        pub get suiteName(): NSString
        @objc("className")
        pub get className(): NSString
        @objc("implementationClassName")
        pub get implementationClassName(): NSString
        @objc("superclassDescription")
        pub get superclassDescription(): NSScriptClassDescription

        @objc("attributeKeys")
        pub get attributeKeys(): NSArray
        @objc("toOneRelationshipKeys")
        pub get toOneRelationshipKeys(): NSArray
        @objc("toManyRelationshipKeys")
        pub get toManyRelationshipKeys(): NSArray
        @objc("defaultSubcontainerAttributeKey")
        pub get defaultSubcontainerAttributeKey(): NSString
        @objc("isLocationRequiredToCreateForKey:")
        pub isLocationRequiredToCreateForKey(key: NSString): bool

        @objc("supportsCommand:")
        pub supportsCommand(d: NSScriptCommandDescription): bool
        @objc("selectorForCommand:")
        pub selectorForCommand(d: NSScriptCommandDescription): i64
    }

    /// `NSScriptSuiteRegistry` — process-wide registry of
    /// scriptable suites. The framework loads bundled
    /// `.scriptSuite` + `.scriptTerminology` pairs at startup;
    /// late-registered suites get added through
    /// `registerLoadedSuite:` / the lower-level
    /// `registerCommandDescription:` /
    /// `registerClassDescription:` calls.
    @objc pub class NSScriptSuiteRegistry : NSObject {
        @objc("sharedScriptSuiteRegistry")
        pub static shared(): NSScriptSuiteRegistry

        @objc("loadSuitesFromMainBundle")
        pub loadSuitesFromMainBundle()
        @objc("loadSuitesFromBundle:")
        pub loadSuitesFromBundle(b: NSObject)
        @objc("suiteNames")
        pub get suiteNames(): NSArray
        @objc("classDescriptionsInSuite:")
        pub classDescriptionsInSuite(name: NSString): NSDictionary
        @objc("commandDescriptionsInSuite:")
        pub commandDescriptionsInSuite(name: NSString): NSDictionary

        @objc("registerCommandDescription:")
        pub registerCommandDescription(d: NSScriptCommandDescription)
        @objc("registerClassDescription:")
        pub registerClassDescription(d: NSScriptClassDescription)
        @objc("classDescriptionWithAppleEventCode:")
        pub classDescriptionWithAppleEventCode(code: u32): NSScriptClassDescription
    }

    /// `NSScriptCoercionHandler` — type-coercion registry the
    /// scripting bridge consults when AppleScript passes a value
    /// of class A into a slot expecting class B. Register custom
    /// coercers through `+registerCoercer:selector:toConvertFromClass:toClass:`.
    @objc pub class NSScriptCoercionHandler : NSObject {
        @objc("sharedCoercionHandler")
        pub static shared(): NSScriptCoercionHandler

        @objc("coerceValue:toClass:")
        pub coerceValue(value: NSObject, toClass: NSObject): NSObject
        @objc("registerCoercer:selector:toConvertFromClass:toClass:")
        pub registerCoercer(
            coercer: NSObject, selector: i64,
            toConvertFromClass: NSObject, toClass: NSObject
        )
    }

    /// `NSScriptKeyValueCoding` — protocol the scripting bridge
    /// dispatches through. Most objects pick up the default
    /// implementation from NSObject's KVC, but custom containers
    /// override these methods to support
    /// `insertObject:in<Key>AtIndex:` etc.
    @objc pub interface NSScriptKeyValueCoding {
        valueAtIndex?(forKey: NSString, i: u64): NSObject
        valueWithName?(forKey: NSString, name: NSString): NSObject
        valueWithUniqueID?(forKey: NSString, uid: NSObject): NSObject
        insertValue?(value: NSObject, atIndex: u64, inPropertyWithKey: NSString)
        removeValueAtIndex?(i: u64, fromPropertyWithKey: NSString)
        replaceValueAtIndex?(
            i: u64, inPropertyWithKey: NSString, withValue: NSObject
        )
        insertValueInPropertyWithKey?(value: NSObject, key: NSString)
    }

    /// `NSScriptObjectSpecifierError` — error keys an object
    /// can stash on a specifier evaluation to explain why the
    /// scripting bridge couldn't resolve it. Mostly used by
    /// container implementations during fault propagation.
    @objc pub interface NSScriptObjectSpecifierError {
        @objc("scriptingValueForSpecifier:")
        scriptingValueForSpecifier(specifier: NSScriptObjectSpecifier): NSObject
    }
}
