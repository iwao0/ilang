/// icu.dll — ICU (International Components for Unicode), Win10+.
///
/// Windows ships ICU since 1903 (build 17763). Provides:
///   * Locale-aware string compare / normalization
///   * Date / number formatting
///   * Bidirectional text / line breaking
///   * Regex over Unicode
///   * Time zones / calendars
///
/// Most ICU symbols use a versioned suffix in C headers
/// (`u_strFromUTF8_69`), but the import library `icu.lib` /
/// `icuuc.lib` / `icuin.lib` re-exports them under unversioned
/// aliases. We bind the unversioned form.

// ─── UErrorCode (selected entries) ───────────────────────────────
pub enum UErrorCode: i32 {
    useDefaultWarning           = -127
    usingFallbackWarning        = -128
    zeroError                   = 0
    illegalArgumentError        = 1
    missingResourceError        = 2
    invalidFormatError          = 3
    fileAccessError             = 4
    internalProgramError        = 5
    messageParseError           = 6
    memoryAllocationError       = 7
    indexOutofboundsError       = 8
    parseError                  = 9
    invalidCharFound            = 10
    truncatedCharFound          = 11
    illegalCharFound            = 12
    invalidTableFormat          = 13
    invalidTableFile            = 14
    bufferOverflowError         = 15
    unsupportedError            = 16
    resourceTypeMismatch        = 17
    illegalEscapeSequence       = 18
    unsupportedEscapeSequence   = 19
}

@extern(C, "icu") {
    /// Generic UTF-16 string handle. Returned strings are UChar*
    /// (u16*) NUL-terminated unless a length is given.

    // ── String operations ───────────────────────────────────────

    /// `u_strFromUTF8(dest, capacity, &destLen, src, srcLen,
    ///                &errorCode)`.
    @lib pub fn u_strFromUTF8(
        dest:        *u16,
        destCapacity: i32,
        pDestLength: *i32,
        src:         *const char,
        srcLength:   i32,
        pErrorCode:  *i32
    ): *u16

    @lib pub fn u_strToUTF8(
        dest:        *char,
        destCapacity: i32,
        pDestLength: *i32,
        src:         *const u16,
        srcLength:   i32,
        pErrorCode:  *i32
    ): *char

    /// Length in code points (handles surrogates).
    @lib pub fn u_strlen(s: *const u16): i32

    /// Locale-aware compare.
    @lib pub fn u_strcasecmp(
        s1:    *const u16,
        s2:    *const u16,
        options: u32
    ): i32

    /// Returns the next code point (handles surrogate pairs).
    @lib pub fn u_strchr(s: *const u16, c: u32): *u16

    // ── Character category lookups ───────────────────────────────

    /// `u_isalpha(c)`, `u_isdigit(c)`, … return non-zero / 0.
    @lib pub fn u_isalpha(c: u32): i32
    @lib pub fn u_isdigit(c: u32): i32
    @lib pub fn u_isalnum(c: u32): i32
    @lib pub fn u_isspace(c: u32): i32
    @lib pub fn u_isupper(c: u32): i32
    @lib pub fn u_islower(c: u32): i32
    @lib pub fn u_ispunct(c: u32): i32
    @lib pub fn u_iscntrl(c: u32): i32
    @lib pub fn u_isprint(c: u32): i32

    /// Lowercase / uppercase a single code point.
    @lib pub fn u_tolower(c: u32): u32
    @lib pub fn u_toupper(c: u32): u32
    @lib pub fn u_totitle(c: u32): u32

    /// Get the Unicode general category (0..29).
    @lib pub fn u_charType(c: u32): i32

    /// Bidi class (0..23).
    @lib pub fn u_charDirection(c: u32): i32

    // ── Normalization ───────────────────────────────────────────

    /// `unorm2_normalize` — pass an `UNormalizer2*` obtained from
    /// `unorm2_getNFCInstance` etc. The opaque pointer goes
    /// through `*void`.
    @lib pub fn unorm2_normalize(
        norm2:      *void,
        src:        *const u16,
        length:     i32,
        dest:       *u16,
        capacity:   i32,
        pErrorCode: *i32
    ): i32

    @lib pub fn unorm2_getNFCInstance(pErrorCode: *i32): *void
    @lib pub fn unorm2_getNFDInstance(pErrorCode: *i32): *void
    @lib pub fn unorm2_getNFKCInstance(pErrorCode: *i32): *void
    @lib pub fn unorm2_getNFKDInstance(pErrorCode: *i32): *void

    // ── Locale ──────────────────────────────────────────────────

    /// `uloc_getDefault()` — returns the current default locale id
    /// as a static ASCII string ("en_US" etc.).
    @lib pub fn uloc_getDefault(): *const char

    /// `uloc_canonicalize(localeID, name, nameCapacity, &err)`.
    @lib pub fn uloc_canonicalize(
        localeID:   *const char,
        name:       *char,
        nameCapacity: i32,
        err:        *i32
    ): i32

    @lib pub fn uloc_getLanguage(
        localeID:   *const char,
        language:   *char,
        languageCapacity: i32,
        err:        *i32
    ): i32

    @lib pub fn uloc_getCountry(
        localeID:   *const char,
        country:    *char,
        countryCapacity: i32,
        err:        *i32
    ): i32

    // ── Collation (locale-aware compare) ────────────────────────

    /// `ucol_open(loc, &err)` — open a collator. Free with
    /// `ucol_close`.
    @lib pub fn ucol_open(loc: *const char, status: *i32): *void
    @lib pub fn ucol_close(coll: *void)

    @lib pub fn ucol_strcoll(
        coll: *void,
        source: *const u16,
        sourceLength: i32,
        target: *const u16,
        targetLength: i32
    ): i32

    // ── Regex ───────────────────────────────────────────────────

    @lib pub fn uregex_open(
        pattern:       *const u16,
        patternLength: i32,
        flags:         u32,
        pe:            *void,             // *UParseError
        status:        *i32
    ): *void

    @lib pub fn uregex_close(regexp: *void)

    @lib pub fn uregex_setText(
        regexp:     *void,
        text:       *const u16,
        textLength: i32,
        status:     *i32
    )

    @lib pub fn uregex_matches(
        regexp:    *void,
        startIndex: i32,
        status:    *i32
    ): i32

    @lib pub fn uregex_find(
        regexp:    *void,
        startIndex: i32,
        status:    *i32
    ): i32

    @lib pub fn uregex_groupCount(regexp: *void, status: *i32): i32
}
