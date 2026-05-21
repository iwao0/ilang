/// oleaut32.dll — OLE Automation: BSTR, VARIANT, SAFEARRAY,
/// type libraries, and IDispatch-style error reporting.
///
/// All BSTRs are 16-bit Unicode strings with a 4-byte length prefix
/// stored at `bstr - 4`. Always allocate with SysAllocString and free
/// with SysFreeString — never use free() or HeapFree directly.

// ─── Variant type codes (VT_*) ───────────────────────────────────
pub enum VarType: u16 {
    empty       = 0      // VT_EMPTY      — not specified
    null        = 1      // VT_NULL       — SQL-style NULL
    i2          = 2      // VT_I2         — i16
    i4          = 3      // VT_I4         — i32
    r4          = 4      // VT_R4         — f32
    r8          = 5      // VT_R8         — f64
    cy          = 6      // VT_CY         — CURRENCY (i64 ÷ 10000)
    date        = 7      // VT_DATE       — f64 (days since 1899-12-30)
    bstr        = 8      // VT_BSTR       — BSTR (UTF-16 with length prefix)
    dispatch    = 9      // VT_DISPATCH   — IDispatch*
    error       = 10     // VT_ERROR      — SCODE
    bool_       = 11     // VT_BOOL       — VARIANT_BOOL (0x0000 / 0xFFFF)
    variant     = 12     // VT_VARIANT    — VARIANT* (in SAFEARRAY only)
    unknown     = 13     // VT_UNKNOWN    — IUnknown*
    decimal     = 14     // VT_DECIMAL
    i1          = 16     // VT_I1         — i8
    ui1         = 17     // VT_UI1        — u8
    ui2         = 18     // VT_UI2        — u16
    ui4         = 19     // VT_UI4        — u32
    i8          = 20     // VT_I8         — i64
    ui8         = 21     // VT_UI8        — u64
    int_        = 22     // VT_INT        — int
    uint_       = 23     // VT_UINT       — unsigned int
    void_       = 24     // VT_VOID
    hresult     = 25     // VT_HRESULT
    ptr         = 26     // VT_PTR
    safeArray   = 27     // VT_SAFEARRAY
    carray      = 28
    userdefined = 29
    lpstr       = 30
    lpwstr      = 31
    record      = 36
    intPtr      = 37
    uintPtr     = 38
    filetime    = 64
    blob        = 65
    stream      = 66
    storage     = 67
    streamedObject = 68
    storedObject = 69
    blobObject  = 70
    cf          = 71
    clsid       = 72
    /// Flag: combine with a base type to mark a by-reference variant.
    byRef       = 0x4000
    /// Flag: combine with a base type to mark an array.
    array       = 0x2000
    /// Mask for the base type bits.
    typeMask    = 0x0FFF
}

// ─── SAFEARRAY feature flags (FADF_*) ────────────────────────────
@flags
pub enum SafeArrayFeatures: u16 {
    auto        = 0x0001    // stack-allocated
    static_     = 0x0002
    embedded    = 0x0004
    fixedSize   = 0x0010    // size cannot change
    record      = 0x0020
    haveIid     = 0x0040
    haveVartype = 0x0080
    bstr        = 0x0100
    unknown     = 0x0200
    dispatch    = 0x0400
    variant     = 0x0800
    reserved    = 0xF008
}

// ─── DISPATCH_* flags for IDispatch::Invoke ──────────────────────
@flags
pub enum DispatchFlags: u16 {
    method          = 0x1
    propertyGet     = 0x2
    propertyPut     = 0x4
    propertyPutRef  = 0x8
}

// ─── Common HRESULT codes specific to Automation ─────────────────
pub enum HResultAuto: u32 {
    typeMismatch      = 0x80020005  // DISP_E_TYPEMISMATCH
    paramNotFound     = 0x80020004  // DISP_E_PARAMNOTFOUND
    badIndex          = 0x8002000b  // DISP_E_BADINDEX
    badVarType        = 0x80020008  // DISP_E_BADVARTYPE
    overflow          = 0x8002000a  // DISP_E_OVERFLOW
    unknownName       = 0x80020006  // DISP_E_UNKNOWNNAME
    unknownInterface  = 0x80020001  // DISP_E_UNKNOWNINTERFACE
    memberNotFound    = 0x80020003  // DISP_E_MEMBERNOTFOUND
    exception         = 0x80020009  // DISP_E_EXCEPTION
}

@extern(C, "oleaut32") {
    // ── SAFEARRAY structs ─────────────────────────────────────────

    /// One dimension descriptor inside a SAFEARRAY.
    pub struct SAFEARRAYBOUND {
        cElements: u32
        lLbound:   i32
    }

    /// SAFEARRAY header. Followed by `cDims` SAFEARRAYBOUND records
    /// and then the data block at `pvData`.
    pub struct SAFEARRAY {
        cDims:       u16    // number of dimensions
        fFeatures:   u16    // SafeArrayFeatures.*
        cbElements:  u32    // size of one element in bytes
        cLocks:      u32    // outstanding access locks
        // 4 bytes implicit padding before pvData on x64
        pvData:      *void
        // Followed by SAFEARRAYBOUND rgsabound[cDims]
    }

    // ── VARIANT — 24 bytes on x64 ─────────────────────────────────

    /// Tagged union holding any Automation-compatible value.
    /// Always call VariantInit before first use and VariantClear
    /// before reuse/free.  The 16-byte union body is interpreted
    /// according to `vt`:
    ///   bstr (8 bytes pointer), iVal (i32), dblVal (f64), pdispVal,
    ///   parray (SAFEARRAY*), etc.
    /// For simple values, treat `data_lo` as the primary slot.
    pub struct VARIANT {
        vt:          u16    // VarType.*
        wReserved1:  u16
        wReserved2:  u16
        wReserved3:  u16
        data_lo:     u64    // first 8 bytes of value union
        data_hi:     u64    // second 8 bytes (DECIMAL high or padding)
    }

    // ── BSTR allocation ───────────────────────────────────────────

    /// Allocates a BSTR from an ANSI string (converted to UTF-16).
    /// Returns NULL on failure. Free with `SysFreeString`.
    /// Note: BSTR is `*u16`; cast as needed.
    @lib pub fn SysAllocString(psz: *const u16): *u16

    /// Allocates a BSTR of `len` characters; ignores any embedded NUL.
    @lib pub fn SysAllocStringLen(strIn: *const u16, ui: u32): *u16

    /// Reallocates a BSTR. Returns 1 on success, 0 on failure.
    @lib pub fn SysReAllocString(pbstr: **u16, psz: *const u16): i32

    @lib pub fn SysReAllocStringLen(pbstr: **u16, psz: *const u16, len: u32): i32

    /// Frees a BSTR.  Safe to call with NULL.
    @lib pub fn SysFreeString(bstrString: *u16)

    /// Returns the length of a BSTR in characters (excluding NUL).
    @lib pub fn SysStringLen(bstr: *u16): u32

    /// Returns the byte length of a BSTR (UTF-16 byte count).
    @lib pub fn SysStringByteLen(bstr: *u16): u32

    // ── VARIANT operations ────────────────────────────────────────

    /// Initialises a VARIANT to VT_EMPTY. Required before first use.
    @lib pub fn VariantInit(pvarg: *VARIANT)

    /// Releases the storage owned by a VARIANT (BSTR, IDispatch, etc.)
    /// and sets it back to VT_EMPTY.
    @lib pub fn VariantClear(pvarg: *VARIANT): i32

    /// Performs a deep copy from `pvargSrc` to `pvargDest`.
    @lib pub fn VariantCopy(pvargDest: *VARIANT, pvargSrc: *const VARIANT): i32

    /// Like VariantCopy but follows VT_BYREF pointers (deep copy).
    @lib pub fn VariantCopyInd(pvargDest: *VARIANT, pvargSrc: *const VARIANT): i32

    /// Converts the contents of `pvarSrc` to `vt` and writes to `pvarDest`.
    /// `wFlags`: VARIANT_NOVALUEPROP (0x1), VARIANT_ALPHABOOL (0x2), ...
    @lib pub fn VariantChangeType(
        pvarDest: *VARIANT,
        pvarSrc:  *const VARIANT,
        wFlags:   u16,
        vt:       u16
    ): i32

    @lib pub fn VariantChangeTypeEx(
        pvarDest: *VARIANT,
        pvarSrc:  *const VARIANT,
        lcid:     u32,
        wFlags:   u16,
        vt:       u16
    ): i32

    // ── SAFEARRAY operations ──────────────────────────────────────

    /// Creates a new SAFEARRAY. `cDims`: number of dimensions;
    /// `rgsabound`: array of `cDims` SAFEARRAYBOUND values.
    @lib pub fn SafeArrayCreate(
        vt:        u16,
        cDims:     u32,
        rgsabound: *SAFEARRAYBOUND
    ): *SAFEARRAY

    /// Destroys a SAFEARRAY and frees the data block.
    @lib pub fn SafeArrayDestroy(psa: *SAFEARRAY): i32

    /// Increments the lock count and returns a pointer to the data.
    /// Pair with SafeArrayUnaccessData.
    @lib pub fn SafeArrayAccessData(psa: *SAFEARRAY, ppvData: **void): i32

    @lib pub fn SafeArrayUnaccessData(psa: *SAFEARRAY): i32

    @lib pub fn SafeArrayLock(psa: *SAFEARRAY): i32
    @lib pub fn SafeArrayUnlock(psa: *SAFEARRAY): i32

    /// Reads one element. `rgIndices`: array of `cDims` indices.
    /// `pv`: output buffer of size matching the element type.
    @lib pub fn SafeArrayGetElement(
        psa:       *SAFEARRAY,
        rgIndices: *const i32,
        pv:        *void
    ): i32

    @lib pub fn SafeArrayPutElement(
        psa:       *SAFEARRAY,
        rgIndices: *const i32,
        pv:        *const void
    ): i32

    @lib pub fn SafeArrayGetLBound(psa: *SAFEARRAY, nDim: u32, plLbound: *i32): i32
    @lib pub fn SafeArrayGetUBound(psa: *SAFEARRAY, nDim: u32, plUbound: *i32): i32
    @lib pub fn SafeArrayGetDim(psa: *SAFEARRAY): u32

    /// Returns the size in bytes of one element.
    @lib pub fn SafeArrayGetElemsize(psa: *SAFEARRAY): u32

    @lib pub fn SafeArrayGetVartype(psa: *SAFEARRAY, pvt: *u16): i32

    /// Resizes the rightmost dimension of `psa` to `psaboundNew`.
    @lib pub fn SafeArrayRedim(psa: *SAFEARRAY, psaboundNew: *const SAFEARRAYBOUND): i32

    @lib pub fn SafeArrayCopy(psa: *SAFEARRAY, ppsaOut: **SAFEARRAY): i32

    // ── Error info ────────────────────────────────────────────────

    /// Retrieves the IErrorInfo for the current thread.
    @lib pub fn GetErrorInfo(dwReserved: u32, pperrinfo: **void): i32

    @lib pub fn SetErrorInfo(dwReserved: u32, perrinfo: *void): i32

    @lib pub fn CreateErrorInfo(pperrinfo: **void): i32

    // ── Type library helpers ──────────────────────────────────────

    /// Loads a type library from a file path.
    @lib pub fn LoadTypeLib(szFile: *const u16, pptlib: **void): i32

    @lib pub fn LoadTypeLibEx(
        szFile:  *const u16,
        regKind: u32,
        pptlib:  **void
    ): i32

    @lib pub fn LoadRegTypeLib(
        rguid:        *const void,   // *GUID
        wVerMajor:    u16,
        wVerMinor:    u16,
        lcid:         u32,
        pptlib:       **void
    ): i32

    /// Registers a type library in the registry.
    @lib pub fn RegisterTypeLib(
        ptlib:    *void,
        szFullPath: *const u16,
        szHelpDir:  *const u16
    ): i32

    @lib pub fn UnRegisterTypeLib(
        libID:     *const void,
        wVerMajor: u16,
        wVerMinor: u16,
        lcid:      u32,
        syskind:   u32
    ): i32
}
