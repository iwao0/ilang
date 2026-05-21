/// fwpuclnt.dll — Windows Filtering Platform (user-mode client).
///
/// WFP lets a privileged process add filters at various layers of
/// the network stack (ALE, INET4, TRANSPORT, …). Common scenarios:
///
///   * Block / allow specific connections (per-app firewall rules)
///   * Inspect traffic without rewriting it
///   * Add per-flow metadata
///
/// All call sites need an engine handle from `FwpmEngineOpen0`.
///
/// `FWP_ACTION_TYPE` values:
///   FWP_ACTION_BLOCK             = 0x1001
///   FWP_ACTION_PERMIT            = 0x1002
///   FWP_ACTION_CALLOUT_TERMINATING = 0x5003
///   FWP_ACTION_CALLOUT_INSPECTION  = 0x6004
///   FWP_ACTION_CALLOUT_UNKNOWN     = 0x4005
///   FWP_ACTION_CONTINUE          = 0x2006
///   FWP_ACTION_NONE              = 0x0007
///
/// `FWPM_SESSION_FLAG_*`:
///   DYNAMIC_SESSION = 0x1
///   RESERVED        = 0x10000000

pub enum FwpAction: u32 {
    block                = 0x1001
    permit               = 0x1002
    calloutTerminating   = 0x5003
    calloutInspection    = 0x6004
    calloutUnknown       = 0x4005
    continue_            = 0x2006
    none                 = 0x0007
}

@extern(C, "fwpuclnt") {
    /// Open a WFP engine handle. `serverName`: NULL for local.
    /// `authnService`: 0 = WINNT, 10 = NEGOTIATE (recommended).
    /// `authIdentity`: NULL = use current thread's token.
    /// `session`: optional FWPM_SESSION pointer (NULL = default).
    /// `engineHandle` (out) is closed via `FwpmEngineClose0`.
    @lib pub fn FwpmEngineOpen0(
        serverName:    *const u16,
        authnService:  u32,
        authIdentity:  *void,
        session:       *const void,
        engineHandle:  *i64
    ): u32

    @lib pub fn FwpmEngineClose0(engineHandle: i64): u32

    /// Begin a write transaction. `flags`:
    ///   FWPM_TXN_READ_ONLY = 0x1
    @lib pub fn FwpmTransactionBegin0(
        engineHandle: i64,
        flags:        u32
    ): u32

    @lib pub fn FwpmTransactionCommit0(engineHandle: i64): u32
    @lib pub fn FwpmTransactionAbort0(engineHandle: i64): u32

    /// Add a filter. `filter` is `*FWPM_FILTER0`; on success the
    /// new filter id is written to `*id`.
    @lib pub fn FwpmFilterAdd0(
        engineHandle: i64,
        filter:       *const void,
        sd:           *const void,    // security descriptor (NULL for default)
        id:           *u64
    ): u32

    @lib pub fn FwpmFilterDeleteById0(engineHandle: i64, id: u64): u32

    /// Add a sublayer (groups related filters; lets you reason
    /// about action precedence within a layer).
    @lib pub fn FwpmSubLayerAdd0(
        engineHandle: i64,
        subLayer:     *const void,    // *FWPM_SUBLAYER0
        sd:           *const void
    ): u32

    @lib pub fn FwpmSubLayerDeleteByKey0(
        engineHandle: i64,
        key:          *const void     // *GUID
    ): u32

    /// Add a provider (identifies the owner of filters / sublayers).
    @lib pub fn FwpmProviderAdd0(
        engineHandle: i64,
        provider:     *const void,    // *FWPM_PROVIDER0
        sd:           *const void
    ): u32

    @lib pub fn FwpmProviderDeleteByKey0(
        engineHandle: i64,
        key:          *const void
    ): u32

    /// Enumerate every filter. `enumHandle` is created by
    /// `FwpmFilterCreateEnumHandle0`.
    @lib pub fn FwpmFilterCreateEnumHandle0(
        engineHandle: i64,
        enumTemplate: *const void,   // *FWPM_FILTER_ENUM_TEMPLATE0; NULL = all
        enumHandle:   *i64
    ): u32

    @lib pub fn FwpmFilterEnum0(
        engineHandle: i64,
        enumHandle:   i64,
        numEntriesRequested: u32,
        entries:      ***void,
        numEntriesReturned: *u32
    ): u32

    @lib pub fn FwpmFilterDestroyEnumHandle0(
        engineHandle: i64,
        enumHandle:   i64
    ): u32

    /// Free memory allocated by an `FwpmXxxEnum0` / `Get*` call.
    @lib pub fn FwpmFreeMemory0(p: **void)
}
