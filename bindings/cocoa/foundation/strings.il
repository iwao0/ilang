/// String-related bindings — pattern matching, scanning, and
/// attributed text. The base `NSString` / `NSMutableString` /
/// `NSCharacterSet` triple stays in `core.il` because `NSString`
/// has methods that take `NSCharacterSet` and other core types
/// (`NSData.dataWithContentsOfFile:` etc.) take `NSString`, so
/// moving them out would create a cyclic dependency between
/// `core` and `strings`.

use core { NSObject, NSString, NSArray, NSDictionary, NSCharacterSet, NSRange,
           nsstr, makeRange }

// NSRegularExpressionOptions — bit flags controlling how the
// regex pattern is compiled. Mirrors the
// `NSRegularExpressionOptions` enum from `<Foundation/NSRegularExpression.h>`.
@flags
pub enum NSRegularExpressionOptions: u64 {
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
pub enum NSMatchingOptions: u64 {
    none                     = 0x00
    reportProgress           = 0x01
    reportCompletion         = 0x02
    anchored                 = 0x04
    withTransparentBounds    = 0x08
    withoutAnchoringBounds   = 0x10
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
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
            pattern: NSString, options: NSRegularExpressionOptions, error: i64
        ): NSRegularExpression

        @objc("pattern")
        pub get pattern(): NSString
        @objc("options")
        pub get options(): NSRegularExpressionOptions

        @objc("numberOfMatchesInString:options:range:")
        pub numberOfMatchesInString(
            s: NSString, options: NSMatchingOptions, range: NSRange
        ): i64
        @objc("stringByReplacingMatchesInString:options:range:withTemplate:")
        pub stringByReplacingMatchesInString(
            s: NSString, options: NSMatchingOptions, range: NSRange, template: NSString
        ): NSString
    }

    /// `NSDataDetector` — `NSRegularExpression` subclass
    /// pre-loaded with the system's URL / phone-number / date /
    /// address / transit-information detectors. The
    /// `checkingTypes` bitmask (`NSTextCheckingTypeLink` etc.)
    /// selects which kinds of matches the detector reports.
    /// Inherits the regex match / replace surface; for richer
    /// per-match info (range, address components, etc.) the ObjC
    /// API returns `NSTextCheckingResult` instances whose
    /// `range:` getter we haven't bound yet (NSRange-returning
    /// selector deferred).
    @objc pub class NSDataDetector : NSRegularExpression {
        @objc("dataDetectorWithTypes:error:")
        pub static dataDetectorWithTypes(
            types: u64, error: i64
        ): NSDataDetector
        @objc("checkingTypes")
        pub get checkingTypes(): u64
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
        @objc("localizedScannerWithString:")
        pub static localizedScannerWithString(s: NSString): NSScanner
        @objc("alloc")
        pub static alloc(): NSScanner
        @objc("initWithString:")
        pub initWithString(s: NSString): NSScanner

        @objc("string")
        pub get string(): NSString
        @objc("isAtEnd")
        pub get isAtEnd(): bool
        @objc("scanLocation")
        pub get scanLocation(): u64
        @objc("setScanLocation:")
        pub set scanLocation(loc: u64)
        @objc("caseSensitive")
        pub get caseSensitive(): bool
        @objc("setCaseSensitive:")
        pub set caseSensitive(v: bool)
        @objc("charactersToBeSkipped")
        pub get charactersToBeSkipped(): NSCharacterSet
        @objc("setCharactersToBeSkipped:")
        pub set charactersToBeSkipped(s: NSCharacterSet)
        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)

        /// ─── String / character-set scans (out-param NSString
        ///     handed back via autoreleased pointer) ────────────
        @objc("scanString:intoString:")
        _scanString(s: NSString, intoString: i64): bool
        @objc("scanCharactersFromSet:intoString:")
        _scanCharactersFromSet(set: NSCharacterSet, intoString: i64): bool
        @objc("scanUpToString:intoString:")
        _scanUpToString(s: NSString, intoString: i64): bool
        @objc("scanUpToCharactersFromSet:intoString:")
        _scanUpToCharactersFromSet(set: NSCharacterSet, intoString: i64): bool

        /// Skip `s` if it appears at the current location.
        /// Returns `true` on success; the location advances past
        /// the matched substring. Returns `false` (location
        /// unchanged) when `s` isn't there.
        pub scanString(s: NSString): bool {
            this._scanString(s, 0)
        }
        /// Same shape for a character set.
        pub scanCharactersFromSet(set: NSCharacterSet): bool {
            this._scanCharactersFromSet(set, 0)
        }
        /// Advance past any character not in `s`. Useful for
        /// "scan everything until the next delimiter".
        pub scanUpToString(s: NSString): bool {
            this._scanUpToString(s, 0)
        }
        pub scanUpToCharactersFromSet(set: NSCharacterSet): bool {
            this._scanUpToCharactersFromSet(set, 0)
        }

        /// ── Raw out-param @objc selectors (non-pub) ────────────
        @objc("scanInt:")
        _scanInt(out: *i32): bool
        @objc("scanInteger:")
        _scanInteger(out: *i64): bool
        @objc("scanDouble:")
        _scanDouble(out: *f64): bool
        @objc("scanFloat:")
        _scanFloat(out: *f32): bool

        /// ── Public Optional-returning wrappers ─────────────────
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

    /// `NSAttributedString` — immutable styled text. Construct
    /// from a plain `NSString` (no attributes), an `NSString` +
    /// attribute dictionary (uniform styling), or another
    /// attributed string (copy). Get the raw text via `.string`
    /// and the length / attribute interrogations below.
    @objc pub class NSAttributedString : NSObject {
        @objc("alloc")
        pub static alloc(): NSAttributedString
        @objc("initWithString:")
        pub initWithString(s: NSString): NSAttributedString
        @objc("initWithString:attributes:")
        pub initWithStringAndAttributes(
            s: NSString, attributes: NSDictionary
        ): NSAttributedString
        @objc("initWithAttributedString:")
        pub initWithAttributedString(
            other: NSAttributedString
        ): NSAttributedString

        @objc("string")
        pub get string(): NSString
        @objc("length")
        pub get length(): i64

        @objc("attributesAtIndex:effectiveRange:")
        pub attributesAtIndex(i: u64, effectiveRange: *u64): NSDictionary
        @objc("attribute:atIndex:effectiveRange:")
        pub attributeAtIndex(
            name: NSString, atIndex: u64, effectiveRange: *u64
        ): NSObject

        @objc("isEqualToAttributedString:")
        pub isEqualToAttributedString(other: NSAttributedString): bool
        @objc("attributedSubstringFromRange:")
        pub attributedSubstringFromRange(range: NSRange): NSAttributedString
    }

    /// `NSMutableAttributedString` — appendable / editable variant.
    /// Wrap mutations in `beginEditing` / `endEditing` brackets
    /// when you make a batch of changes so the layout engine
    /// only relays out once at the end.
    @objc pub class NSMutableAttributedString : NSAttributedString {
        @objc("alloc")
        pub static alloc(): NSMutableAttributedString
        @objc("init")
        pub init(): NSMutableAttributedString

        @objc("mutableString")
        pub get mutableString(): NSString

        @objc("addAttribute:value:range:")
        pub addAttribute(name: NSString, value: NSObject, range: NSRange)
        @objc("addAttributes:range:")
        pub addAttributes(attrs: NSDictionary, range: NSRange)
        @objc("setAttributes:range:")
        pub setAttributes(attrs: NSDictionary, range: NSRange)
        @objc("removeAttribute:range:")
        pub removeAttribute(name: NSString, range: NSRange)

        @objc("replaceCharactersInRange:withString:")
        pub replaceCharactersInRangeWithString(range: NSRange, with: NSString)
        @objc("replaceCharactersInRange:withAttributedString:")
        pub replaceCharactersInRangeWithAttributedString(
            range: NSRange, with: NSAttributedString
        )
        @objc("appendAttributedString:")
        pub appendAttributedString(s: NSAttributedString)
        @objc("insertAttributedString:atIndex:")
        pub insertAttributedString(s: NSAttributedString, atIndex: u64)
        @objc("deleteCharactersInRange:")
        pub deleteCharactersInRange(range: NSRange)

        @objc("beginEditing")
        pub beginEditing()
        @objc("endEditing")
        pub endEditing()
    }

    /// `{location: 0, length: s.length}` — the range covering
    /// every UTF-16 unit in `s`. Hand it to the
    /// `NSRegularExpression` match / replace selectors when you
    /// want to operate on the whole string.
    pub fn fullRange(s: NSString): NSRange {
        makeRange(0 as u64, s.length as u64)
    }

    /// Compile a regex pattern. Discards the `NSError **` —
    /// callers get a wrapper whose `handle` is nil on a bad
    /// pattern. Cheap form of
    /// `NSRegularExpression.regularExpressionWithPattern(pattern,
    /// options, 0)`.
    pub fn regex(pattern: string, options: NSRegularExpressionOptions): NSRegularExpression {
        NSRegularExpression.regularExpressionWithPattern(
            nsstr(pattern), options, 0
        )
    }
}
