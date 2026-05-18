// Foundation bindings — the ObjC base layer (NSObject / NSString /
// collections / dates / URLs) plus the ilang ↔ ObjC plumbing that
// every higher-level binding leans on. Doesn't touch AppKit; safe
// to import on macOS server contexts that don't open a window.
//
// Imported via the `cocoa` umbrella module (`use cocoa { ... }`)
// or directly with `use foundation { ... }`.

// ─── Foundation geometry types (HFA-passed) ──────────────────────
// NSPoint / NSSize / NSRect / NSVector are plain f64 structs. The
// AArch64 ABI rides them through v0..v3 (HFA path) when passed to
// objc_msgSend — same layout C uses, no shim needed.
pub struct NSPoint  { x: f64; y: f64 }
pub struct NSSize   { width: f64; height: f64 }
pub struct NSRect   { origin: NSPoint; size: NSSize }
// CGVector — `(dx, dy)` pair Foundation uses for velocities,
// gravity, impulses, etc. Same layout as `NSSize` but the field
// names differ; a dedicated type keeps call sites readable.
pub struct NSVector { dx: f64; dy: f64 }

// NSRange — Foundation's `(location, length)` pair. NSUInteger
// fields, so 8 bytes each on 64-bit. Passed by value through the
// integer-register ABI. Build with `makeRange(loc, len)`.
pub struct NSRange { location: u64; length: u64 }

// NSRegularExpressionOptions — bit flags controlling how the
// regex pattern is compiled. Mirrors the
// `NSRegularExpressionOptions` enum from `<Foundation/NSRegularExpression.h>`.
@flags
pub enum RegexOptions: u64 {
    none                       = 0x00
    caseInsensitive            = 0x01
    allowCommentsAndWhitespace = 0x02
    ignoreMetacharacters       = 0x04
    dotMatchesLineSeparators   = 0x08
    anchorsMatchLines          = 0x10
    useUnixLineSeparators      = 0x20
    useUnicodeWordBoundaries   = 0x40
}

// NSMatchingOptions — bit flags controlling a single regex match
// operation (anchoring, partial-match behaviour, …). The
// progress / completion bits are only meaningful for the
// block-taking enumerator API, which we don't bind here.
@flags
pub enum MatchOptions: u64 {
    none                     = 0x00
    reportProgress           = 0x01
    reportCompletion         = 0x02
    anchored                 = 0x04
    withTransparentBounds    = 0x08
    withoutAnchoringBounds   = 0x10
}

// NSDateFormatterStyle — the locale-aware default presets you
// pick by assigning to `dateStyle` / `timeStyle`. Setting
// `dateFormat` directly to a Unicode TR35 pattern overrides
// these.
pub enum DateFormatterStyle: u64 {
    none   = 0
    short  = 1
    medium = 2
    long   = 3
    full   = 4
}

// NSCalendarUnit — bit set used by `[NSCalendar
// components:fromDate:]` to say which fields the returned
// `NSDateComponents` should be populated with. Combine with `|`.
@flags
pub enum CalendarUnit: u64 {
    none       = 0x00
    era        = 0x02
    year       = 0x04
    month      = 0x08
    day        = 0x10
    hour       = 0x20
    minute     = 0x40
    second     = 0x80
    weekday    = 0x200
    quarter    = 0x800
    weekOfYear = 0x4000
    yearForWeekOfYear = 0x8000
    nanosecond = 0x10000
    calendar   = 0x100000
    timeZone   = 0x200000
}

// NSJSONReadingOptions — bit flags controlling
// `JSONObjectWithData:options:error:`. `mutableContainers` /
// `mutableLeaves` ask for NSMutableArray / NSMutableString
// instances in the parsed tree; `fragmentsAllowed` permits the
// JSON to be a bare scalar instead of an array or object.
@flags
pub enum JSONReadingOptions: u64 {
    none              = 0x00
    mutableContainers = 0x01
    mutableLeaves     = 0x02
    fragmentsAllowed  = 0x04
}

// NSJSONWritingOptions — bit flags controlling
// `dataWithJSONObject:options:error:`. `prettyPrinted` emits
// indented output; `sortedKeys` orders dictionaries by their
// keys (useful for reproducible builds / diffing); `withoutEscapingSlashes`
// emits `/` as itself instead of `\/`.
@flags
pub enum JSONWritingOptions: u64 {
    none                  = 0x00
    prettyPrinted         = 0x01
    sortedKeys            = 0x02
    fragmentsAllowed      = 0x04
    withoutEscapingSlashes = 0x08
}

// NSPropertyListFormat — output / input format for
// NSPropertyListSerialization. `xml` is the human-readable form
// macOS' `defaults` command emits; `binary` is the compact form
// the system uses for in-memory representations.
pub enum PropertyListFormat: u64 {
    openStep = 1
    xml      = 100
    binary   = 200
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// Root of the Cocoa class hierarchy. Bound (not subclassed)
    /// so users can call e.g. `obj.release()` on any returned
    /// instance.
    @objc pub class NSObject {
        @objc("release")
        pub release()
        @objc("retain")
        pub retain(): NSObject
        @objc("hash")
        pub get hash(): i64

        /// Wrap a raw ObjC `id` (the `i64` pointer value) into an
        /// `NSObject` instance without going through `alloc + init`.
        /// Bridges to the @objc-desugar's hidden `__wrap_handle`
        /// helper, which simply stores `h` into the object's
        /// `handle` field. Use this for `nil` (handle = 0) or for
        /// adopting an `id` returned from a C-side bridge.
        pub static wrap(h: i64): NSObject {
            NSObject.__wrap_handle(h)
        }
    }

    /// `NSString` — Cocoa's UTF-16 string type. ilang code works
    /// in UTF-8, so the user-facing round-trip is
    /// `nsstr(s) ↔ ilstr(ns)`; the raw `*const char` selectors
    /// (`stringWithUTF8String:` and `UTF8String`) stay
    /// module-private and are only invoked through those
    /// helpers.
    @objc pub class NSString : NSObject {
        @objc("stringWithUTF8String:")
        static _fromCStr(s: *const char): NSString
        @objc("UTF8String")
        _utf8String(): *const char
        @objc("length")
        pub get length(): i64
        @objc("isEqualToString:")
        pub isEqualToString(other: NSString): bool

        // ─── out-parameter wrapper pattern ──────────────────────
        // The raw ObjC selector takes three `NSUInteger *` slots
        // that the caller fills in. We bind the selector as a
        // non-pub method whose params are raw `*u64` (legal inside
        // `@extern(ObjC)` — same C-pointer surface as the rest of
        // the block), then expose a `pub` wrapper that allocates
        // three local `u64`s, takes their addresses via `&`, and
        // returns the three values as a tuple. The raw method is
        // not visible outside the class.
        @objc("getLineStart:end:contentsEnd:forRange:")
        _getLineBounds(start: *u64, end: *u64, contentsEnd: *u64, range: NSRange)

        /// `(start, end, contentsEnd)` of the line containing
        /// `range`. `end` includes the line terminator;
        /// `contentsEnd` is just before it. Mirrors
        /// `-[NSString getLineStart:end:contentsEnd:forRange:]`.
        pub lineBounds(range: NSRange): (u64, u64, u64) {
            let s: u64 = 0
            let e: u64 = 0
            let c: u64 = 0
            this._getLineBounds(&s, &e, &c, range)
            (s, e, c)
        }

        // ─── Predicates ─────────────────────────────────────────
        @objc("hasPrefix:")
        pub hasPrefix(prefix: NSString): bool
        @objc("hasSuffix:")
        pub hasSuffix(suffix: NSString): bool
        @objc("containsString:")
        pub containsString(s: NSString): bool

        // ─── Case conversion ───────────────────────────────────
        @objc("lowercaseString")
        pub get lowercaseString(): NSString
        @objc("uppercaseString")
        pub get uppercaseString(): NSString
        @objc("capitalizedString")
        pub get capitalizedString(): NSString

        // ─── Substring / append ────────────────────────────────
        @objc("substringFromIndex:")
        pub substringFromIndex(i: u64): NSString
        @objc("substringToIndex:")
        pub substringToIndex(i: u64): NSString
        @objc("substringWithRange:")
        pub substringWithRange(range: NSRange): NSString
        @objc("stringByAppendingString:")
        pub stringByAppendingString(s: NSString): NSString

        // ─── Find / replace / split / trim ─────────────────────
        @objc("stringByReplacingOccurrencesOfString:withString:")
        pub stringByReplacing(target: NSString, replacement: NSString): NSString
        @objc("componentsSeparatedByString:")
        pub componentsSeparatedByString(separator: NSString): NSArray
        @objc("componentsSeparatedByCharactersInSet:")
        pub componentsSeparatedByCharactersInSet(set: NSCharacterSet): NSArray
        @objc("stringByTrimmingCharactersInSet:")
        pub stringByTrimmingCharactersInSet(set: NSCharacterSet): NSString

        // ─── Parsing (best-effort, returns 0 on a non-number) ──
        @objc("intValue")
        pub get intValue(): i32
        @objc("integerValue")
        pub get integerValue(): i64
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("floatValue")
        pub get floatValue(): f32
        @objc("boolValue")
        pub get boolValue(): bool
    }

    /// `NSMutableString` — appendable / editable variant of
    /// NSString. `string()` factory mints an empty one; the
    /// mutate-in-place methods all return `void` (no chaining).
    @objc pub class NSMutableString : NSString {
        @objc("string")
        pub static string(): NSMutableString

        @objc("appendString:")
        pub appendString(s: NSString)
        @objc("insertString:atIndex:")
        pub insertString(s: NSString, atIndex: u64)
        @objc("deleteCharactersInRange:")
        pub deleteCharactersInRange(range: NSRange)
        @objc("replaceCharactersInRange:withString:")
        pub replaceCharactersInRange(range: NSRange, with: NSString)
        @objc("setString:")
        pub setString(s: NSString)
    }

    /// `NSNumber` — boxed numeric value. Read back as `intValue`,
    /// `doubleValue`, or `boolValue` depending on what's inside.
    @objc pub class NSNumber : NSObject {
        @objc("numberWithLongLong:")
        pub static fromI64(v: i64): NSNumber
        @objc("numberWithDouble:")
        pub static fromF64(v: f64): NSNumber
        @objc("numberWithBool:")
        pub static fromBool(v: bool): NSNumber

        @objc("longLongValue")
        pub get i64Value(): i64
        @objc("doubleValue")
        pub get f64Value(): f64
        @objc("boolValue")
        pub get boolValue(): bool
    }

    /// `NSArray` — immutable ordered collection of `id`. Returned
    /// from many Foundation APIs (`NSFileManager.contentsOfDirectory…`,
    /// `[NSString componentsSeparatedByString:]`, etc.). For
    /// construction use `NSMutableArray`.
    @objc pub class NSArray : NSObject {
        @objc("count")
        pub get count(): i64
        @objc("objectAtIndex:")
        pub objectAtIndex(i: i64): NSObject
        @objc("containsObject:")
        pub containsObject(o: NSObject): bool
    }

    /// `NSMutableArray` — array that can be appended to. Initialise
    /// with `alloc().init()` or use the `array` class method.
    @objc pub class NSMutableArray : NSArray {
        @objc("alloc")
        pub static alloc(): NSMutableArray
        @objc("init")
        pub init(): NSMutableArray
        @objc("array")
        pub static array(): NSMutableArray

        @objc("addObject:")
        pub addObject(o: NSObject)
        @objc("removeObjectAtIndex:")
        pub removeObjectAtIndex(i: i64)
        @objc("insertObject:atIndex:")
        pub insertObject(o: NSObject, atIndex: i64)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSDictionary` — immutable id→id map.
    @objc pub class NSDictionary : NSObject {
        @objc("count")
        pub get count(): i64
        @objc("objectForKey:")
        pub objectForKey(k: NSObject): NSObject
        @objc("allKeys")
        pub get allKeys(): NSArray
    }

    /// `NSMutableDictionary` — dictionary you can write to. Keys
    /// should conform to NSCopying (NSString / NSNumber are fine).
    @objc pub class NSMutableDictionary : NSDictionary {
        @objc("alloc")
        pub static alloc(): NSMutableDictionary
        @objc("init")
        pub init(): NSMutableDictionary
        @objc("dictionary")
        pub static dictionary(): NSMutableDictionary

        @objc("setObject:forKey:")
        pub setObject(o: NSObject, forKey: NSObject)
        @objc("removeObjectForKey:")
        pub removeObjectForKey(k: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSData` — immutable byte buffer. The raw selectors traffic
    /// in `*const char`; user code drives them through the pub
    /// wrappers that take / return ilang `u8[]`. The wrappers
    /// copy bytes both ways (NSData's `bytes` pointer is owned by
    /// the autoreleased instance and shouldn't outlive it).
    @objc pub class NSData : NSObject {
        @objc("dataWithBytes:length:")
        static _fromBytes(bytes: *const char, length: i64): NSData
        @objc("length")
        pub get length(): i64
        @objc("bytes")
        _bytes(): *const char

        /// Build an `NSData` from a byte buffer (copies the bytes).
        pub static fromBytes(buf: u8[]): NSData {
            let p: *u8 = buf
            NSData._fromBytes(p as *char, buf.length)
        }

        /// Copy this NSData's bytes into a fresh `u8[]`. Safe to
        /// keep the result past the autorelease pool drain.
        pub bytes(): u8[] {
            bytesFromBuffer(this._bytes() as *void, this.length as size_t)
        }

        /// Load file contents into an NSData. The `error:` slot
        /// is discarded — pass `0` to ignore failures (the result
        /// has a nil-handle wrapper on error).
        @objc("dataWithContentsOfFile:")
        pub static dataWithContentsOfFile(path: NSString): NSData
        @objc("writeToFile:atomically:")
        pub writeToFile(path: NSString, atomically: bool): bool
    }

    /// `NSMutableData` — appendable variant of NSData. Useful for
    /// building binary buffers piecewise before handing them to a
    /// `writeToFile:` or a `dataTaskWithRequest:`-style API.
    @objc pub class NSMutableData : NSData {
        @objc("data")
        pub static data(): NSMutableData
        @objc("dataWithCapacity:")
        pub static dataWithCapacity(capacity: u64): NSMutableData

        @objc("appendBytes:length:")
        _appendBytes(bytes: *const char, length: u64)

        /// Append `buf` to the receiver (copies the bytes in).
        pub appendBytesFromBuffer(buf: u8[]) {
            let p: *u8 = buf
            this._appendBytes(p as *char, buf.length as u64)
        }

        @objc("setLength:")
        pub setLength(length: u64)
        @objc("increaseLengthBy:")
        pub increaseLengthBy(extra: u64)
    }

    /// `NSDate` — a moment in time. `date` is "now"; the interval
    /// methods give you the offset in seconds.
    @objc pub class NSDate : NSObject {
        @objc("date")
        pub static now(): NSDate
        @objc("dateWithTimeIntervalSince1970:")
        pub static dateWithTimeIntervalSince1970(secs: f64): NSDate
        @objc("dateWithTimeIntervalSinceNow:")
        pub static dateWithTimeIntervalSinceNow(secs: f64): NSDate
        @objc("distantFuture")
        pub static distantFuture(): NSDate
        @objc("distantPast")
        pub static distantPast(): NSDate

        @objc("timeIntervalSince1970")
        pub get timeIntervalSince1970(): f64
        @objc("timeIntervalSinceNow")
        pub get timeIntervalSinceNow(): f64
        @objc("timeIntervalSinceDate:")
        pub timeIntervalSinceDate(other: NSDate): f64

        @objc("dateByAddingTimeInterval:")
        pub dateByAddingTimeInterval(secs: f64): NSDate
        @objc("isEqualToDate:")
        pub isEqualToDate(other: NSDate): bool
        @objc("earlierDate:")
        pub earlierDate(other: NSDate): NSDate
        @objc("laterDate:")
        pub laterDate(other: NSDate): NSDate
    }

    /// `NSDateFormatter` — string ↔ date conversion. Mutate
    /// `dateStyle` / `timeStyle` for a locale-aware default
    /// (`short` / `medium` / `long` / `full`) or set `dateFormat`
    /// directly for a Unicode TR35 pattern like `"yyyy-MM-dd"`.
    /// Set `locale` to an `NSLocale` if the system locale isn't
    /// what you want (e.g. for log files: `en_US_POSIX`).
    @objc pub class NSDateFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateFormatter
        @objc("init")
        pub init(): NSDateFormatter

        @objc("dateFormat")
        pub get dateFormat(): NSString
        @objc("setDateFormat:")
        pub set dateFormat(s: NSString)

        @objc("dateStyle")
        pub get dateStyle(): DateFormatterStyle
        @objc("setDateStyle:")
        pub set dateStyle(v: DateFormatterStyle)
        @objc("timeStyle")
        pub get timeStyle(): DateFormatterStyle
        @objc("setTimeStyle:")
        pub set timeStyle(v: DateFormatterStyle)

        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)
        @objc("timeZone")
        pub get timeZone(): NSObject
        @objc("setTimeZone:")
        pub set timeZone(tz: NSObject)

        @objc("stringFromDate:")
        pub stringFromDate(d: NSDate): NSString
        @objc("dateFromString:")
        pub dateFromString(s: NSString): NSDate
    }

    /// `NSCalendar` — Gregorian / Japanese / ISO8601 / etc.
    /// calendar arithmetic and component extraction. Use
    /// `currentCalendar()` for the user's locale or
    /// `calendarWithIdentifier:` to pin one explicitly.
    @objc pub class NSCalendar : NSObject {
        @objc("currentCalendar")
        pub static currentCalendar(): NSCalendar
        @objc("autoupdatingCurrentCalendar")
        pub static autoupdatingCurrentCalendar(): NSCalendar
        @objc("calendarWithIdentifier:")
        pub static calendarWithIdentifier(ident: NSString): NSCalendar

        @objc("components:fromDate:")
        pub componentsFromDate(units: CalendarUnit, date: NSDate): NSDateComponents
        @objc("dateFromComponents:")
        pub dateFromComponents(comps: NSDateComponents): NSDate
        @objc("dateByAddingComponents:toDate:options:")
        pub dateByAddingComponents(
            comps: NSDateComponents, toDate: NSDate, options: u64
        ): NSDate
        @objc("isDateInToday:")
        pub isDateInToday(d: NSDate): bool
        @objc("isDateInYesterday:")
        pub isDateInYesterday(d: NSDate): bool
        @objc("isDateInTomorrow:")
        pub isDateInTomorrow(d: NSDate): bool
    }

    /// `NSDateComponents` — bag of optional calendar fields. Set
    /// the ones you care about; the rest are sentinel values
    /// (`NSDateComponentUndefined`, which surfaces as the
    /// platform-specific "missing" code — most Foundation APIs
    /// just ignore unset components).
    @objc pub class NSDateComponents : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateComponents
        @objc("init")
        pub init(): NSDateComponents

        @objc("year")
        pub get year(): i64
        @objc("setYear:")
        pub set year(v: i64)
        @objc("month")
        pub get month(): i64
        @objc("setMonth:")
        pub set month(v: i64)
        @objc("day")
        pub get day(): i64
        @objc("setDay:")
        pub set day(v: i64)
        @objc("hour")
        pub get hour(): i64
        @objc("setHour:")
        pub set hour(v: i64)
        @objc("minute")
        pub get minute(): i64
        @objc("setMinute:")
        pub set minute(v: i64)
        @objc("second")
        pub get second(): i64
        @objc("setSecond:")
        pub set second(v: i64)
        @objc("weekday")
        pub get weekday(): i64
        @objc("setWeekday:")
        pub set weekday(v: i64)
    }

    /// `NSURL` — URL value. Use `fromString` for `http://...`
    /// style URLs, `fromFilePath` for filesystem paths.
    @objc pub class NSURL : NSObject {
        @objc("URLWithString:")
        pub static fromString(s: NSString): NSURL
        @objc("fileURLWithPath:")
        pub static fromFilePath(s: NSString): NSURL
        @objc("absoluteString")
        pub get absoluteString(): NSString
        @objc("path")
        pub get path(): NSString
    }

    /// `NSNotification` — the value Cocoa hands delegate callbacks
    /// (`applicationDidFinishLaunching:`, `windowDidResize:`, etc.).
    /// `name` identifies what happened, `object` is whatever
    /// posted it (the NSApplication, NSWindow, etc.), `userInfo`
    /// is an optional payload dictionary.
    @objc pub class NSNotification : NSObject {
        @objc("name")
        pub get name(): NSString
        @objc("object")
        pub get notificationObject(): NSObject
        @objc("userInfo")
        pub get userInfo(): NSDictionary
    }

    /// `NSError` — error value passed through Cocoa APIs that fail
    /// in non-fatal ways. Most factories that produce one take an
    /// `NSError **` out-parameter; the ilang side exposes it as a
    /// raw `i64` slot so callers can pass `0` to ignore the error
    /// or hold one address-of-`i64` value (`&local as *void as i64`)
    /// to receive it. The accessors below read from a wrapper you
    /// already have in hand.
    @objc pub class NSError : NSObject {
        @objc("code")
        pub get code(): i64
        @objc("domain")
        pub get domain(): NSString
        @objc("localizedDescription")
        pub get localizedDescription(): NSString
        @objc("userInfo")
        pub get userInfo(): NSDictionary
    }

    /// `NSFileManager` — filesystem operations. `defaultManager`
    /// returns the process-wide shared instance. Methods that take
    /// an `NSError **` out-parameter expose it as a raw `i64`
    /// (`0` discards). Returned `NSArray`s of names are UTF-8
    /// strings reachable via `ilstr(arr.objectAtIndex(i) as NSString)`.
    @objc pub class NSFileManager : NSObject {
        @objc("defaultManager")
        pub static defaultManager(): NSFileManager

        @objc("fileExistsAtPath:")
        pub fileExistsAtPath(path: NSString): bool
        @objc("contentsOfDirectoryAtPath:error:")
        pub contentsOfDirectoryAtPath(path: NSString, error: i64): NSArray
        @objc("createDirectoryAtPath:withIntermediateDirectories:attributes:error:")
        pub createDirectoryAtPath(
            path: NSString, intermediates: bool, attrs: NSObject, error: i64
        ): bool
        @objc("removeItemAtPath:error:")
        pub removeItemAtPath(path: NSString, error: i64): bool
        @objc("copyItemAtPath:toPath:error:")
        pub copyItemAtPath(src: NSString, dst: NSString, error: i64): bool
        @objc("moveItemAtPath:toPath:error:")
        pub moveItemAtPath(src: NSString, dst: NSString, error: i64): bool
        @objc("attributesOfItemAtPath:error:")
        pub attributesOfItemAtPath(path: NSString, error: i64): NSDictionary
        @objc("currentDirectoryPath")
        pub get currentDirectoryPath(): NSString
    }

    /// `NSBundle` — read-only access to a code+resources bundle.
    /// `mainBundle` is the running executable's bundle; useful for
    /// finding resource files shipped alongside the binary.
    @objc pub class NSBundle : NSObject {
        @objc("mainBundle")
        pub static mainBundle(): NSBundle

        @objc("bundlePath")
        pub get bundlePath(): NSString
        @objc("resourcePath")
        pub get resourcePath(): NSString
        @objc("bundleIdentifier")
        pub get bundleIdentifier(): NSString
        @objc("infoDictionary")
        pub get infoDictionary(): NSDictionary
        @objc("pathForResource:ofType:")
        pub pathForResource(name: NSString, ofType: NSString): NSString
    }

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
            obj: NSObject, options: JSONWritingOptions, error: i64
        ): NSData
        @objc("JSONObjectWithData:options:error:")
        pub static JSONObjectWithData(
            data: NSData, options: JSONReadingOptions, error: i64
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
            obj: NSObject, format: PropertyListFormat, options: u64, error: i64
        ): NSData
        @objc("propertyListWithData:options:format:error:")
        pub static propertyListWithData(
            data: NSData, options: u64, format: *u64, error: i64
        ): NSObject
    }

    /// `NSCharacterSet` — immutable set of Unicode characters,
    /// commonly used to drive `NSString` tokenising / trimming
    /// (`componentsSeparatedByCharactersInSet:`,
    /// `stringByTrimmingCharactersInSet:`). The class methods
    /// return the standard predefined sets.
    @objc pub class NSCharacterSet : NSObject {
        @objc("whitespaceCharacterSet")
        pub static whitespace(): NSCharacterSet
        @objc("whitespaceAndNewlineCharacterSet")
        pub static whitespaceAndNewline(): NSCharacterSet
        @objc("newlineCharacterSet")
        pub static newline(): NSCharacterSet
        @objc("letterCharacterSet")
        pub static letter(): NSCharacterSet
        @objc("decimalDigitCharacterSet")
        pub static decimalDigit(): NSCharacterSet
        @objc("alphanumericCharacterSet")
        pub static alphanumeric(): NSCharacterSet
        @objc("punctuationCharacterSet")
        pub static punctuation(): NSCharacterSet
    }

    /// `NSRegularExpression` — ICU-backed pattern matching.
    /// Construct with `regex(pattern, options)` (see helpers
    /// below) which folds away the `NSError **` out-parameter at
    /// the cost of returning a nil-handle wrapper on a bad
    /// pattern.
    ///
    /// Match-introspection (NSTextCheckingResult) is intentionally
    /// not bound yet — its `range:` / `rangeAtIndex:` API leans on
    /// `NSRange`-returning selectors that need careful ABI work to
    /// pass back the struct via objc_msgSend_stret on x86_64. Use
    /// `numberOfMatches` for presence checks and `replaceAll` for
    /// substitution; richer match details await a follow-up.
    @objc pub class NSRegularExpression : NSObject {
        @objc("regularExpressionWithPattern:options:error:")
        pub static regularExpressionWithPattern(
            pattern: NSString, options: RegexOptions, error: i64
        ): NSRegularExpression

        @objc("pattern")
        pub get pattern(): NSString
        @objc("options")
        pub get options(): RegexOptions

        @objc("numberOfMatchesInString:options:range:")
        pub numberOfMatchesInString(
            s: NSString, options: MatchOptions, range: NSRange
        ): i64
        @objc("stringByReplacingMatchesInString:options:range:withTemplate:")
        pub stringByReplacingMatchesInString(
            s: NSString, options: MatchOptions, range: NSRange, template: NSString
        ): NSString
    }

    /// `NSScanner` — stateful cursor over an NSString. The
    /// underlying ObjC selectors return their parsed value
    /// through an out-parameter (`int *`, `double *`, …) and
    /// return `BOOL` to say whether a scan succeeded. We bind
    /// each raw selector as a non-pub method whose out slot is
    /// the matching ilang raw pointer (`*i32`, `*i64`, `*f64`,
    /// `*f32`); the `pub` wrappers below take the address of a
    /// local, call the raw method, and surface the result as a
    /// `T?` Optional (`None` on a failed scan).
    ///
    /// String-out selectors (`scanString:intoString:` and
    /// friends) hand back an `NSString * __autoreleasing *`,
    /// which would need careful retain handling to wrap safely
    /// into an ilang `NSString`. Deferred until the autorelease
    /// semantics get their own pass.
    @objc pub class NSScanner : NSObject {
        @objc("scannerWithString:")
        pub static scannerWithString(s: NSString): NSScanner

        @objc("isAtEnd")
        pub get isAtEnd(): bool
        @objc("scanLocation")
        pub get scanLocation(): u64
        @objc("setScanLocation:")
        pub set scanLocation(loc: u64)

        // ── Raw out-param @objc selectors (non-pub) ────────────
        @objc("scanInt:")
        _scanInt(out: *i32): bool
        @objc("scanInteger:")
        _scanInteger(out: *i64): bool
        @objc("scanDouble:")
        _scanDouble(out: *f64): bool
        @objc("scanFloat:")
        _scanFloat(out: *f32): bool

        // ── Public Optional-returning wrappers ─────────────────
        pub scanInt(): i32? {
            let v: i32 = 0
            if !this._scanInt(&v) { return none }
            some(v)
        }
        pub scanInteger(): i64? {
            let v: i64 = 0
            if !this._scanInteger(&v) { return none }
            some(v)
        }
        pub scanDouble(): f64? {
            let v: f64 = 0.0
            if !this._scanDouble(&v) { return none }
            some(v)
        }
        pub scanFloat(): f32? {
            let v: f32 = 0.0 as f32
            if !this._scanFloat(&v) { return none }
            some(v)
        }
    }

    // ─── Convenience helpers ─────────────────────────────────────

    // libobjc's `sel_registerName` exposed under an ilang-side
    // name with an `i64` return so user code (which can't name
    // raw pointer types) can shuffle a SEL around. The cast back
    // to a typed selector happens automatically inside @objc
    // wrappers; user code just treats SEL as opaque i64.
    @lib("objc") @symbol("sel_registerName")
        fn _sel_register_name(name: *const char): i64

    // ObjC autorelease pool primitives. Mirror `@autoreleasepool {
    // ... }` in Cocoa: push a pool token, run the body, pop the
    // token. Inside the body any `autorelease`-flagged temporaries
    // (e.g. `[NSString stringWithUTF8String:]`) accumulate against
    // this pool instead of the outer thread-default one — pop
    // drains them.
    @lib("objc") @symbol("objc_autoreleasePoolPush")
        fn _autoreleasepool_push(): i64
    @lib("objc") @symbol("objc_autoreleasePoolPop")
        fn _autoreleasepool_pop(token: i64)

    // `objc_retain` / `objc_release` as raw ObjC-id i64 thunks —
    // exposed so the `OrError` wrappers below can keep an object's
    // refcount stable across a `.handle` extraction. Without these
    // the pattern
    //   let (h, err) = withObjcError(fn(slot): i64 {
    //       device.newLibraryWithSource(…, slot).handle
    //   })
    //   (MTLLibrary.__wrap_handle(h), err)
    // double-releases: the closure's typed wrapper goes out of
    // scope at the closure boundary (releasing the +1 from `new…`),
    // and the outer `__wrap_handle` then thinks it owns a fresh
    // +1. Adding one `_objc_retain` inside the closure balances
    // the books.
    @lib("objc") @symbol("objc_retain")
        fn _objc_retain(obj: i64): i64
    @lib("objc") @symbol("objc_release")
        fn _objc_release(obj: i64)

    // Heap-allocates an ObjC block whose `invoke` trampoline calls
    // the captured ilang closure. The block is autoreleased before
    // return so callers can pass it straight to APIs that take a
    // completion handler without manual `Block_release`.
    // Implemented in `ilang-runtime/objc_blocks.rs`.
    @lib("c") @symbol("__ilang_make_void_block")
        fn _make_void_block(closure: fn()): i64
    @lib("c") @symbol("__ilang_make_obj_block")
        fn _make_obj_block(closure: fn(i64)): i64
    @lib("c") @symbol("__ilang_make_obj_to_obj_block")
        fn _make_obj_to_obj_block(closure: fn(i64): i64): i64
    @lib("c") @symbol("__ilang_make_void_bytes_block")
        fn _make_void_bytes_block(closure: fn(i64, i64)): i64

    // Per-thread `NSError *` slot used by the public
    // `withObjcError` wrapper at module scope. The runtime hosts
    // the slot so binding wrappers outside any @extern(C) block
    // can hand its address to `NSError **` out-parameters without
    // needing ilang's `&local` syntax (which only works inside
    // @extern(C)).
    @lib("c") @symbol("__ilang_objc_err_slot_ptr")
        fn _objc_err_slot_ptr(): i64
    @lib("c") @symbol("__ilang_objc_take_err")
        fn _objc_take_err(): i64

    /// Intern an Objective-C selector by name and return its
    /// stable SEL pointer (as `i64`). Use it with
    /// `button.setAction(selectorPtr("clicked:"))`.
    pub fn selectorPtr(name: string): i64 {
        _sel_register_name(cstrFromString(name))
    }

    /// Build an `NSString` from an ilang `string`. Wraps the
    /// `cstrFromString` + `[NSString stringWithUTF8String:]`
    /// two-step.
    pub fn nsstr(s: string): NSString {
        NSString._fromCStr(cstrFromString(s))
    }

    /// Convert an `NSString` back to an ilang `string` — the
    /// mirror of `nsstr`. Internally `[NSString UTF8String]`
    /// returns a C `const char *` whose lifetime is tied to the
    /// autorelease pool, so call this inside an
    /// `autoreleasepool { ... }` if the string is long-lived or
    /// you'll race the pool drain.
    pub fn ilstr(s: NSString): string {
        stringFromCstr(s._utf8String())
    }

    /// Construct an `NSPoint` (= `CGPoint` on macOS).
    pub fn makePoint(x: f64, y: f64): NSPoint {
        let p = new NSPoint()
        p.x = x
        p.y = y
        p
    }

    /// Construct an `NSSize` (= `CGSize` on macOS).
    pub fn makeSize(w: f64, h: f64): NSSize {
        let s = new NSSize()
        s.width = w
        s.height = h
        s
    }

    /// Construct an `NSRect` from primitive components.
    pub fn makeRect(x: f64, y: f64, w: f64, h: f64): NSRect {
        let r = new NSRect()
        r.origin.x = x
        r.origin.y = y
        r.size.width = w
        r.size.height = h
        r
    }

    /// Construct an `NSVector` (= `CGVector` on macOS). Used for
    /// gravity / velocity / impulse / force in physics APIs.
    pub fn makeVector(dx: f64, dy: f64): NSVector {
        let v = new NSVector()
        v.dx = dx
        v.dy = dy
        v
    }

    /// Build an `NSRange` from `(location, length)`. Both fields
    /// are NSUInteger; pass non-negative values.
    pub fn makeRange(location: u64, length: u64): NSRange {
        let r = new NSRange()
        r.location = location
        r.length = length
        r
    }

    /// `{location: 0, length: s.length}` — the range covering
    /// every UTF-16 unit in `s`. Hand it to the `NSRegularExpression`
    /// match / replace selectors when you want to operate on the
    /// whole string.
    pub fn fullRange(s: NSString): NSRange {
        makeRange(0 as u64, s.length as u64)
    }

    /// Compile a regex pattern. Discards the `NSError **` —
    /// callers get a wrapper whose `handle` is nil on a bad
    /// pattern. Cheap form of
    /// `NSRegularExpression.regularExpressionWithPattern(pattern,
    /// options, 0)`.
    pub fn regex(pattern: string, options: RegexOptions): NSRegularExpression {
        NSRegularExpression.regularExpressionWithPattern(
            nsstr(pattern), options, 0
        )
    }

    /// Build an ObjC `void(^)(void)` block from an ilang closure.
    /// Returns the block as an opaque `i64` pointer suitable for
    /// passing to any `@objc` method whose selector takes a
    /// `void(^)(void)` (completion handlers etc.). The block is
    /// autoreleased — let it die with the surrounding pool, or
    /// retain it via `objc_retain` if you need a longer lifetime.
    pub fn makeBlock(body: fn()): i64 {
        _make_void_block(body)
    }

    /// Build an ObjC `void(^)(id)` block — the callback shape used
    /// by AppKit / Foundation when handing a single object back
    /// (e.g. `addLocalMonitorForEventsMatchingMask:handler:`'s
    /// NSEvent argument). The ilang closure receives the object's
    /// raw `id` as `i64`; wrap it with `NSObject.wrap(arg)` (or a
    /// subclass equivalent) inside if you want a typed view.
    pub fn makeObjectBlock(body: fn(i64)): i64 {
        _make_obj_block(body)
    }

    /// Build an ObjC `id(^)(id)` block — the monitoring callback
    /// shape used by `addLocalMonitorForEventsMatchingMask:handler:`
    /// where the handler returns the event to forward (often the
    /// same one), a replacement, or `0` to swallow. The ilang
    /// closure is `fn(handle: i64): i64` — both sides traffic in
    /// raw `id`s, so wrap them with `NSObject.__wrap_handle_unowned`
    /// for AppKit-owned arguments to avoid double-release.
    pub fn makeObjectToObjectBlock(body: fn(i64): i64): i64 {
        _make_obj_to_obj_block(body)
    }

    /// Build an ObjC `void(^)(void *, size_t)` block — the
    /// callback shape used by
    /// `SKMutableTexture.modifyPixelDataWithBlock:` etc. The
    /// ilang closure receives a raw bytes pointer (as `i64`) and
    /// a length; mutate individual cells via the
    /// `cocoa.writeU8` / `readU8` / `writeU32` / `readU32` etc.
    /// helpers below. The buffer is owned by the caller — don't
    /// keep the pointer past the block invocation.
    pub fn makeBytesBlock(body: fn(i64, i64)): i64 {
        _make_void_bytes_block(body)
    }

    // Raw-pointer helpers — call from inside a `makeBytesBlock`
    // body (or any code that has an `i64` pointer in hand) to
    // poke individual primitive cells. The underlying
    // `writeU8` / `readU8` / … FFI helpers are gated to
    // `@extern(C)` context, so these wrappers do the
    // `i64 → *void` cast that user code can't write directly.

    pub fn writeU8(ptr: i64, offset: i64, value: u8) {
        writeU8(ptr as *void, offset, value)
    }
    pub fn readU8(ptr: i64, offset: i64): u8 {
        readU8(ptr as *const void, offset)
    }
    pub fn writeU16(ptr: i64, offset: i64, value: u16) {
        writeU16(ptr as *void, offset, value)
    }
    pub fn readU16(ptr: i64, offset: i64): u16 {
        readU16(ptr as *const void, offset)
    }
    pub fn writeU32(ptr: i64, offset: i64, value: u32) {
        writeU32(ptr as *void, offset, value)
    }
    pub fn readU32(ptr: i64, offset: i64): u32 {
        readU32(ptr as *const void, offset)
    }
    pub fn writeF32(ptr: i64, offset: i64, value: f32) {
        writeF32(ptr as *void, offset, value)
    }
    pub fn readF32(ptr: i64, offset: i64): f32 {
        readF32(ptr as *const void, offset)
    }

    /// Run `body` inside a fresh ObjC autorelease pool. Mirrors
    /// Cocoa's `@autoreleasepool { ... }` — any autorelease'd
    /// temporaries (e.g. the NSStrings minted by `nsstr`) get
    /// drained when `body` returns instead of leaking until the
    /// outer thread-default pool drains (which, in a long-running
    /// loop, may never happen).
    pub fn autoreleasepool(body: fn()) {
        let token = _autoreleasepool_push()
        body()
        _autoreleasepool_pop(token)
    }
}

/// `nil` sender — an NSObject wrapper around the zero pointer.
/// Pass to AppKit calls that want an arbitrary "no sender"
/// receiver (e.g. `win.makeKeyAndOrderFront(cocoa.nil)`).
pub const nil = NSObject.wrap(0)

/// Apply `objc_retain` to a raw `id` (as `i64`) and return the
/// same handle. Use this inside an `OrError` closure to balance
/// the upcoming auto-release of the closure-local @objc wrapper
/// that owns the `+1` returned by a `new…` / `alloc` /  `copy…`
/// selector. See the implementation note next to `_objc_retain`
/// for the full rationale.
pub fn objcRetain(handle: i64): i64 {
    if handle == 0 { 0 } else { _objc_retain(handle) }
}

/// Apply `objc_release` to a raw `id` (as `i64`). Symmetric to
/// `objcRetain`. Pass `0` safely — it's a no-op.
pub fn objcRelease(handle: i64) {
    if handle != 0 { _objc_release(handle) }
}

/// Run `body` with an `NSError **` slot address that the body
/// passes into an error-bearing @objc call's `error:` argument.
/// The body's return value is the call's result (any type the
/// caller wants to surface); the wrapper returns it paired with
/// an `NSError?` populated from whatever the slot ended up
/// holding. On success the error is `none`; on failure it's
/// `some(NSError)` (call `e.localizedDescription`, `e.domain`,
/// `e.code` to inspect).
///
/// Used by per-API wrappers like
/// `MTLDevice.newLibraryWithSourceOrError` — see metal.il for
/// concrete examples.
pub fn withObjcError(body: fn(i64): i64): (i64, NSError?) {
    let slot = _objc_err_slot_ptr()
    // Drain any stale value before the call.
    _objc_take_err()
    let result = body(slot)
    let err_ptr = _objc_take_err()
    let err: NSError? = if err_ptr == 0 {
        none
    } else {
        some(NSError.__wrap_handle_unowned(err_ptr))
    }
    (result, err)
}
