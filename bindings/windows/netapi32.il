/// netapi32.dll — Windows network management: user / group / share /
/// session enumeration and configuration.
///
/// All these functions allocate output buffers internally and must be
/// freed with `NetApiBufferFree`. Names are UTF-16 — the historic API
/// has no ANSI variants for the modern functions.
///
/// Most functions return 0 (NERR_Success) on success, otherwise a
/// Win32 error code.

// ─── Filter masks for NetUserEnum (FILTER_*) ─────────────────────
@flags
pub enum UserFilter: u32 {
    tempDuplicateAccount = 0x0001
    normalAccount        = 0x0002
    interdomainTrustAccount = 0x0008
    workstationTrustAccount = 0x0010
    serverTrustAccount      = 0x0020
}

// ─── Share types (STYPE_*) ───────────────────────────────────────
@flags
pub enum ShareType: u32 {
    diskTree   = 0          // STYPE_DISKTREE
    printQ     = 1
    device     = 2
    ipc        = 3
    clusterFs  = 0x02000000
    clusterDfs = 0x04000000
    special    = 0x80000000   // hidden admin share (e.g. C$)
    temporary  = 0x40000000
}

// ─── NetUserAdd / NetUserGetInfo level common flags ──────────────
@flags
pub enum UserFlags: u32 {
    script               = 0x0001
    accountDisable       = 0x0002
    homedirRequired      = 0x0008
    lockout              = 0x0010
    passwdNotreqd        = 0x0020
    passwdCantChange     = 0x0040
    encryptedTextPasswordAllowed = 0x0080
    tempDuplicateAccount = 0x0100
    normalAccount        = 0x0200
    interdomainTrust     = 0x0800
    workstationTrust     = 0x1000
    serverTrust          = 0x2000
    dontExpirePasswd     = 0x10000
    mnsLogonAccount      = 0x20000
    smartcardRequired    = 0x40000
    trustedForDelegation = 0x80000
    notDelegated         = 0x100000
    useDesKeyOnly        = 0x200000
    dontRequirePreauth   = 0x400000
    passwordExpired      = 0x800000
    trustedToAuthenticateForDelegation = 0x1000000
}

// ─── Common privilege levels for USER_INFO_1.priv ────────────────
pub enum UserPrivilege: u32 {
    guest = 0
    user  = 1
    admin = 2
}

@extern(C, "netapi32") {
    // ── USER_INFO_0 — 8 bytes (just name pointer) ────────────────

    pub struct USER_INFO_0 {
        usri0_name: *u16
    }

    // ── USER_INFO_1 — 48 bytes on x64 ────────────────────────────

    pub struct USER_INFO_1 {
        usri1_name:        *u16
        usri1_password:    *u16     // NULL on output
        usri1_password_age: u32     // seconds since last change
        _pad:              u32
        usri1_priv:        u32      // UserPrivilege.*
        _pad2:             u32
        usri1_home_dir:    *u16
        usri1_comment:     *u16
        usri1_flags:       u32      // UserFlags.*
        _pad3:             u32
        usri1_script_path: *u16
    }

    // ── SHARE_INFO_0 — 8 bytes ───────────────────────────────────

    pub struct SHARE_INFO_0 {
        shi0_netname: *u16
    }

    // ── SHARE_INFO_1 — 24 bytes ──────────────────────────────────

    pub struct SHARE_INFO_1 {
        shi1_netname: *u16
        shi1_type:    u32
        _pad:         u32
        shi1_remark:  *u16
    }

    // ── SHARE_INFO_2 — 56 bytes ──────────────────────────────────

    pub struct SHARE_INFO_2 {
        shi2_netname:        *u16
        shi2_type:           u32
        _pad:                u32
        shi2_remark:         *u16
        shi2_permissions:    u32
        shi2_max_uses:       u32
        shi2_current_uses:   u32
        _pad2:               u32
        shi2_path:           *u16
        shi2_passwd:         *u16
    }

    // ── WKSTA_INFO_100 — 24 bytes ────────────────────────────────

    pub struct WKSTA_INFO_100 {
        wki100_platform_id:    u32
        _pad:                  u32
        wki100_computername:   *u16
        wki100_langroup:       *u16
        wki100_ver_major:      u32
        wki100_ver_minor:      u32
    }

    // ── SERVER_INFO_100 — 16 bytes ───────────────────────────────

    pub struct SERVER_INFO_100 {
        sv100_platform_id: u32
        _pad:              u32
        sv100_name:        *u16
    }

    // ── User enumeration / management ────────────────────────────

    /// Enumerates users. `level`: 0 (names) / 1 (with flags).
    /// `filter`: UserFilter.* (0 for all).
    /// `bufptr`: receives a pointer to an array of USER_INFO_<level>.
    /// `prefmaxlen`: 0xFFFFFFFF for "no preferred max".
    /// `entriesread` / `totalentries`: receives counts.
    /// `resume_handle`: pass 0 the first time; receives a continuation token.
    /// Returns 0 on completion or 234 (ERROR_MORE_DATA) when more remains.
    /// Free `*bufptr` with `NetApiBufferFree`.
    @lib pub fn NetUserEnum(
        servername:    *const u16,
        level:         u32,
        filter:        u32,
        bufptr:        **void,
        prefmaxlen:    u32,
        entriesread:   *u32,
        totalentries:  *u32,
        resume_handle: *u32
    ): u32

    @lib pub fn NetUserGetInfo(
        servername: *const u16,
        username:   *const u16,
        level:      u32,
        bufptr:     **void
    ): u32

    @lib pub fn NetUserAdd(
        servername:   *const u16,
        level:        u32,
        buf:          *const void,
        parm_err:     *u32
    ): u32

    @lib pub fn NetUserDel(servername: *const u16, username: *const u16): u32

    @lib pub fn NetUserChangePassword(
        domainname:   *const u16,
        username:     *const u16,
        oldpassword:  *const u16,
        newpassword:  *const u16
    ): u32

    // ── Share enumeration / management ───────────────────────────

    @lib pub fn NetShareEnum(
        servername:    *const u16,
        level:         u32,
        bufptr:        **void,
        prefmaxlen:    u32,
        entriesread:   *u32,
        totalentries:  *u32,
        resume_handle: *u32
    ): u32

    @lib pub fn NetShareGetInfo(
        servername: *const u16,
        netname:    *const u16,
        level:      u32,
        bufptr:     **void
    ): u32

    @lib pub fn NetShareAdd(
        servername: *const u16,
        level:      u32,
        buf:        *const void,
        parm_err:   *u32
    ): u32

    @lib pub fn NetShareDel(
        servername: *const u16,
        netname:    *const u16,
        reserved:   u32
    ): u32

    // ── Workstation / server info ────────────────────────────────

    @lib pub fn NetWkstaGetInfo(
        servername: *const u16,
        level:      u32,
        bufptr:     **void
    ): u32

    @lib pub fn NetServerEnum(
        servername:    *const u16,
        level:         u32,
        bufptr:        **void,
        prefmaxlen:    u32,
        entriesread:   *u32,
        totalentries:  *u32,
        servertype:    u32,        // SV_TYPE_* mask, 0xFFFFFFFF for all
        domain:        *const u16,
        resume_handle: *u32
    ): u32

    @lib pub fn NetServerGetInfo(
        servername: *const u16,
        level:      u32,
        bufptr:     **void
    ): u32

    // ── Sessions / open files / connections (admin) ──────────────

    @lib pub fn NetSessionEnum(
        servername:    *const u16,
        UncClientName: *const u16,
        username:      *const u16,
        level:         u32,
        bufptr:        **void,
        prefmaxlen:    u32,
        entriesread:   *u32,
        totalentries:  *u32,
        resume_handle: *u32
    ): u32

    @lib pub fn NetSessionDel(
        servername: *const u16,
        UncClientName: *const u16,
        username:   *const u16
    ): u32

    // ── Domain / DC discovery ─────────────────────────────────────

    /// Returns the name of the primary domain controller for `domain`.
    /// `bufptr`: receives a UTF-16 string (`\\\\PDCNAME`), free with NetApiBufferFree.
    @lib pub fn NetGetDCName(
        servername: *const u16,
        domainname: *const u16,
        bufptr:     **u16
    ): u32

    /// Returns information about the user's primary domain.
    @lib pub fn NetGetJoinInformation(
        lpServer:        *const u16,
        lpNameBuffer:    **u16,
        BufferType:      *u32       // 1=NetSetupUnknown, 2=NetSetupUnjoined, 3=Workgroup, 4=Domain
    ): u32

    // ── Buffer management ────────────────────────────────────────

    /// Frees a buffer returned by any Net* function.
    @lib pub fn NetApiBufferFree(Buffer: *void): u32
}
