// shlwapi.dll — Shell Lightweight Utility APIs.
// Path manipulation, string formatting, and portable registry helpers.
//
// All path functions work on null-terminated ANSI strings.
// MAX_PATH = 260 bytes (including NUL).  Declare buffers at least that size.

// ─── StrToInt64ExA base flags ─────────────────────────────────────
pub enum StrToIntBase: u32 {
    auto  = 0   // detect 0x prefix → hex, else decimal
    hex   = 1
    dec   = 10
}

@extern(C, "shlwapi") {
    // ── Path combination / canonicalisation ──────────────────────

    /// Appends `pszMore` to `pszPath` with a backslash separator.
    /// `pszPath` must be a writable MAX_PATH buffer.
    @lib pub fn PathCombineA(
        pszDest: *char,
        pszDir:  *const char,
        pszFile: *const char
    ): *char

    /// Appends `pszMore` to `pszPath` (which must have room).
    @lib pub fn PathAppendA(pszPath: *char, pszMore: *const char): i32

    /// Adds a trailing backslash to `pszPath` if missing.
    @lib pub fn PathAddBackslashA(pszPath: *char): *char

    /// Removes the trailing backslash from `pszPath`.
    @lib pub fn PathRemoveBackslashA(pszPath: *char): *char

    /// Copies `pszPath` to `pszBuf` with `..\` sequences collapsed.
    @lib pub fn PathCanonicalizeA(pszBuf: *char, pszPath: *const char): i32

    /// Makes `pszPath` relative to `pszRoot`, writing the result into
    /// `pszPath`.  Returns TRUE on success.
    @lib pub fn PathRelativePathToA(
        pszPath: *char,
        pszFrom: *const char,
        dwAttrFrom: u32,
        pszTo:   *const char,
        dwAttrTo: u32
    ): i32

    // ── Path inspection ───────────────────────────────────────────

    /// Returns TRUE if the file or directory exists.
    @lib pub fn PathFileExistsA(pszPath: *const char): i32

    /// Returns TRUE if `pszPath` names a directory.
    @lib pub fn PathIsDirectoryA(pszPath: *const char): i32

    /// Returns TRUE if `pszPath` is a relative path.
    @lib pub fn PathIsRelativeA(pszPath: *const char): i32

    /// Returns TRUE if `pszPath` is a root path ("C:\", "\\", etc.).
    @lib pub fn PathIsRootA(pszPath: *const char): i32

    /// Returns TRUE if `pszPath` contains a UNC prefix (\\server\share).
    @lib pub fn PathIsUNCA(pszPath: *const char): i32

    /// Returns TRUE if `pszPath` matches the wildcard `pszSpec`.
    @lib pub fn PathMatchSpecA(pszFile: *const char, pszSpec: *const char): i32

    /// Returns TRUE if `pszPath` is a long path name (non-8.3).
    @lib pub fn PathIsLFNFileSpecA(lpName: *const char): i32

    // ── Path component extraction ─────────────────────────────────

    /// Returns a pointer into `pszPath` just after the last backslash.
    /// Returns `pszPath` itself if there is no backslash.
    @lib pub fn PathFindFileNameA(pszPath: *const char): *char

    /// Returns a pointer to the file-extension dot in `pszPath`,
    /// or to the NUL terminator if there is no extension.
    @lib pub fn PathFindExtensionA(pszPath: *const char): *char

    /// Returns the drive number (0='A', 1='B', …) or -1 if none.
    @lib pub fn PathGetDriveNumberA(pszPath: *const char): i32

    // ── Path modification ─────────────────────────────────────────

    /// Removes the last path component (file or directory name).
    @lib pub fn PathRemoveFileSpecA(pszPath: *char)

    /// Removes the file extension from `pszPath` in-place.
    @lib pub fn PathRemoveExtensionA(pszPath: *char)

    /// Replaces the extension of `pszPath` with `pszExt` (e.g. ".txt").
    @lib pub fn PathRenameExtensionA(pszPath: *char, pszExt: *const char): i32

    /// Strips the directory part, leaving only the filename.
    @lib pub fn PathStripPathA(pszPath: *char)

    /// Strips everything except the root (e.g. "C:\").
    @lib pub fn PathStripToRootA(pszPath: *char): i32

    /// Adds double-quotes around `lpsz` if it contains a space.
    @lib pub fn PathQuoteSpacesA(lpsz: *char)

    /// Removes surrounding double-quotes from `lpsz`.
    @lib pub fn PathUnquoteSpacesA(lpsz: *char)

    /// Converts forward-slashes to backslashes in `pszPath`.
    @lib pub fn PathMakePrettyA(pszPath: *char): i32

    // ── String utilities ──────────────────────────────────────────

    /// Formats `qdw` (bytes) as a human-readable string like "1.23 GB".
    /// `pszBuf` should be at least 15 characters.
    @lib pub fn StrFormatByteSizeA(
        qdw:    i64,
        pszBuf: *char,
        uiBufSize: u32
    ): *char

    /// Formats `qdw` as kilobytes, e.g. "1,234 KB".
    @lib pub fn StrFormatKBSizeA(
        qdw:    i64,
        pszBuf: *char,
        uiBufSize: u32
    ): *char

    /// Case-sensitive search: returns a pointer to the first occurrence
    /// of `pszKey` in `pszStr`, or NULL.
    @lib pub fn StrStrA(pszFirst: *const char, pszSrch: *const char): *char

    /// Case-insensitive search.
    @lib pub fn StrStrIA(pszFirst: *const char, pszSrch: *const char): *char

    /// Case-sensitive substring reverse-search (find last occurrence).
    @lib pub fn StrRStrIA(pszSource: *const char, pszLast: *const char, pszSrch: *const char): *char

    /// Compare first `nChar` characters (case-sensitive).
    /// Note: StrCmpA/StrCmpIA are SDK macros for lstrcmpA/lstrcmpiA (kernel32);
    /// use those directly for single-arg comparison.
    @lib pub fn StrCmpNA(psz1: *const char, psz2: *const char, nChar: i32): i32

    /// Compare first `nChar` characters (case-insensitive).
    @lib pub fn StrCmpNIA(psz1: *const char, psz2: *const char, nChar: i32): i32

    /// Converts the string to an integer.
    @lib pub fn StrToIntA(pszSrc: *const char): i32

    /// Converts to i64 with explicit base (StrToIntBase.*).
    @lib pub fn StrToInt64ExA(
        pszString: *const char,
        dwFlags:   u32,
        pllRet:    *i64
    ): i32

    /// Trims leading and trailing characters in `pszTrimChars` from
    /// `psz` in-place.
    @lib pub fn StrTrimA(psz: *char, pszTrimChars: *const char): i32

    /// Finds the first occurrence of `wMatch` in `pszStart`.
    @lib pub fn StrChrA(pszStart: *const char, wMatch: u16): *char

    /// Finds the last occurrence of `wMatch`.
    @lib pub fn StrRChrA(pszStart: *const char, pszEnd: *const char, wMatch: u16): *char

    /// Duplicates `psz` into a heap allocation (use LocalFree to free).
    @lib pub fn StrDupA(psz: *const char): *char

    // ── URL helpers ───────────────────────────────────────────────

    /// Returns TRUE if `pszURL` is an absolute URL (has a scheme).
    @lib pub fn UrlIsA(pszURL: *const char, urlIs: u32): i32

    /// Canonicalises `pszURL` into `pszCanonicalized` (must be MAX_PATH).
    @lib pub fn UrlCanonicalizeA(
        pszURL:          *const char,
        pszCanonicalized: *char,
        pcchCanonicalized: *u32,
        dwFlags:         u32
    ): i32

    /// Combines a base URL with a relative URL.
    @lib pub fn UrlCombineA(
        pszBase:     *const char,
        pszRelative: *const char,
        pszCombined: *char,
        pcchCombined: *u32,
        dwFlags:     u32
    ): i32

    /// Compares two URLs after canonicalisation.
    @lib pub fn UrlCompareA(pszURL1: *const char, pszURL2: *const char, fIgnoreSlash: i32): i32
}
