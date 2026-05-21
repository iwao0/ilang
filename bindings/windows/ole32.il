/// ole32.dll — Component Object Model (COM) foundation.
///
/// Usage pattern:
///   1. CoInitializeEx(0 as *void, CoInit.apartmentThreaded as u32)
///   2. CoCreateInstance(&CLSID, 0 as *void, ClsCtx.inprocServer as u32,
///                       &IID, &ppv as **void)
///   3. Use the object through its vtable (cast ppv to a struct of fn pointers).
///   4. IUnknown.Release() when done.
///   5. CoUninitialize()

// ─── CoInitializeEx thread model (COINIT_*) ──────────────────────
@flags
pub enum CoInit: u32 {
    apartmentThreaded = 0x00000002   // STA — default for UI threads
    multiThreaded     = 0x00000000   // MTA
    disableOle1dde    = 0x00000004
    speedOverMemory   = 0x00000008
}

// ─── Class context flags (CLSCTX_*) ──────────────────────────────
@flags
pub enum ClsCtx: u32 {
    inprocServer     = 0x00000001   // in-process DLL server
    inprocHandler    = 0x00000002
    localServer      = 0x00000004   // out-of-process EXE server
    inprocServer16   = 0x00000008
    remoteServer     = 0x00000010
    inprocHandler16  = 0x00000020
    reserved1        = 0x00000040
    reserved2        = 0x00000080
    reserved3        = 0x00000100
    reserved4        = 0x00000200
    noCodeDownload   = 0x00000400
    reserved5        = 0x00000800
    noCustomMarshal  = 0x00001000
    enableCodeDownload = 0x00002000
    noFailureLog     = 0x00004000
    disableAaa       = 0x00008000
    enableAaa        = 0x00010000
    fromDefaultContext = 0x00020000
    activate32BitServer = 0x00040000
    activate64BitServer = 0x00080000
    enableCloaking   = 0x00100000
    appContainer     = 0x00400000
    activateAaaAsIu  = 0x00800000
    psClsid          = 0x01000000
    fromContractId   = 0x02000000
    unusedMask       = 0xFC000000
    /// All local contexts: inprocServer | inprocHandler | localServer.
    inproc           = 0x00000003
    /// All server contexts: inprocServer | localServer | remoteServer.
    server           = 0x00000015
    all              = 0x00000017
}

// ─── Common HRESULT values ────────────────────────────────────────
pub enum HResult: i32 {
    ok                = 0x00000000   // S_OK
    false_            = 0x00000001   // S_FALSE
    notImpl           = -2147467263  // E_NOTIMPL      0x80004001
    noInterface       = -2147467262  // E_NOINTERFACE  0x80004002
    pointer           = -2147467261  // E_POINTER      0x80004003
    abort             = -2147467260  // E_ABORT        0x80004004
    fail              = -2147467259  // E_FAIL         0x80004005
    unexpected        = -2147418113  // E_UNEXPECTED   0x8000FFFF
    invalidArg        = -2147024809  // E_INVALIDARG   0x80070057
    outOfMemory       = -2147024882  // E_OUTOFMEMORY  0x8007000E
    accessDenied      = -2147024891  // E_ACCESSDENIED 0x80070005
    handle            = -2147024890  // E_HANDLE       0x80070006
    rpcChangedMode    = -2147417850  // RPC_E_CHANGED_MODE
    classNotReg       = -2147221164  // REGDB_E_CLASSNOTREG
    noCls             = -2147221231  // CO_E_NOTINITIALIZED
}

// ─── REGCLS flags for CoRegisterClassObject ───────────────────────
@flags
pub enum RegCls: u32 {
    singleuse     = 0
    multipleuse   = 1
    multi_separate = 2
    suspended     = 4
    surrogate     = 8
}

// ─── TYMED (media for data transfer) ─────────────────────────────
@flags
pub enum Tymed: u32 {
    hglobal   = 1
    file      = 2
    istream   = 4
    istorage  = 8
    gdi       = 16
    mfpict    = 32
    enhmf     = 64
    null      = 0
}

// ─── DVASPECT (data/view aspect) ─────────────────────────────────
@flags
pub enum DvAspect: u32 {
    content   = 1
    thumbnail = 2
    icon      = 4
    docprint  = 8
}

@extern(C, "ole32") {
    // ── GUID — 16 bytes ───────────────────────────────────────────

    /// Globally Unique Identifier.  Also used as CLSID and IID.
    /// Initialise with literal values or parse from a string with
    /// `CLSIDFromString`.
    pub struct GUID {
        Data1: u32
        Data2: u16
        Data3: u16
        // Data4[8] — stored as two u32 words; low byte of Data4[0]
        // is the first byte when transmitted over the network.
        Data4lo: u32
        Data4hi: u32
    }

    // ── IUnknown — every COM interface starts here ────────────────
    //
    // Declared once in ole32 (the COM foundation) so dxgi / d3d12 /
    // wasapi / any future COM binding shares a single identity.
    // Cross-module casts (`device as IUnknown` etc.) work without
    // detouring through `*void`.
    @com pub interface IUnknown {
        QueryInterface(riid: *const GUID, ppvObject: **void): i32
        AddRef(): u32
        Release(): u32
    }

    // ── STATSTG — 72 bytes on x64 ────────────────────────────────

    /// Stream / storage statistics returned by IStream::Stat.
    pub struct STATSTG {
        pwcsName:         *u16   // CoTaskMemFree when done (if STATFLAG_DEFAULT)
        type_:            u32
        cbSize_lo:        u32    // ULARGE_INTEGER lo
        cbSize_hi:        u32    // ULARGE_INTEGER hi
        mtime_lo:         u32    // FILETIME lo
        mtime_hi:         u32
        ctime_lo:         u32
        ctime_hi:         u32
        atime_lo:         u32
        atime_hi:         u32
        grfMode:          u32
        grfLocksSupported: u32
        clsid_d1:         u32
        clsid_d2:         u16
        clsid_d3:         u16
        clsid_d4lo:       u32
        clsid_d4hi:       u32
        grfStateBits:     u32
        reserved:         u32
    }

    // ── CoTask memory ─────────────────────────────────────────────

    /// Allocates COM task memory (the allocator shared between COM
    /// components). Free with `CoTaskMemFree`.
    @lib pub fn CoTaskMemAlloc(cb: u64): *void

    @lib pub fn CoTaskMemFree(pv: *void)

    @lib pub fn CoTaskMemRealloc(pv: *void, cb: u64): *void

    // ── Initialisation ────────────────────────────────────────────

    /// Initialises COM for the calling thread.
    /// `pvReserved`: must be `0 as *void`.
    /// `dwCoInit`: CoInit.* (apartmentThreaded or multiThreaded).
    @lib pub fn CoInitializeEx(pvReserved: *void, dwCoInit: u32): i32

    @lib pub fn CoInitialize(pvReserved: *void): i32

    @lib pub fn CoUninitialize()

    // ── Object creation ───────────────────────────────────────────

    /// Creates a COM object.
    /// `rclsid`: pointer to the CLSID of the class.
    /// `pUnkOuter`: NULL for no aggregation.
    /// `dwClsContext`: ClsCtx.*.
    /// `riid`: pointer to the desired interface IID (e.g. IID_IUnknown).
    /// `ppv`: receives the interface pointer.
    @lib pub fn CoCreateInstance(
        rclsid:      *const GUID,
        pUnkOuter:   *void,
        dwClsContext: u32,
        riid:        *const GUID,
        ppv:         **void
    ): i32

    @lib pub fn CoCreateInstanceEx(
        rclsid:       *const GUID,
        punkOuter:    *void,
        dwClsCtx:     u32,
        pServerInfo:  *void,   // *COSERVERINFO
        dwCount:      u32,
        rgmqResults:  *void    // *MULTI_QI
    ): i32

    // ── GUID / CLSID / IID helpers ────────────────────────────────

    /// Converts a CLSID to its string form "{xxxxxxxx-…}".
    /// Free `lplpsz` with `CoTaskMemFree`.
    @lib pub fn StringFromCLSID(rclsid: *const GUID, lplpsz: **u16): i32

    @lib pub fn StringFromIID(rclsid: *const GUID, lplpsz: **u16): i32

    /// Converts a string "{xxxxxxxx-…}" back to a CLSID.
    @lib pub fn CLSIDFromString(lpsz: *const u16, pclsid: *GUID): i32

    @lib pub fn IIDFromString(lpsz: *const u16, lpiid: *GUID): i32

    /// Writes the GUID string into `lpsz` (must be at least 39 u16).
    /// Returns the number of characters written (including NUL).
    @lib pub fn StringFromGUID2(rguid: *const GUID, lpsz: *u16, cchMax: i32): i32

    @lib pub fn IsEqualGUID(rguid1: *const GUID, rguid2: *const GUID): i32

    // ── Class object registration ─────────────────────────────────

    /// Registers a class factory. Returns a cookie in `lpdwRegister`.
    @lib pub fn CoRegisterClassObject(
        rclsid:       *const GUID,
        pUnk:         *void,   // *IUnknown
        dwClsContext: u32,
        flags:        u32,     // RegCls.*
        lpdwRegister: *u32
    ): i32

    @lib pub fn CoRevokeClassObject(dwRegister: u32): i32

    // ── Marshalling / IPC ─────────────────────────────────────────

    /// Retrieves the current thread's apartment type.
    /// `pAptType` receives: 0=STA, 1=MTA, 2=NA, 3=implicit MTA.
    @lib pub fn CoGetApartmentType(pAptType: *i32, pAptQualifier: *i32): i32

    /// Gets the class name for a CLSID (reads from the registry).
    @lib pub fn ProgIDFromCLSID(clsid: *const GUID, lplpszProgID: **u16): i32
    @lib pub fn CLSIDFromProgID(lpszProgID: *const u16, lpclsid: *GUID): i32

}
