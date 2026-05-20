/// version.dll — file version information.
///
/// Reads the VERSIONINFO resource embedded in EXEs/DLLs.
///
/// Typical usage:
///   1. dwSize = GetFileVersionInfoSizeA(path, &handle)
///   2. buf = HeapAlloc(...; dwSize)
///   3. GetFileVersionInfoA(path, 0, dwSize, buf)
///   4. VerQueryValueA(buf, "\\", &ffi, &len)
///      → ffi is *VS_FIXEDFILEINFO  (52 bytes)
///   5. for translated strings:
///      VerQueryValueA(buf, "\\VarFileInfo\\Translation", &tx, &len)
///      VerQueryValueA(buf, "\\StringFileInfo\\xxxxxxxx\\CompanyName", &s, &len)

// ─── VS_FIXEDFILEINFO field bit flags ────────────────────────────
@flags
pub enum VsFileFlags: u32 {
    debug         = 0x0001
    prerelease    = 0x0002
    patched       = 0x0004
    privatebuild  = 0x0008
    infoinferred  = 0x0010
    specialbuild  = 0x0020
}

// ─── VS_FIXEDFILEINFO dwFileOS values ────────────────────────────
pub enum VsFileOS: u32 {
    unknown   = 0x00000000
    dos       = 0x00010000
    os216     = 0x00020000
    os232     = 0x00030000
    nt        = 0x00040000
    wince     = 0x00050000
    windows16 = 0x00000001
    pm16      = 0x00000002
    pm32      = 0x00000003
    windows32 = 0x00000004
    dosWindows16 = 0x00010001
    dosWindows32 = 0x00010004
    os216Pm16    = 0x00020002
    os232Pm32    = 0x00030003
    ntWindows32  = 0x00040004
}

// ─── VS_FIXEDFILEINFO dwFileType values ──────────────────────────
pub enum VsFileType: u32 {
    unknown  = 0
    app      = 1
    dll      = 2
    drv      = 3
    font     = 4
    vxd      = 5
    staticLib = 7
}

@extern(C, "version") {
    // ── VS_FIXEDFILEINFO — 52 bytes ──────────────────────────────

    /// The fixed-size header inside a version resource.
    /// `dwSignature` is always 0xFEEF04BD.
    /// File version = (`dwFileVersionMS` >> 16, `dwFileVersionMS` & 0xFFFF,
    ///                 `dwFileVersionLS` >> 16, `dwFileVersionLS` & 0xFFFF).
    pub struct VS_FIXEDFILEINFO {
        dwSignature:        u32
        dwStrucVersion:     u32
        dwFileVersionMS:    u32
        dwFileVersionLS:    u32
        dwProductVersionMS: u32
        dwProductVersionLS: u32
        dwFileFlagsMask:    u32
        dwFileFlags:        u32    // VsFileFlags.*
        dwFileOS:           u32    // VsFileOS.*
        dwFileType:         u32    // VsFileType.*
        dwFileSubtype:      u32
        dwFileDateMS:       u32
        dwFileDateLS:       u32
    }

    // ── Version-info query ───────────────────────────────────────

    /// Returns the size, in bytes, of the version-information block
    /// for `lptstrFilename`. Returns 0 if no version info exists.
    /// `lpdwHandle` is filled with a value used by `GetFileVersionInfoA`
    /// (or pass `0 as *u32` to ignore).
    @lib pub fn GetFileVersionInfoSizeA(
        lptstrFilename: *const char,
        lpdwHandle:     *u32
    ): u32

    /// Copies the version-info block into `lpData` (allocate `dwLen`
    /// bytes first using `GetFileVersionInfoSizeA`).
    /// `dwHandle`: must be 0.
    @lib pub fn GetFileVersionInfoA(
        lptstrFilename: *const char,
        dwHandle:       u32,
        dwLen:          u32,
        lpData:         *void
    ): i32

    /// Extracts a sub-value from the version block.
    /// `lpSubBlock`: backslash-separated path within the version block:
    ///   "\\" → *VS_FIXEDFILEINFO
    ///   "\\VarFileInfo\\Translation" → DWORD[] of language/codepage IDs
    ///   "\\StringFileInfo\\<lang>\\CompanyName" → string (TCHAR)
    /// `lplpBuffer`: receives a pointer into the block (do not free).
    /// `puLen`: receives the byte length of the value.
    @lib pub fn VerQueryValueA(
        pBlock:     *const void,
        lpSubBlock: *const char,
        lplpBuffer: **void,
        puLen:      *u32
    ): i32

    // ── Language name lookup ──────────────────────────────────────

    /// Looks up the human-readable language name for a 16-bit
    /// language identifier (the low word of an LCID).
    @lib pub fn VerLanguageNameA(
        wLang:       u32,
        szLang:      *char,
        cchLang:     u32
    ): u32

    // ── File installation helpers ─────────────────────────────────

    /// Compares the version of two files.  Returns flags indicating
    /// which is newer (VFF_CURNEDEST = 0x1, VFF_FILEINUSE = 0x2,
    /// VFF_BUFFTOOSMALL = 0x4).
    @lib pub fn VerFindFileA(
        uFlags:       u32,
        szFileName:   *const char,
        szWinDir:     *const char,
        szAppDir:     *const char,
        szCurDir:     *char,
        lpuCurDirLen: *u32,
        szDestDir:    *char,
        lpuDestDirLen: *u32
    ): u32

    @lib pub fn VerInstallFileA(
        uFlags:        u32,
        szSrcFileName: *const char,
        szDestFileName: *const char,
        szSrcDir:      *const char,
        szDestDir:     *const char,
        szCurDir:      *const char,
        szTmpFile:     *char,
        lpuTmpFileLen: *u32
    ): u32
}
