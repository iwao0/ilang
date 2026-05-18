// System / settings / notifications bindings:
//
//   * NSNotificationCenter / NSDistributedNotificationCenter —
//     in-process and cross-process publish/subscribe.
//   * NSUserDefaults — app preferences store, backed by a plist
//     in `~/Library/Preferences/`.
//   * NSProcessInfo — running-process metadata (arguments,
//     environment, processor / memory counts, thermal state).
//   * NSLocale — language / region / formatting locale lookup
//     used by NSDateFormatter / NSNumberFormatter and friends.

use core { NSObject, NSString, NSArray, NSDictionary, NSData,
           makeObjectBlock }
use network { NSURL }

// NSProcessInfoThermalState — system thermal pressure. `nominal`
// means everything's fine; `serious` / `critical` are signals to
// scale work back / pause it entirely to stop the CPU throttling
// (or the user's lap from catching fire).
pub enum ProcessInfoThermalState: i64 {
    nominal  = 0
    fair     = 1
    serious  = 2
    critical = 3
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSNotification` — the value Cocoa hands delegate callbacks
    /// (`applicationDidFinishLaunching:`, `windowDidResize:`, etc.).
    /// `name` identifies what happened, `object` is whatever
    /// posted it (the NSApplication, NSWindow, etc.), `userInfo`
    /// is an optional payload dictionary. Lives here alongside
    /// the centre that dispatches them.
    @objc pub class NSNotification : NSObject {
        @objc("name")
        pub get name(): NSString
        @objc("object")
        pub get notificationObject(): NSObject
        @objc("userInfo")
        pub get userInfo(): NSDictionary
    }

    /// `NSNotificationCenter` — in-process pub/sub. The selector-
    /// based `addObserver:selector:name:object:` is the canonical
    /// form for delegate-style code; for block-based receivers use
    /// `addObserverForName:object:queue:usingBlock:` (returns an
    /// opaque observer token to hand to `removeObserver:`).
    @objc pub class NSNotificationCenter : NSObject {
        @objc("defaultCenter")
        pub static defaultCenter(): NSNotificationCenter

        @objc("addObserver:selector:name:object:")
        pub addObserverWithSelector(
            observer: NSObject, selector: i64,
            name: NSString, object: NSObject
        )

        @objc("addObserverForName:object:queue:usingBlock:")
        _addObserverWithBlock(
            name: NSString, object: NSObject, queue: NSObject, block: i64
        ): NSObject

        /// Block-based subscription. `body` runs every time a
        /// notification matching `(name, object)` posts; both
        /// arguments may be `nil` (use `cocoa.nil`) to match any
        /// name / any sender. `queue` controls which OperationQueue
        /// the block runs on — pass `cocoa.nil` to use the posting
        /// thread. Returns an opaque observer token; hand it to
        /// `removeObserver:` to unsubscribe.
        pub addObserver(
            name: NSString, object: NSObject, queue: NSObject,
            body: fn(i64)
        ): NSObject {
            this._addObserverWithBlock(name, object, queue, makeObjectBlock(body))
        }

        @objc("removeObserver:")
        pub removeObserver(observer: NSObject)
        @objc("removeObserver:name:object:")
        pub removeObserverForName(
            observer: NSObject, name: NSString, object: NSObject
        )

        @objc("postNotification:")
        pub postNotification(n: NSNotification)
        @objc("postNotificationName:object:")
        pub postNotificationName(name: NSString, object: NSObject)
        @objc("postNotificationName:object:userInfo:")
        pub postNotificationNameWithInfo(
            name: NSString, object: NSObject, userInfo: NSDictionary
        )
    }

    /// `NSDistributedNotificationCenter` — cross-process variant.
    /// Same API surface as NSNotificationCenter but posts through
    /// `distnoted` so subscribers in other processes can react
    /// (system-wide preferences changes, Spotlight indexing, etc).
    /// Deliveries are best-effort and may coalesce — don't use it
    /// for ordered or guaranteed-delivery semantics.
    @objc pub class NSDistributedNotificationCenter : NSObject {
        @objc("defaultCenter")
        pub static defaultCenter(): NSDistributedNotificationCenter

        @objc("addObserver:selector:name:object:")
        pub addObserverWithSelector(
            observer: NSObject, selector: i64,
            name: NSString, object: NSString
        )

        @objc("removeObserver:")
        pub removeObserver(observer: NSObject)
        @objc("removeObserver:name:object:")
        pub removeObserverForName(
            observer: NSObject, name: NSString, object: NSString
        )

        @objc("postNotificationName:object:")
        pub postNotificationName(name: NSString, object: NSString)
        @objc("postNotificationName:object:userInfo:")
        pub postNotificationNameWithInfo(
            name: NSString, object: NSString, userInfo: NSDictionary
        )
    }

    /// `NSUserDefaults` — preferences store. `standardUserDefaults`
    /// is the app's main bundle's prefs; instantiate
    /// `initWithSuiteName:` for a shared group container.
    /// Setting a key writes to the in-memory cache and a
    /// background thread flushes to disk; `synchronize()` forces
    /// a flush (rarely needed now that the system auto-flushes).
    @objc pub class NSUserDefaults : NSObject {
        @objc("standardUserDefaults")
        pub static standard(): NSUserDefaults
        @objc("alloc")
        pub static alloc(): NSUserDefaults
        @objc("init")
        pub init(): NSUserDefaults
        @objc("initWithSuiteName:")
        pub initWithSuiteName(name: NSString): NSUserDefaults

        // ─── Typed getters ────────────────────────────────────
        @objc("objectForKey:")
        pub objectForKey(k: NSString): NSObject
        @objc("stringForKey:")
        pub stringForKey(k: NSString): NSString
        @objc("integerForKey:")
        pub integerForKey(k: NSString): i64
        @objc("doubleForKey:")
        pub doubleForKey(k: NSString): f64
        @objc("floatForKey:")
        pub floatForKey(k: NSString): f32
        @objc("boolForKey:")
        pub boolForKey(k: NSString): bool
        @objc("dataForKey:")
        pub dataForKey(k: NSString): NSData
        @objc("arrayForKey:")
        pub arrayForKey(k: NSString): NSArray
        @objc("dictionaryForKey:")
        pub dictionaryForKey(k: NSString): NSDictionary
        @objc("URLForKey:")
        pub urlForKey(k: NSString): NSURL
        @objc("stringArrayForKey:")
        pub stringArrayForKey(k: NSString): NSArray

        // ─── Typed setters ────────────────────────────────────
        @objc("setObject:forKey:")
        pub setObject(value: NSObject, forKey: NSString)
        @objc("setInteger:forKey:")
        pub setInteger(value: i64, forKey: NSString)
        @objc("setDouble:forKey:")
        pub setDouble(value: f64, forKey: NSString)
        @objc("setFloat:forKey:")
        pub setFloat(value: f32, forKey: NSString)
        @objc("setBool:forKey:")
        pub setBool(value: bool, forKey: NSString)
        @objc("setURL:forKey:")
        pub setURL(value: NSURL, forKey: NSString)

        @objc("removeObjectForKey:")
        pub removeObjectForKey(k: NSString)
        @objc("synchronize")
        pub synchronize(): bool

        @objc("registerDefaults:")
        pub registerDefaults(d: NSDictionary)
        @objc("dictionaryRepresentation")
        pub get dictionaryRepresentation(): NSDictionary
        @objc("persistentDomainForName:")
        pub persistentDomainForName(name: NSString): NSDictionary
        @objc("removePersistentDomainForName:")
        pub removePersistentDomainForName(name: NSString)
    }

    /// `NSProcessInfo` — running-process metadata. The shared
    /// instance is the only way in; everything else hangs off it
    /// as a read-only property.
    @objc pub class NSProcessInfo : NSObject {
        @objc("processInfo")
        pub static processInfo(): NSProcessInfo

        @objc("arguments")
        pub get arguments(): NSArray
        @objc("environment")
        pub get environment(): NSDictionary
        @objc("processName")
        pub get processName(): NSString
        @objc("setProcessName:")
        pub set processName(s: NSString)
        @objc("hostName")
        pub get hostName(): NSString
        @objc("processIdentifier")
        pub get processIdentifier(): i32
        @objc("globallyUniqueString")
        pub get globallyUniqueString(): NSString

        @objc("physicalMemory")
        pub get physicalMemory(): u64
        @objc("processorCount")
        pub get processorCount(): u64
        @objc("activeProcessorCount")
        pub get activeProcessorCount(): u64
        @objc("systemUptime")
        pub get systemUptime(): f64
        @objc("operatingSystemVersionString")
        pub get operatingSystemVersionString(): NSString

        @objc("thermalState")
        pub get thermalState(): ProcessInfoThermalState
        @objc("isLowPowerModeEnabled")
        pub get isLowPowerModeEnabled(): bool

        @objc("isMacCatalystApp")
        pub get isMacCatalystApp(): bool
        @objc("isiOSAppOnMac")
        pub get isiOSAppOnMac(): bool
    }

    /// `NSLocale` — represents a language + region + formatting
    /// conventions tuple. Hand one to `NSDateFormatter` /
    /// `NSNumberFormatter.locale` to override the system default.
    /// For raw POSIX behaviour (`en_US_POSIX`) the value is what
    /// you want for log files / wire formats so date / number
    /// formatting doesn't shift with the user's region.
    @objc pub class NSLocale : NSObject {
        @objc("currentLocale")
        pub static current(): NSLocale
        @objc("systemLocale")
        pub static system(): NSLocale
        @objc("autoupdatingCurrentLocale")
        pub static autoupdatingCurrent(): NSLocale
        @objc("localeWithLocaleIdentifier:")
        pub static localeWithIdentifier(id: NSString): NSLocale
        @objc("alloc")
        pub static alloc(): NSLocale
        @objc("initWithLocaleIdentifier:")
        pub initWithIdentifier(id: NSString): NSLocale

        @objc("localeIdentifier")
        pub get localeIdentifier(): NSString
        @objc("languageCode")
        pub get languageCode(): NSString
        @objc("countryCode")
        pub get countryCode(): NSString
        @objc("scriptCode")
        pub get scriptCode(): NSString
        @objc("variantCode")
        pub get variantCode(): NSString
        @objc("currencyCode")
        pub get currencyCode(): NSString
        @objc("currencySymbol")
        pub get currencySymbol(): NSString
        @objc("decimalSeparator")
        pub get decimalSeparator(): NSString
        @objc("groupingSeparator")
        pub get groupingSeparator(): NSString
        @objc("usesMetricSystem")
        pub get usesMetricSystem(): bool

        @objc("preferredLanguages")
        pub static preferredLanguages(): NSArray
        @objc("availableLocaleIdentifiers")
        pub static availableLocaleIdentifiers(): NSArray
        @objc("ISOCountryCodes")
        pub static isoCountryCodes(): NSArray
        @objc("ISOLanguageCodes")
        pub static isoLanguageCodes(): NSArray
        @objc("ISOCurrencyCodes")
        pub static isoCurrencyCodes(): NSArray

        /// Look up the locale's display name for a candidate
        /// identifier — e.g. `displayNameForIdentifier("en_US")`
        /// on a Japanese locale returns `"英語 (アメリカ合衆国)"`.
        @objc("displayNameForKey:value:")
        pub displayNameForKey(key: NSString, value: NSString): NSString
    }

    /// `NSTask` — launch and control a sub-process. The
    /// `executableURL` / `arguments` / `currentDirectoryURL`
    /// properties set up the spawn, `standardInput` /
    /// `standardOutput` / `standardError` redirect the stdio
    /// streams (assign an NSFileHandle or NSPipe to each), and
    /// `launchAndReturnError:` actually starts it. macOS only.
    @objc pub class NSTask : NSObject {
        @objc("alloc")
        pub static alloc(): NSTask
        @objc("init")
        pub init(): NSTask

        @objc("launchedTaskWithExecutableURL:arguments:error:terminationHandler:")
        pub static launchedTaskWithExecutableURL(
            url: NSURL, arguments: NSArray, error: i64,
            terminationHandler: i64
        ): NSTask

        @objc("executableURL")
        pub get executableURL(): NSURL
        @objc("setExecutableURL:")
        pub set executableURL(u: NSURL)
        @objc("arguments")
        pub get arguments(): NSArray
        @objc("setArguments:")
        pub set arguments(args: NSArray)
        @objc("environment")
        pub get environment(): NSDictionary
        @objc("setEnvironment:")
        pub set environment(env: NSDictionary)
        @objc("currentDirectoryURL")
        pub get currentDirectoryURL(): NSURL
        @objc("setCurrentDirectoryURL:")
        pub set currentDirectoryURL(u: NSURL)

        @objc("standardInput")
        pub get standardInput(): NSObject
        @objc("setStandardInput:")
        pub set standardInput(s: NSObject)
        @objc("standardOutput")
        pub get standardOutput(): NSObject
        @objc("setStandardOutput:")
        pub set standardOutput(s: NSObject)
        @objc("standardError")
        pub get standardError(): NSObject
        @objc("setStandardError:")
        pub set standardError(s: NSObject)

        @objc("launchAndReturnError:")
        pub launchAndReturnError(error: i64): bool
        @objc("interrupt")
        pub interrupt()
        @objc("terminate")
        pub terminate()
        @objc("suspend")
        pub suspend(): bool
        @objc("resume")
        pub resume(): bool
        @objc("waitUntilExit")
        pub waitUntilExit()

        @objc("processIdentifier")
        pub get processIdentifier(): i32
        @objc("isRunning")
        pub get isRunning(): bool
        @objc("terminationStatus")
        pub get terminationStatus(): i32
        @objc("terminationReason")
        pub get terminationReason(): i64
    }

    /// `NSUUID` — RFC 4122 UUID. The default initialiser
    /// (`UUID()`) mints a random v4 UUID; `initWithUUIDString:`
    /// parses a canonical string like
    /// `"550E8400-E29B-41D4-A716-446655440000"`. Use
    /// `UUIDString` to round-trip back to text. Lives in `system`
    /// because the runtime supplies the entropy.
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
    }
}
