// mpr.dll — Multiple Provider Router: Windows networking facade
// over SMB / NetWare / WebDAV / generic network providers.
//
// Used to map / unmap network drives, list available shares, and
// resolve UNC paths to credentials.

// ─── NETRESOURCE dwScope values (RESOURCE_*) ─────────────────────
pub enum WNetScope: u32 {
    connected   = 0x00000001
    globalnet   = 0x00000002
    remembered  = 0x00000003
    recent      = 0x00000004
    context     = 0x00000005
}

// ─── NETRESOURCE dwType (RESOURCETYPE_*) ─────────────────────────
pub enum WNetType: u32 {
    any         = 0x00000000
    disk        = 0x00000001
    print       = 0x00000002
    reserved    = 0x00000008
    unknown     = 0xFFFFFFFF
}

// ─── NETRESOURCE dwUsage flags (RESOURCEUSAGE_*) ─────────────────
@flags
pub enum WNetUsage: u32 {
    connectable      = 0x00000001
    container        = 0x00000002
    noLocalDevice    = 0x00000004
    sibling          = 0x00000008
    attached         = 0x00000010
    all              = 0x00000013
    reserved         = 0x80000000
}

// ─── WNetAddConnection2 dwFlags (CONNECT_*) ──────────────────────
@flags
pub enum WNetConnectFlags: u32 {
    /// Save the connection in the user's profile.
    updateProfile      = 0x00000001
    interactive        = 0x00000008
    prompt             = 0x00000010
    redirect           = 0x00000080
    /// Use the current default credentials.
    currentMedia       = 0x00000200
    commandLine        = 0x00000800
    cmdSaveCred        = 0x00001000
    temporary          = 0x00000004
}

@extern(C, "mpr") {
    // ── NETRESOURCE — 64 bytes on x64 ────────────────────────────

    /// Describes a network resource (share, drive letter, container).
    /// For `WNetAddConnection2A` you typically set `dwType`,
    /// `lpLocalName` (e.g. "Z:"), and `lpRemoteName` (e.g. "\\\\srv\\share").
    pub struct NETRESOURCE {
        dwScope:       u32
        dwType:        u32
        dwDisplayType: u32
        dwUsage:       u32
        lpLocalName:   *char       // "Z:" or NULL for deviceless
        lpRemoteName:  *char       // "\\\\server\\share"
        lpComment:     *char
        lpProvider:    *char
    }

    // ── Connection management ─────────────────────────────────────

    /// Connects to a network resource.
    /// `dwFlags`: WNetConnectFlags.* (e.g. .updateProfile to persist).
    /// `lpPassword`: NULL for default; "" for empty password.
    /// `lpUserName`: NULL for current user.
    @lib pub fn WNetAddConnection2A(
        lpNetResource: *const NETRESOURCE,
        lpPassword:    *const char,
        lpUserName:    *const char,
        dwFlags:       u32
    ): u32

    /// Simpler legacy variant — `lpName` is the local device,
    /// `lpRemoteName` is the UNC share.
    @lib pub fn WNetAddConnectionA(
        lpRemoteName: *const char,
        lpPassword:   *const char,
        lpLocalName:  *const char
    ): u32

    /// Disconnects a resource. `lpName`: local device or remote name.
    /// `dwFlags`: WNetConnectFlags.* (.updateProfile to drop the
    /// persistent entry as well).
    /// `fForce`: 1 = drop even if files are open.
    @lib pub fn WNetCancelConnection2A(
        lpName:    *const char,
        dwFlags:   u32,
        fForce:    i32
    ): u32

    @lib pub fn WNetCancelConnectionA(
        lpName:    *const char,
        fForce:    i32
    ): u32

    /// Returns the UNC remote name for a connected local device
    /// (e.g. "Z:" → "\\\\server\\share").
    @lib pub fn WNetGetConnectionA(
        lpLocalName:  *const char,
        lpRemoteName: *char,
        lpnLength:    *u32
    ): u32

    /// Returns the user name that established the connection.
    @lib pub fn WNetGetUserA(
        lpName:    *const char,
        lpUserName: *char,
        lpnLength: *u32
    ): u32

    // ── Enumeration ───────────────────────────────────────────────

    /// Opens an enumeration of network resources.
    /// `dwScope`: WNetScope.*, `dwType`: WNetType.*, `dwUsage`: WNetUsage.*.
    /// `lpNetResource`: NULL to enumerate from the root of the network.
    /// `phEnum`: receives a handle to pass to `WNetEnumResourceA`.
    @lib pub fn WNetOpenEnumA(
        dwScope:       u32,
        dwType:        u32,
        dwUsage:       u32,
        lpNetResource: *const NETRESOURCE,
        phEnum:        **void
    ): u32

    /// Reads the next batch. `lpcCount`: in = max entries, out = actual.
    /// `lpBuffer`: receives `*lpcCount` NETRESOURCE records.
    /// Returns 259 (ERROR_NO_MORE_ITEMS) at end of enumeration.
    @lib pub fn WNetEnumResourceA(
        hEnum:        *void,
        lpcCount:     *u32,
        lpBuffer:     *void,
        lpBufferSize: *u32
    ): u32

    @lib pub fn WNetCloseEnum(hEnum: *void): u32

    /// Returns information about a network resource without
    /// connecting to it.
    @lib pub fn WNetGetResourceInformationA(
        lpNetResource:  *const NETRESOURCE,
        lpBuffer:       *void,
        lpcbBuffer:     *u32,
        lplpSystem:     **char
    ): u32

    // ── Error reporting ──────────────────────────────────────────

    /// Returns extended error info about the last failed WNet call.
    @lib pub fn WNetGetLastErrorA(
        lpError:        *u32,
        lpErrorBuf:     *char,
        nErrorBufSize:  u32,
        lpNameBuf:      *char,
        nNameBufSize:   u32
    ): u32
}
