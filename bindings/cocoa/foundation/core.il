// Foundation bindings — the ObjC base layer (NSObject / NSString /
// collections / dates / URLs) plus the ilang ↔ ObjC plumbing that
// every higher-level binding leans on. Doesn't touch AppKit; safe
// to import on macOS server contexts that don't open a window.
//
// Imported via the `cocoa` umbrella module (`use cocoa { ... }`)
// or directly with `use foundation { ... }`.

/// ─── Foundation geometry types (HFA-passed) ──────────────────────
/// NSPoint / NSSize / NSRect / NSVector are plain f64 structs. The
/// AArch64 ABI rides them through v0..v3 (HFA path) when passed to
/// objc_msgSend — same layout C uses, no shim needed.
pub struct NSPoint  { x: f64; y: f64 }
pub struct NSSize   { width: f64; height: f64 }
pub struct NSRect   { origin: NSPoint; size: NSSize }
/// CGVector — `(dx, dy)` pair Foundation uses for velocities,
/// gravity, impulses, etc. Same layout as `NSSize` but the field
/// names differ; a dedicated type keeps call sites readable.
pub struct NSVector { dx: f64; dy: f64 }

/// NSRange — Foundation's `(location, length)` pair. NSUInteger
/// fields, so 8 bytes each on 64-bit. Passed by value through the
/// integer-register ABI. Build with `makeRange(loc, len)`.
pub struct NSRange { location: u64; length: u64 }

/// NSStringEncoding — the integer codes Foundation uses for the
/// `dataUsingEncoding:` / `initWithData:encoding:` round-trip
/// (also accepted by NSData's `base64EncodedString:options:`-
/// adjacent APIs that take an encoding argument). UTF-8 is the
/// safe default; the JIS / ShiftJIS / EUC-JP entries cover legacy
/// Japanese text files.
pub enum NSStringEncoding: u64 {
    ascii            = 1
    nextstep         = 2
    japaneseEUC      = 3
    utf8             = 4
    isoLatin1        = 5
    nonLossyASCII    = 7
    shiftJIS         = 8
    isoLatin2        = 9
    unicode          = 10
    windowsCP1251    = 11
    windowsCP1252    = 12
    windowsCP1253    = 13
    windowsCP1254    = 14
    windowsCP1250    = 15
    iso2022JP        = 21
    macOSRoman       = 30
    utf16BigEndian   = 0x90000100
    utf16LittleEndian = 0x94000100
    utf32            = 0x8c000100
    utf32BigEndian   = 0x98000100
    utf32LittleEndian = 0x9c000100
}

// NSDataBase64EncodingOptions — bit flags for
// `NSData.base64EncodedString(options:)`. The line-wrap variants
// match standard email / PEM conventions (64 char lines).
@flags
pub enum NSDataBase64EncodingOptions: u64 {
    none                              = 0x00
    lineLength64Characters            = 0x01
    lineLength76Characters            = 0x02
    endLineWithCarriageReturn         = 0x10
    endLineWithLineFeed               = 0x20
}

// NSDataBase64DecodingOptions — controls
// `NSData.initWithBase64EncodedString:options:`.
@flags
pub enum NSDataBase64DecodingOptions: u64 {
    none                                  = 0x00
    ignoreUnknownCharacters               = 0x01
}

// NSDataReadingOptions — flags for
// `[NSData dataWithContentsOfFile:options:error:]` / the URL
// variant. `mappedIfSafe` / `alwaysMapped` swap the read for an
// mmap when the underlying device supports it; `uncached`
// bypasses the disk cache for one-shot bulk reads.
@flags
pub enum NSDataReadingOptions: u64 {
    none                  = 0x00
    mappedIfSafe          = 0x01
    uncached              = 0x02
    alwaysMapped          = 0x08
}

// NSDataWritingOptions — flags for
// `[NSData writeToFile:options:error:]`. `atomic` writes to a
// temp file and renames into place; `withoutOverwriting` fails
// when the destination already exists.
@flags
pub enum NSDataWritingOptions: u64 {
    none                                  = 0x00
    atomic                                = 0x01
    withoutOverwriting                    = 0x02
    fileProtectionNone                    = 0x10000000
    fileProtectionComplete                = 0x20000000
    fileProtectionCompleteUnlessOpen      = 0x30000000
    fileProtectionCompleteUntilFirstUserAuthentication = 0x40000000
}

// NSDataSearchOptions — controls
// `[NSData rangeOfData:options:range:]`. `backwards` searches
// from the end; `anchored` requires the match at the range
// start.
@flags
pub enum NSDataSearchOptions: u64 {
    none      = 0x00
    backwards = 0x01
    anchored  = 0x02
}

/// NSPropertyListMutabilityOptions — what
/// `[NSPropertyListSerialization propertyListWithData:options:
///  format:error:]` should hand back. The default
/// `immutable` keeps every container immutable; `mutableContainers`
/// returns NSMutableArray / NSMutableDictionary instead;
/// `mutableContainersAndLeaves` also returns NSMutableString for
/// scalar values.
pub enum NSPropertyListMutabilityOptions: u64 {
    immutable                  = 0
    mutableContainers          = 1
    mutableContainersAndLeaves = 2
}

// NSFileVersionAddingOptions — flags for
// `[NSFileVersion addVersionOfItemAtURL:withContentsOfURL:options:error:]`.
@flags
pub enum NSFileVersionAddingOptions: u64 {
    none                      = 0x00
    byMoving                  = 0x01
}

// NSFileVersionReplacingOptions — flags for
// `[NSFileVersion replaceItemAtURL:options:error:]`. `byMoving`
// renames into place instead of copy + delete.
@flags
pub enum NSFileVersionReplacingOptions: u64 {
    none                      = 0x00
    byMoving                  = 0x01
}

/// NSComparisonResult — the three-way return value of every
/// Foundation `compare:` method. Mirrors C's strcmp / qsort
/// convention: negative = ascending, zero = same, positive =
/// descending.
pub enum NSComparisonResult: i64 {
    orderedAscending  = -1
    orderedSame       = 0
    orderedDescending = 1
}

// NSStringCompareOptions — bit flags for the locale / case /
// numeric variants of NSString comparison. Combine with `|`.
@flags
pub enum NSStringCompareOptions: u64 {
    none              = 0x00
    caseInsensitive   = 0x01
    literal           = 0x02
    backwards         = 0x04
    anchored          = 0x08
    numeric           = 0x40
    diacriticInsensitive = 0x80
    widthInsensitive  = 0x100
    forcedOrdering    = 0x200
    regularExpression = 0x400
}

// NSEnumerationOptions — bit flags for the block-based
// `enumerateObjectsUsingBlock:` family. `concurrent` lets the
// system dispatch the body across threads; `reverse` iterates
// back to front.
@flags
pub enum NSEnumerationOptions: u64 {
    none       = 0x00
    concurrent = 0x01
    reverse    = 0x02
}

// NSBinarySearchingOptions — controls
// `[NSArray indexOfObject:inSortedRange:options:usingComparator:]`.
// Pick `insertionIndex` to learn where a fresh value should slot
// in instead of "first matching index" / "last matching index".
@flags
pub enum NSBinarySearchingOptions: u64 {
    none           = 0x00
    firstEqual     = 0x100
    lastEqual      = 0x200
    insertionIndex = 0x400
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

        /// ─── Standard NSObject surface ─────────────────────────
        @objc("isKindOfClass:")
        pub isKindOfClass(cls: NSObject): bool
        @objc("isMemberOfClass:")
        pub isMemberOfClass(cls: NSObject): bool
        @objc("conformsToProtocol:")
        pub conformsToProtocol(proto: NSObject): bool
        @objc("respondsToSelector:")
        pub respondsToSelector(sel: i64): bool
        @objc("performSelector:")
        pub performSelector(sel: i64): NSObject
        @objc("performSelector:withObject:")
        pub performSelectorWithObject(sel: i64, withObject: NSObject): NSObject
        @objc("performSelector:withObject:withObject:")
        pub performSelectorWithObjects(
            sel: i64, withObject: NSObject, withObject2: NSObject
        ): NSObject
        @objc("description")
        pub get description(): NSString
        @objc("debugDescription")
        pub get debugDescription(): NSString
        @objc("copy")
        pub copy(): NSObject
        @objc("mutableCopy")
        pub mutableCopy(): NSObject
        @objc("isEqual:")
        pub isEqual(other: NSObject): bool
        @objc("class")
        pub get classObject(): NSObject

        /// ─── Key-Value Coding (NSKeyValueCoding) ──────────────
        @objc("valueForKey:")
        pub valueForKey(key: NSString): NSObject
        @objc("setValue:forKey:")
        pub setValueForKey(value: NSObject, forKey: NSString)
        @objc("valueForKeyPath:")
        pub valueForKeyPath(keyPath: NSString): NSObject
        @objc("setValue:forKeyPath:")
        pub setValueForKeyPath(value: NSObject, forKeyPath: NSString)
        @objc("valueForUndefinedKey:")
        pub valueForUndefinedKey(key: NSString): NSObject
        @objc("mutableArrayValueForKey:")
        pub mutableArrayValueForKey(key: NSString): NSObject
        @objc("mutableSetValueForKey:")
        pub mutableSetValueForKey(key: NSString): NSObject

        /// ─── Key-Value Observing (NSKeyValueObserving) ────────
        @objc("addObserver:forKeyPath:options:context:")
        pub addKVObserver(
            observer: NSObject, forKeyPath: NSString, options: u64, context: i64
        )
        @objc("removeObserver:forKeyPath:")
        pub removeKVObserver(observer: NSObject, forKeyPath: NSString)
        @objc("removeObserver:forKeyPath:context:")
        pub removeKVObserverWithContext(
            observer: NSObject, forKeyPath: NSString, context: i64
        )
        @objc("willChangeValueForKey:")
        pub willChangeValueForKey(key: NSString)
        @objc("didChangeValueForKey:")
        pub didChangeValueForKey(key: NSString)

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

    /// `NSValueTransformer` — Key-Value Binding value
    /// transformer. Register a named transformer via
    /// `+setValueTransformer:forName:`; the Cocoa Bindings
    /// layer looks them up by name to convert between model and
    /// view values (e.g. negation, lookup-table mapping).
    @objc pub class NSValueTransformer : NSObject {
        @objc("valueTransformerForName:")
        pub static valueTransformerForName(name: NSString): NSValueTransformer
        @objc("setValueTransformer:forName:")
        pub static setValueTransformer(t: NSValueTransformer, forName: NSString)
        @objc("valueTransformerNames")
        pub static valueTransformerNames(): NSArray

        @objc("transformedValue:")
        pub transformedValue(v: NSObject): NSObject
        @objc("reverseTransformedValue:")
        pub reverseTransformedValue(v: NSObject): NSObject
        @objc("allowsReverseTransformation")
        pub static allowsReverseTransformation(): bool
        @objc("transformedValueClass")
        pub static transformedValueClass(): NSObject
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

        /// ─── out-parameter wrapper pattern ──────────────────────
        /// The raw ObjC selector takes three `NSUInteger *` slots
        /// that the caller fills in. We bind the selector as a
        /// non-pub method whose params are raw `*u64` (legal inside
        /// `@extern(ObjC)` — same C-pointer surface as the rest of
        /// the block), then expose a `pub` wrapper that allocates
        /// three local `u64`s, takes their addresses via `&`, and
        /// returns the three values as a tuple. The raw method is
        /// not visible outside the class.
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

        /// ─── Predicates ─────────────────────────────────────────
        @objc("hasPrefix:")
        pub hasPrefix(prefix: NSString): bool
        @objc("hasSuffix:")
        pub hasSuffix(suffix: NSString): bool
        @objc("containsString:")
        pub containsString(s: NSString): bool

        /// ─── Case conversion ───────────────────────────────────
        @objc("lowercaseString")
        pub get lowercaseString(): NSString
        @objc("uppercaseString")
        pub get uppercaseString(): NSString
        @objc("capitalizedString")
        pub get capitalizedString(): NSString

        /// ─── Substring / append ────────────────────────────────
        @objc("substringFromIndex:")
        pub substringFromIndex(i: u64): NSString
        @objc("substringToIndex:")
        pub substringToIndex(i: u64): NSString
        @objc("substringWithRange:")
        pub substringWithRange(range: NSRange): NSString
        @objc("stringByAppendingString:")
        pub stringByAppendingString(s: NSString): NSString

        /// ─── Find / replace / split / trim ─────────────────────
        @objc("stringByReplacingOccurrencesOfString:withString:")
        pub stringByReplacing(target: NSString, replacement: NSString): NSString
        @objc("componentsSeparatedByString:")
        pub componentsSeparatedByString(separator: NSString): NSArray
        @objc("componentsSeparatedByCharactersInSet:")
        pub componentsSeparatedByCharactersInSet(set: NSCharacterSet): NSArray
        @objc("stringByTrimmingCharactersInSet:")
        pub stringByTrimmingCharactersInSet(set: NSCharacterSet): NSString

        /// ─── Parsing (best-effort, returns 0 on a non-number) ──
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

        /// ─── URL percent-encoding ──────────────────────────────
        @objc("stringByAddingPercentEncodingWithAllowedCharacters:")
        pub stringByAddingPercentEncoding(
            allowedCharacters: NSCharacterSet
        ): NSString
        @objc("stringByRemovingPercentEncoding")
        pub get stringByRemovingPercentEncoding(): NSString

        /// ─── Encoding round-trip with NSData ───────────────────
        @objc("dataUsingEncoding:")
        pub dataUsingEncoding(encoding: NSStringEncoding): NSData
        @objc("dataUsingEncoding:allowLossyConversion:")
        pub dataUsingEncodingLossy(
            encoding: NSStringEncoding, allowLossyConversion: bool
        ): NSData
        @objc("alloc")
        pub static alloc(): NSString
        @objc("initWithData:encoding:")
        pub initWithData(data: NSData, encoding: NSStringEncoding): NSString

        /// ─── File round-trip ──────────────────────────────────
        @objc("stringWithContentsOfFile:encoding:error:")
        pub static stringWithContentsOfFile(
            path: NSString, encoding: NSStringEncoding, error: i64
        ): NSString
        @objc("writeToFile:atomically:encoding:error:")
        pub writeToFile(
            path: NSString, atomically: bool,
            encoding: NSStringEncoding, error: i64
        ): bool

        /// ─── Comparison ───────────────────────────────────────
        @objc("compare:")
        pub compareTo(other: NSString): NSComparisonResult
        @objc("compare:options:")
        pub compareToWithOptions(
            other: NSString, options: NSStringCompareOptions
        ): NSComparisonResult
        @objc("compare:options:range:")
        pub compareToWithOptionsAndRange(
            other: NSString, options: NSStringCompareOptions, range: NSRange
        ): NSComparisonResult
        @objc("caseInsensitiveCompare:")
        pub caseInsensitiveCompare(other: NSString): NSComparisonResult
        @objc("localizedCompare:")
        pub localizedCompare(other: NSString): NSComparisonResult
        @objc("localizedCaseInsensitiveCompare:")
        pub localizedCaseInsensitiveCompare(other: NSString): NSComparisonResult
        @objc("localizedStandardCompare:")
        pub localizedStandardCompare(other: NSString): NSComparisonResult
        @objc("canBeConvertedToEncoding:")
        pub canBeConvertedToEncoding(encoding: NSStringEncoding): bool

        /// ─── Per-line / per-paragraph chunking ────────────────
        @objc("lineRangeForRange:")
        pub lineRangeForRange(range: NSRange): NSRange
        @objc("paragraphRangeForRange:")
        pub paragraphRangeForRange(range: NSRange): NSRange
        @objc("characterAtIndex:")
        pub characterAtIndex(i: u64): u16

        /// ─── NSString path methods (pre-NSURL helpers, but
        ///     still useful for raw POSIX-path manipulation) ────
        @objc("stringByAppendingPathComponent:")
        pub stringByAppendingPathComponent(component: NSString): NSString
        @objc("stringByAppendingPathExtension:")
        pub stringByAppendingPathExtension(ext: NSString): NSString
        @objc("stringByDeletingLastPathComponent")
        pub get stringByDeletingLastPathComponent(): NSString
        @objc("stringByDeletingPathExtension")
        pub get stringByDeletingPathExtension(): NSString
        @objc("stringByExpandingTildeInPath")
        pub get stringByExpandingTildeInPath(): NSString
        @objc("stringByAbbreviatingWithTildeInPath")
        pub get stringByAbbreviatingWithTildeInPath(): NSString
        @objc("stringByResolvingSymlinksInPath")
        pub get stringByResolvingSymlinksInPath(): NSString
        @objc("stringByStandardizingPath")
        pub get stringByStandardizingPath(): NSString
        @objc("lastPathComponent")
        pub get lastPathComponent(): NSString
        @objc("pathExtension")
        pub get pathExtension(): NSString
        @objc("pathComponents")
        pub get pathComponents(): NSArray
        @objc("isAbsolutePath")
        pub get isAbsolutePath(): bool

        /// ─── Canonical Unicode normalisation forms ────────────
        @objc("precomposedStringWithCanonicalMapping")
        pub get precomposedStringWithCanonicalMapping(): NSString
        @objc("decomposedStringWithCanonicalMapping")
        pub get decomposedStringWithCanonicalMapping(): NSString
        @objc("precomposedStringWithCompatibilityMapping")
        pub get precomposedStringWithCompatibilityMapping(): NSString
        @objc("decomposedStringWithCompatibilityMapping")
        pub get decomposedStringWithCompatibilityMapping(): NSString
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

        @objc("firstObject")
        pub get firstObject(): NSObject
        @objc("lastObject")
        pub get lastObject(): NSObject
        @objc("indexOfObject:")
        pub indexOfObject(o: NSObject): u64
        @objc("indexOfObject:inRange:")
        pub indexOfObjectInRange(o: NSObject, range: NSRange): u64
        @objc("indexOfObjectIdenticalTo:")
        pub indexOfObjectIdenticalTo(o: NSObject): u64
        @objc("isEqualToArray:")
        pub isEqualToArray(other: NSArray): bool

        @objc("arrayByAddingObject:")
        pub arrayByAddingObject(o: NSObject): NSArray
        @objc("arrayByAddingObjectsFromArray:")
        pub arrayByAddingObjectsFromArray(other: NSArray): NSArray
        @objc("subarrayWithRange:")
        pub subarrayWithRange(range: NSRange): NSArray
        @objc("componentsJoinedByString:")
        pub componentsJoinedByString(separator: NSString): NSString
        @objc("filteredArrayUsingPredicate:")
        pub filteredArrayUsingPredicate(predicate: NSObject): NSArray
        @objc("sortedArrayUsingDescriptors:")
        pub sortedArrayUsingDescriptors(descriptors: NSArray): NSArray
        @objc("reverseObjectEnumerator")
        pub reverseObjectEnumerator(): NSObject
        @objc("objectEnumerator")
        pub objectEnumerator(): NSObject
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
        @objc("removeObject:")
        pub removeObject(o: NSObject)
        @objc("removeLastObject")
        pub removeLastObject()
        @objc("replaceObjectAtIndex:withObject:")
        pub replaceObjectAtIndex(i: i64, withObject: NSObject)
        @objc("exchangeObjectAtIndex:withObjectAtIndex:")
        pub exchangeObjectAtIndex(i: u64, withObjectAtIndex: u64)
        @objc("addObjectsFromArray:")
        pub addObjectsFromArray(other: NSArray)
        @objc("sortUsingDescriptors:")
        pub sortUsingDescriptors(descriptors: NSArray)
    }

    /// `NSDictionary` — immutable id→id map.
    @objc pub class NSDictionary : NSObject {
        @objc("count")
        pub get count(): i64
        @objc("objectForKey:")
        pub objectForKey(k: NSObject): NSObject
        @objc("allKeys")
        pub get allKeys(): NSArray
        @objc("allValues")
        pub get allValues(): NSArray
        @objc("isEqualToDictionary:")
        pub isEqualToDictionary(other: NSDictionary): bool
        @objc("objectsForKeys:notFoundMarker:")
        pub objectsForKeys(keys: NSArray, notFoundMarker: NSObject): NSArray
        @objc("keysSortedByValueUsingSelector:")
        pub keysSortedByValueUsingSelector(sel: i64): NSArray
        @objc("valueForKey:")
        pub valueForKeyDict(k: NSString): NSObject
        @objc("keyEnumerator")
        pub keyEnumerator(): NSObject
        @objc("objectEnumerator")
        pub objectEnumerator(): NSObject
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
        @objc("addEntriesFromDictionary:")
        pub addEntriesFromDictionary(other: NSDictionary)
        @objc("setDictionary:")
        pub setDictionary(other: NSDictionary)
        @objc("removeObjectsForKeys:")
        pub removeObjectsForKeys(keys: NSArray)
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

        /// ─── Base64 round-trip ─────────────────────────────────
        @objc("base64EncodedStringWithOptions:")
        pub base64EncodedString(options: NSDataBase64EncodingOptions): NSString
        @objc("alloc")
        pub static alloc(): NSData
        @objc("initWithBase64EncodedString:options:")
        pub initWithBase64EncodedString(
            s: NSString, options: NSDataBase64DecodingOptions
        ): NSData

        @objc("subdataWithRange:")
        pub subdataWithRange(range: NSRange): NSData
        @objc("isEqualToData:")
        pub isEqualToData(other: NSData): bool
        @objc("dataWithContentsOfFile:options:error:")
        pub static dataWithContentsOfFileOptions(
            path: NSString, options: NSDataReadingOptions, error: i64
        ): NSData
        @objc("writeToFile:options:error:")
        pub writeToFileOptions(
            path: NSString, options: NSDataWritingOptions, error: i64
        ): bool
    }

    /// `NSDispatchData` — Foundation wrapper around the
    /// libdispatch `dispatch_data_t` value. Lets a discontiguous
    /// `dispatch_data_t` (often the result of a chained
    /// `dispatch_io_read` callback) flow through APIs that
    /// expect `NSData`. Read-only; the bytes are owned by the
    /// underlying dispatch object.
    @objc pub class NSDispatchData : NSData {}

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

    /// `NSError` — error value passed through Cocoa APIs that fail
    /// in non-fatal ways. Most factories that produce one take an
    /// `NSError **` out-parameter; the ilang side exposes it as a
    /// raw `i64` slot so callers can pass `0` to ignore the error
    /// or hold one address-of-`i64` value (`&local as *void as i64`)
    /// to receive it. The accessors below read from a wrapper you
    /// already have in hand.
    @objc pub class NSError : NSObject {
        @objc("errorWithDomain:code:userInfo:")
        pub static errorWithDomain(
            domain: NSString, code: i64, userInfo: NSDictionary
        ): NSError
        @objc("alloc")
        pub static alloc(): NSError
        @objc("initWithDomain:code:userInfo:")
        pub initWithDomain(
            domain: NSString, code: i64, userInfo: NSDictionary
        ): NSError

        @objc("code")
        pub get code(): i64
        @objc("domain")
        pub get domain(): NSString
        @objc("localizedDescription")
        pub get localizedDescription(): NSString
        @objc("localizedFailureReason")
        pub get localizedFailureReason(): NSString
        @objc("localizedRecoverySuggestion")
        pub get localizedRecoverySuggestion(): NSString
        @objc("userInfo")
        pub get userInfo(): NSDictionary
    }

    /// `NSObjectProtocol` — the root protocol every Objective-C
    /// object conforms to. The instance methods (`-respondsToSelector:`,
    /// `-isKindOfClass:`, etc.) are already exposed on NSObject;
    /// this declaration exists so APIs that accept an `id<NSObject>`
    /// have a concrete protocol to name.
    @objc pub interface NSObjectProtocol {
        @objc("class")
        classObject(): NSObject
        @objc("isKindOfClass:")
        isKindOfClass(cls: NSObject): bool
        @objc("isMemberOfClass:")
        isMemberOfClass(cls: NSObject): bool
        @objc("conformsToProtocol:")
        conformsToProtocol(proto: NSObject): bool
        @objc("respondsToSelector:")
        respondsToSelector(sel: i64): bool
        @objc("isEqual:")
        isEqual(other: NSObject): bool
        @objc("hash")
        hash(): i64
        @objc("description")
        description(): NSString
    }

    /// `NSDiscardableContent` — protocol every type whose
    /// backing buffer the system can drop under memory pressure
    /// conforms to. NSPurgeableData is the concrete poster
    /// child; user types that wrap discardable byte buffers
    /// adopt it so cache APIs (NSCache etc.) can ask "is the
    /// content still here?" before reading.
    @objc pub interface NSDiscardableContent {
        beginContentAccess(): bool
        endContentAccess()
        discardContentIfPossible()
        isContentDiscarded(): bool
    }

    /// `NSException` — legacy exception-style error carrier.
    /// Modern Cocoa surfaces failures through `NSError **`
    /// out-parameters; NSException is only used for programmer
    /// errors (array out-of-range, invalid argument). Caught
    /// natively by ObjC `@try`/`@catch`, but ilang code typically
    /// just lets them propagate to crash.
    @objc pub class NSException : NSObject {
        @objc("exceptionWithName:reason:userInfo:")
        pub static exceptionWithName(
            name: NSString, reason: NSString, userInfo: NSDictionary
        ): NSException
        @objc("raise")
        pub raise()
        @objc("name")
        pub get name(): NSString
        @objc("reason")
        pub get reason(): NSString
        @objc("userInfo")
        pub get userInfo(): NSDictionary
        @objc("callStackSymbols")
        pub get callStackSymbols(): NSArray
    }

    /// `NSMethodSignature` — describes an Objective-C method's
    /// argument / return types. Used together with NSInvocation
    /// for runtime dynamic dispatch; rarely needed in ordinary
    /// code.
    @objc pub class NSMethodSignature : NSObject {
        @objc("numberOfArguments")
        pub get numberOfArguments(): u64
        @objc("frameLength")
        pub get frameLength(): u64
        @objc("methodReturnLength")
        pub get methodReturnLength(): u64
        @objc("isOneway")
        pub get isOneway(): bool
    }

    /// `NSInvocation` — captured method call (selector + target +
    /// arguments) you can replay later or queue. Use it for
    /// runtime-driven dispatch when the selector / argument set
    /// only known at call time. Construct via
    /// `+invocationWithMethodSignature:`.
    @objc pub class NSInvocation : NSObject {
        @objc("invocationWithMethodSignature:")
        pub static invocationWithMethodSignature(
            sig: NSMethodSignature
        ): NSInvocation

        @objc("selector")
        pub get selector(): i64
        @objc("setSelector:")
        pub set selector(s: i64)
        @objc("target")
        pub get target(): NSObject
        @objc("setTarget:")
        pub set target(t: NSObject)
        @objc("methodSignature")
        pub get methodSignature(): NSMethodSignature

        @objc("invoke")
        pub invoke()
        @objc("invokeWithTarget:")
        pub invokeWithTarget(t: NSObject)
        @objc("retainArguments")
        pub retainArguments()
        @objc("argumentsRetained")
        pub get argumentsRetained(): bool
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

        @objc("characterSetWithCharactersInString:")
        pub static characterSetWithCharactersInString(s: NSString): NSCharacterSet
        @objc("invertedSet")
        pub get invertedSet(): NSCharacterSet
        @objc("hasMemberInPlane:")
        pub hasMemberInPlane(plane: u8): bool
        @objc("characterIsMember:")
        pub characterIsMember(c: u16): bool

        /// ─── URL percent-encoding character sets ─────────────
        /// Match each component's `allowedCharactersIn…` set from
        /// RFC 3986; hand to
        /// `NSString.stringByAddingPercentEncoding(allowedCharacters:)`
        /// to encode a piece of a URL safely.
        @objc("URLPathAllowedCharacterSet")
        pub static urlPathAllowed(): NSCharacterSet
        @objc("URLHostAllowedCharacterSet")
        pub static urlHostAllowed(): NSCharacterSet
        @objc("URLUserAllowedCharacterSet")
        pub static urlUserAllowed(): NSCharacterSet
        @objc("URLPasswordAllowedCharacterSet")
        pub static urlPasswordAllowed(): NSCharacterSet
        @objc("URLQueryAllowedCharacterSet")
        pub static urlQueryAllowed(): NSCharacterSet
        @objc("URLFragmentAllowedCharacterSet")
        pub static urlFragmentAllowed(): NSCharacterSet

        /// ─── More predefined sets ────────────────────────────
        @objc("controlCharacterSet")
        pub static controlCharacters(): NSCharacterSet
        @objc("uppercaseLetterCharacterSet")
        pub static uppercaseLetters(): NSCharacterSet
        @objc("lowercaseLetterCharacterSet")
        pub static lowercaseLetters(): NSCharacterSet
        @objc("symbolCharacterSet")
        pub static symbols(): NSCharacterSet
        @objc("illegalCharacterSet")
        pub static illegal(): NSCharacterSet
        @objc("nonBaseCharacterSet")
        pub static nonBase(): NSCharacterSet
        @objc("capitalizedLetterCharacterSet")
        pub static capitalizedLetters(): NSCharacterSet
    }

    /// `NSMutableCharacterSet` — writable variant. Mint with
    /// `characterSet()` (empty), then layer characters / ranges
    /// on with the add / remove methods or `formUnionWithCharacterSet:`
    /// / `formIntersectionWithCharacterSet:` for set algebra.
    @objc pub class NSMutableCharacterSet : NSCharacterSet {
        @objc("alloc")
        pub static alloc(): NSMutableCharacterSet
        @objc("init")
        pub init(): NSMutableCharacterSet
        @objc("characterSet")
        pub static characterSet(): NSMutableCharacterSet

        @objc("addCharactersInRange:")
        pub addCharactersInRange(range: NSRange)
        @objc("removeCharactersInRange:")
        pub removeCharactersInRange(range: NSRange)
        @objc("addCharactersInString:")
        pub addCharactersInString(s: NSString)
        @objc("removeCharactersInString:")
        pub removeCharactersInString(s: NSString)
        @objc("formUnionWithCharacterSet:")
        pub formUnionWithCharacterSet(other: NSCharacterSet)
        @objc("formIntersectionWithCharacterSet:")
        pub formIntersectionWithCharacterSet(other: NSCharacterSet)
        @objc("invert")
        pub invert()
    }

    // ─── Convenience helpers ─────────────────────────────────────

    // libobjc's `sel_registerName` exposed under an ilang-side
    // name with an `i64` return so user code (which can't name
    // raw pointer types) can shuffle a SEL around. The cast back
    // to a typed selector happens automatically inside @objc
    // wrappers; user code just treats SEL as opaque i64.
    @lib("objc") @symbol("sel_registerName")
        fn _sel_register_name(name: *const char): i64

    // ObjC runtime Class ↔ NSString and SEL ↔ NSString helpers.
    // The reverse-direction lookups (`NSClassFromString`,
    // `NSSelectorFromString`) return 0 when the name doesn't
    // resolve — wrap before dereferencing.
    @symbol("NSStringFromClass")
        fn NSStringFromClass(cls: NSObject): NSString
    @symbol("NSClassFromString")
        fn NSClassFromString(name: NSString): NSObject
    @symbol("NSStringFromSelector")
        fn NSStringFromSelector(sel: i64): NSString
    @symbol("NSSelectorFromString")
        fn NSSelectorFromString(name: NSString): i64

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
    @lib("c") @symbol("__ilang_make_void_three_obj_block")
        fn _make_void_three_obj_block(closure: fn(i64, i64, i64)): i64

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

    /// Build an ObjC `void(^)(id, id, id)` block — the callback
    /// shape every `NSURLSession.dataTaskWithRequest:completionHandler:`
    /// (and its download/upload siblings) wants, where the three
    /// arguments arrive as raw `id` handles for
    /// `(NSData *, NSURLResponse *, NSError *)`. Wrap each handle
    /// inside the body with `NSObject.__wrap_handle_unowned` —
    /// these come pre-autoreleased so we MUST NOT take a `+1` to
    /// avoid an over-release at scope exit.
    pub fn makeThreeObjectBlock(body: fn(i64, i64, i64)): i64 {
        _make_void_three_obj_block(body)
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
