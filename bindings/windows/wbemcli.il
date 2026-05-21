/// wbemcli.dll — WMI (Windows Management Instrumentation) client.
///
/// WMI is COM-only — no flat C exports. The CLSID + IIDs below
/// drive the standard CoCreateInstance bootstrap.
///
/// Bring-up:
///   CoInitializeEx(NULL, COINIT_MULTITHREADED)
///   CoCreateInstance(&CLSID_WbemLocator, NULL,
///                    CLSCTX_INPROC_SERVER, &IID_IWbemLocator,
///                    &locator)
///   locator.ConnectServer(L"ROOT\\CIMV2", ...,
///                         &services)
///   CoSetProxyBlanket(services, RPC_C_AUTHN_DEFAULT,
///                     RPC_C_AUTHZ_NONE, NULL,
///                     RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
///                     NULL, EOAC_NONE)
///   services.ExecQuery(L"WQL", L"SELECT * FROM Win32_Process",
///                      WBEM_FLAG_FORWARD_ONLY, NULL, &enum)
///
/// Use ole32.IUnknown for AddRef / Release. The vtable shapes for
/// IWbemLocator / IWbemServices / IWbemClassObject / IEnumWbemClassObject
/// are not yet bound at the ilang call level.

// ─── WBEM_GENERIC_FLAG_TYPE ──────────────────────────────────────
@flags
pub enum WbemGenericFlag: u32 {
    returnImmediately    = 0x0010
    returnWbemComplete   = 0x0000
    bidirectional        = 0x0000
    forwardOnly          = 0x0020
    noErrorObject        = 0x0040
    returnErrorObject    = 0x0000
    sendStatus           = 0x0080
    dontSendStatus       = 0x0000
    ensureLocatable      = 0x0100
    connectRepositoryOnly = 0x0040
}

// ─── WBEM_CONDITION_FLAG_TYPE (CIMTYPE) ──────────────────────────
@flags
pub enum WbemConditionFlag: u32 {
    alwaysFalse        = 0
    alwaysTrue         = 1
    onlyIfTrue         = 0x1
    onlyIfFalse        = 0x2
    onlyIfIdentical    = 0x3
    maskPrimaryCondition = 0x3
}

// ─── WBEM_STATUS — call result codes ─────────────────────────────
pub enum WbemStatus: u32 {
    success            = 0
    sFalse             = 1
    noError            = 0
    failed             = 0x80041001
    notFound           = 0x80041002
    accessDenied       = 0x80041003
    providerFailure    = 0x80041004
    typeMismatch       = 0x80041005
    outOfMemory        = 0x80041006
    invalidContext     = 0x80041007
    invalidParameter   = 0x80041008
    notAvailable       = 0x80041009
    criticalError      = 0x8004100A
    transportFailure   = 0x80041015
    invalidOperation   = 0x80041016
    invalidQuery       = 0x80041017
    invalidQueryType   = 0x80041018
    queryNotImplemented = 0x80041019
    invalidObject      = 0x8004102D
    invalidClass       = 0x80041010
}

// ─── CIM type tags (CIMTYPE_ENUMERATION) ─────────────────────────
pub enum CimType: i32 {
    illegal     = 0x0FFF
    empty       = 0x0
    sint8       = 0x10
    uint8       = 0x11
    sint16      = 0x2
    uint16      = 0x12
    sint32      = 0x3
    uint32      = 0x13
    sint64      = 0x14
    uint64      = 0x15
    real32      = 0x4
    real64      = 0x5
    boolean     = 0xB
    string      = 0x8
    datetime    = 0x65
    reference   = 0x66
    char16      = 0x67
    object      = 0xD
    flagArray   = 0x2000
}

/// Documentation-only — no C entry points. Wire via CoCreateInstance.
///
/// CLSID_WbemLocator:
///   {4590F811-1D3A-11D0-891F-00AA004B2E24}
///
/// IID_IWbemLocator:
///   {DC12A687-737F-11CF-884D-00AA004B2E24}
///
/// IID_IWbemServices:
///   {9556DC99-828C-11CF-A37E-00AA003240C7}
///
/// IID_IWbemClassObject:
///   {DC12A681-737F-11CF-884D-00AA004B2E24}
///
/// IID_IEnumWbemClassObject:
///   {027947E1-D731-11CE-A357-000000000001}
///
/// IID_IWbemCallResult:
///   {44ACA675-E8FC-11D0-A07C-00C04FB68820}
///
/// IID_IWbemContext:
///   {44ACA674-E8FC-11D0-A07C-00C04FB68820}
// (WMI has no flat C function surface. CLSID / IIDs above are
//  used with ole32's CoCreateInstance + CoSetProxyBlanket.)
