// rpcrt4.dll — Microsoft RPC runtime.
//
// In practice the most-used parts are the UUID helpers — needed any
// time you mint a COM CLSID, a DCE/RPC interface ID, or just want a
// platform-native GUID.

// ─── RPC_STATUS return codes (selected) ──────────────────────────
pub enum RpcStatus: i32 {
    ok                = 0     // RPC_S_OK
    invalidArg        = 87
    outOfMemory       = 14
    invalidBinding    = 1702
    serverUnavailable = 1722
    callFailed        = 1726
    uuidLocalOnly     = 1824    // RPC_S_UUID_LOCAL_ONLY
    uuidNoAddress     = 1739    // RPC_S_UUID_NO_ADDRESS
    stringTooLong     = 1707
    invalidStringUuid = 1705
}

@extern(C, "rpcrt4") {
    // ── UUID — 16 bytes (identical layout to GUID) ───────────────

    /// Universally unique identifier. Bit-identical to ole32's GUID
    /// (same `Data1/2/3/4` shape). Pass either type to either lib.
    pub struct UUID {
        Data1:   u32
        Data2:   u16
        Data3:   u16
        Data4lo: u32
        Data4hi: u32
    }

    // ── UUID generation ───────────────────────────────────────────

    /// Generates a new UUID (v4-style random on modern Windows).
    /// Returns RPC_S_OK (0) on success.
    @lib pub fn UuidCreate(Uuid: *UUID): i32

    /// Generates a UUID that is GUARANTEED unique even when the
    /// network adapter has no MAC address — Windows falls back to
    /// time + random data. Returns RPC_S_UUID_LOCAL_ONLY (1824) if
    /// the result is local-only, but the UUID is still valid.
    @lib pub fn UuidCreateSequential(Uuid: *UUID): i32

    /// Returns the all-zeroes "nil" UUID.
    @lib pub fn UuidCreateNil(Uuid: *UUID): i32

    // ── UUID comparison / hashing ─────────────────────────────────

    /// Returns 1 if the two UUIDs are equal, 0 otherwise.
    /// `Status`: output, set to RPC_S_OK on success.
    @lib pub fn UuidEqual(Uuid1: *const UUID, Uuid2: *const UUID, Status: *i32): i32

    /// Returns 1 if `Uuid` is the nil UUID, 0 otherwise.
    @lib pub fn UuidIsNil(Uuid: *const UUID, Status: *i32): i32

    /// Returns a small hash of the UUID (good for hash-table keys).
    @lib pub fn UuidHash(Uuid: *const UUID, Status: *i32): u16

    /// Compares two UUIDs; returns -1 / 0 / 1.
    @lib pub fn UuidCompare(Uuid1: *const UUID, Uuid2: *const UUID, Status: *i32): i32

    // ── UUID ↔ string ─────────────────────────────────────────────

    /// Allocates a new ASCII string ("xxxxxxxx-xxxx-…") for the UUID.
    /// `StringUuid` receives a heap-allocated buffer; free with
    /// `RpcStringFreeA`.
    @lib pub fn UuidToStringA(Uuid: *const UUID, StringUuid: **char): i32

    /// Parses an ASCII UUID string back into a UUID.
    @lib pub fn UuidFromStringA(StringUuid: *const char, Uuid: *UUID): i32

    /// Frees a string returned by UuidToStringA / similar.
    /// Sets `*String` to NULL when it returns.
    @lib pub fn RpcStringFreeA(String: **char): i32

    // ── Error info ────────────────────────────────────────────────

    /// Returns information about the last RPC exception that occurred
    /// in the calling thread, into a pre-allocated RPC_ERROR_ENQ_PARAMS
    /// (rarely needed for non-RPC code).
    @lib pub fn RpcMgmtSetCancelTimeout(Timeout: i32): i32
}
