/// msi.dll — Windows Installer (MSI) — install / query / uninstall
/// product packages.
///
/// Two entry points dominate practical use:
///   * `MsiInstallProduct` / `MsiConfigureProductEx` — install or
///     uninstall an .msi
///   * `MsiGetProductInfo` / `MsiEnumProducts` — query installed
///     product database
///
/// Database (`.msi` file) access lives behind `MsiOpenDatabase`
/// (read/write tables) — useful for transform / patch authoring.

// ─── INSTALLSTATE — feature / component install state ────────────
pub enum InstallState: i32 {
    notUsed     = -7
    badConfig   = -6
    incomplete  = -5
    sourceAbsent = -4
    moreData    = -3
    invalidArg  = -2
    unknown     = -1
    broken      = 0
    advertised  = 1
    removed     = 1     // alias
    absent      = 2
    local       = 3
    source      = 4
    default_    = 5
}

// ─── INSTALLLEVEL — feature install levels ───────────────────────
pub enum InstallLevel: i32 {
    default_ = 0
    minimum  = 1
    maximum  = 0xFFFF
}

// ─── INSTALLUILEVEL — UI mode for MsiSetInternalUI ───────────────
pub enum InstallUiLevel: u32 {
    noChange  = 0
    default_  = 1
    none      = 2
    basic     = 3
    reduced   = 4
    full      = 5
    sourceResOnly = 0x100
    uacOnly       = 0x200
}

// ─── INSTALLLOGMODE — log channel bits ───────────────────────────
@flags
pub enum InstallLogMode: u32 {
    fatalexit      = 0x00000001
    error          = 0x00000002
    warning        = 0x00000004
    user           = 0x00000008
    info           = 0x00000010
    resolveSource  = 0x00000040
    outofdiskspace = 0x00000080
    actionStart    = 0x00000100
    actionData     = 0x00000200
    commonData     = 0x00000400
    propertyDump   = 0x00000800
    verbose        = 0x00001000
    extraDebug     = 0x00002000
}

// ─── Common return codes (ERROR_* from winerror.h) ───────────────
pub enum MsiError: u32 {
    success            = 0
    invalidParameter   = 87
    callNotImplemented = 120
    unknownProduct     = 1605
    unknownFeature     = 1606
    unknownComponent   = 1607
    unknownProperty    = 1608
    handleInvalid      = 1609
    badConfiguration   = 1610
    indexAbsent        = 1611
    installSourceAbsent = 1612
    bufferOverflow     = 122
    moreData           = 234
}

@extern(C, "msi") {
    /// Opaque MSI handle.
    @handle pub struct MSIHANDLE {}

    // ── Install / configure ─────────────────────────────────────

    /// `MsiInstallProductA(packagePath, commandLine)` — install
    /// an .msi. `commandLine` is a string of property=value pairs.
    @lib pub fn MsiInstallProductA(
        szPackagePath: *const char,
        szCommandLine: *const char
    ): u32

    /// Re-configure an installed product. `szInstallLevel`
    /// 1..32767; INSTALLSTATE_LOCAL etc. for `eInstallState`.
    @lib pub fn MsiConfigureProductA(
        szProduct:     *const char,
        iInstallLevel: i32,
        eInstallState: i32
    ): u32

    @lib pub fn MsiConfigureProductExA(
        szProduct:     *const char,
        iInstallLevel: i32,
        eInstallState: i32,
        szCommandLine: *const char
    ): u32

    /// Uninstall by product code. Equivalent to
    /// `MsiConfigureProduct(code, 0, INSTALLSTATE_ABSENT)`.
    @lib pub fn MsiReinstallProductA(szProduct: *const char, dwReinstallMode: u32): u32

    // ── Product enumeration / query ─────────────────────────────

    /// Enumerate installed product GUIDs. `iProductIndex` starts
    /// at 0 and increments until ERROR_NO_MORE_ITEMS (259).
    /// `lpProductBuf` must hold at least 39 chars.
    @lib pub fn MsiEnumProductsA(iProductIndex: u32, lpProductBuf: *char): u32

    /// `MsiGetProductInfoA(product, property, valueBuf, &chCount)` —
    /// query a property of an installed product (e.g.
    /// "ProductName", "InstallLocation", "VersionString").
    @lib pub fn MsiGetProductInfoA(
        szProduct:   *const char,
        szAttribute: *const char,
        lpValueBuf:  *char,
        pcchValueBuf: *u32
    ): u32

    /// Translate a product code GUID → human-readable product name
    /// (UI list / Programs & Features-style display).
    @lib pub fn MsiGetProductCodeA(
        szComponent: *const char,
        lpBuf39:     *char
    ): u32

    /// Test whether a feature is installed.
    @lib pub fn MsiQueryFeatureStateA(
        szProduct: *const char,
        szFeature: *const char
    ): i32

    @lib pub fn MsiQueryProductStateA(szProduct: *const char): i32

    // ── UI control ──────────────────────────────────────────────

    /// Set the install UI mode (silent / basic / full). Returns
    /// the previous mode.
    @lib pub fn MsiSetInternalUI(dwUILevel: u32, phWnd: *i64): u32

    /// Open an external log file. `szLogFile` may be NULL to
    /// disable the previously-attached log.
    @lib pub fn MsiEnableLogA(
        dwLogMode:  u32,
        szLogFile:  *const char,
        dwLogAttributes: u32
    ): u32

    // ── Database access (authoring / transforms) ────────────────

    /// Open an MSI database. `szPersist` is a persistence flag
    /// pointer: NULL = MSIDBOPEN_READONLY, 1 = TRANSACT,
    /// 2 = DIRECT, etc. (Win32 spells them as pointer constants
    /// — pass `0 as *const char` for read-only or
    /// `1 as *const char` for transacted.)
    @lib pub fn MsiOpenDatabaseA(
        szDatabasePath: *const char,
        szPersist:      *const char,
        phDatabase:     *MSIHANDLE
    ): u32

    @lib pub fn MsiDatabaseCommit(hDatabase: MSIHANDLE): u32
    @lib pub fn MsiCloseHandle(hAny: MSIHANDLE): u32
    @lib pub fn MsiCloseAllHandles(): u32

    // ── Records / views (table query) ───────────────────────────

    @lib pub fn MsiCreateRecord(cParams: u32): MSIHANDLE
    @lib pub fn MsiDatabaseOpenViewA(
        hDatabase: MSIHANDLE,
        szQuery:   *const char,
        phView:    *MSIHANDLE
    ): u32

    @lib pub fn MsiViewExecute(hView: MSIHANDLE, hRecord: MSIHANDLE): u32
    @lib pub fn MsiViewFetch(hView: MSIHANDLE, phRecord: *MSIHANDLE): u32
    @lib pub fn MsiViewClose(hView: MSIHANDLE): u32

    @lib pub fn MsiRecordGetInteger(hRecord: MSIHANDLE, iField: u32): i32
    @lib pub fn MsiRecordGetStringA(
        hRecord: MSIHANDLE,
        iField:  u32,
        szValueBuf: *char,
        pcchValueBuf: *u32
    ): u32
    @lib pub fn MsiRecordSetStringA(
        hRecord: MSIHANDLE,
        iField:  u32,
        szValue: *const char
    ): u32
    @lib pub fn MsiRecordSetInteger(
        hRecord: MSIHANDLE,
        iField:  u32,
        iValue:  i32
    ): u32

    // ── Misc ────────────────────────────────────────────────────

    /// Compose `Major.Minor.Build.Revision` from a packed version.
    @lib pub fn MsiGetFileVersionA(
        szFilePath:     *const char,
        lpVersionBuf:   *char,
        pcchVersionBuf: *u32,
        lpLangBuf:      *char,
        pcchLangBuf:    *u32
    ): u32

    /// File hash (computes the same hash that Windows Installer
    /// records in the MsiFileHash table). `pHash` is a struct of
    /// 5 u32 (size 20).
    @lib pub fn MsiGetFileHashA(
        szFilePath:     *const char,
        dwOptions:      u32,
        pHash:          *void
    ): u32
}
