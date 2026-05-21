/// propsys.dll — Property System: PROPVARIANT manipulation +
/// IPropertyStore plumbing.
///
/// Used by SHGetPropertyStoreFromParsingName / IFileOperation /
/// audio endpoint properties / taskbar tooltips. The COM
/// `IPropertyStore` interface is heavy — this file binds the
/// PROPVARIANT helpers + factory entry points.
///
/// `PROPVARIANT` is a tagged union; layout is 24 bytes (vt + 6
/// bytes padding + 16-byte payload). Initialise with PropVariantInit
/// before reading.

use wasapi { PROPVARIANT }

// PropVariantInit / Clear / Copy / ChangeType live in ole32.dll
// (or oleaut32.dll depending on the SDK era). They aren't exported
// from propsys.dll itself, so we bind them against ole32 here so a
// single `use windows { PropVariantClear }` works.

@extern(C, "ole32") {
    /// Free any heap-allocated payload (strings, blobs, vectors).
    @lib pub fn PropVariantClear(pvar: *PROPVARIANT): i32

    /// Deep-copy.
    @lib pub fn PropVariantCopy(pvarDest: *PROPVARIANT, pvarSrc: *const PROPVARIANT): i32
}

@extern(C, "propsys") {
    /// Coerce a PROPVARIANT to a target VARTYPE. Returns S_OK on
    /// success; the converted value lands in `*pvarOut`.
    @lib pub fn PropVariantChangeType(
        pvarOut: *PROPVARIANT,
        pvarIn:  *const PROPVARIANT,
        flags:   u16,
        vt:      u16
    ): i32

    // ── Read helpers (typed PROPVARIANT → primitive) ─────────────

    @lib pub fn PropVariantToInt32(pvar: *const PROPVARIANT, pn: *i32): i32
    @lib pub fn PropVariantToInt64(pvar: *const PROPVARIANT, pn: *i64): i32
    @lib pub fn PropVariantToUInt32(pvar: *const PROPVARIANT, pn: *u32): i32
    @lib pub fn PropVariantToUInt64(pvar: *const PROPVARIANT, pn: *u64): i32
    @lib pub fn PropVariantToDouble(pvar: *const PROPVARIANT, pn: *f64): i32
    @lib pub fn PropVariantToBoolean(pvar: *const PROPVARIANT, pf: *i32): i32

    /// Read a string into a caller-supplied buffer (capped at
    /// `cch` wide chars). The result is always NUL-terminated.
    @lib pub fn PropVariantToString(
        pvar: *const PROPVARIANT,
        psz:  *u16,
        cch:  u32
    ): i32

    /// Read a string allocated by the system (caller frees with
    /// `CoTaskMemFree`).
    @lib pub fn PropVariantToStringAlloc(
        pvar: *const PROPVARIANT,
        ppsz: **u16
    ): i32

    // (Init helpers — `InitPropVariantFromInt32` and friends —
    //  are inline wrappers in propvarutil.h, NOT DLL exports.
    //  Build a PROPVARIANT manually:
    //    let v = PROPVARIANT {
    //        vt: 3 as u16,    // VT_I4
    //        _r0: 0 as u16, _r1: 0 as u16, _r2: 0 as u16,
    //        payload0: <value> as u64,
    //        payload1: 0 as u64,
    //    }
    //  Common vt values: VT_I2=2, VT_I4=3, VT_R4=4, VT_R8=5,
    //  VT_BOOL=11, VT_BSTR=8, VT_LPWSTR=31.)

    // ── Factories ───────────────────────────────────────────────

    /// Create a PROPERTYKEY → PROPVARIANT map. `riid` is the
    /// `IID_IPropertyStore` GUID; the returned interface lives on
    /// the heap until released.
    @lib pub fn PSCreateMemoryPropertyStore(
        riid:           *const void,
        ppv:            **void
    ): i32

    /// Build a multi-store proxy that fans queries across `cStores`
    /// inner stores.
    @lib pub fn PSCreateMultiplexPropertyStore(
        riid:           *const void,
        prgpunkStores:  **void,
        cStores:        u32,
        ppv:            **void
    ): i32

    /// Look up a PROPERTYKEY (well-known property GUID + integer
    /// id) by canonical name like "System.Title".
    @lib pub fn PSGetPropertyKeyFromName(
        pszName: *const u16,
        ppropkey: *void          // *PROPERTYKEY — 20 bytes
    ): i32

    /// Inverse: name from key. Returns a wide string (caller frees
    /// with `CoTaskMemFree`).
    @lib pub fn PSGetNameFromPropertyKey(
        propkey:   *const void,
        ppszCanonicalName: **u16
    ): i32
}
