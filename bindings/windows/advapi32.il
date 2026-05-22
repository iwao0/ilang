/// advapi32.dll — Advanced API services: registry, Windows services,
/// security tokens, event logging, and LSA policy.

// ─── Predefined registry root keys ───────────────────────────────
// Use as `0x80000000 as i64 as HKEY` etc.
pub enum PredefinedKey: u64 {
    classesRoot   = 0x80000000
    currentUser   = 0x80000001
    localMachine  = 0x80000002
    users         = 0x80000003
    performanceData = 0x80000004
    currentConfig = 0x80000005
    dynData       = 0x80000006   // Win9x only; included for completeness
}

// ─── Registry key access rights (KEY_*) ──────────────────────────
@flags
pub enum KeyAccess: u32 {
    queryValue       = 0x0001
    setValue         = 0x0002
    createSubKey     = 0x0004
    enumerateSubKeys = 0x0008
    notify           = 0x0010
    createLink       = 0x0020
    wow64_32key      = 0x0200
    wow64_64key      = 0x0100
    /// Standard combo for reading: queryValue | enumerateSubKeys | notify.
    read             = 0x20019
    /// Standard combo for writing: setValue | createSubKey.
    write            = 0x20006
    execute          = 0x20019   // = read
    allAccess        = 0xF003F
}

// ─── Registry value type codes (REG_*) ───────────────────────────
pub enum RegValueType: u32 {
    none                      = 0
    sz                        = 1    // NUL-terminated ANSI/UTF-8
    expandSz                  = 2    // NUL-terminated with %env% refs
    binary                    = 3
    dword                     = 4    // 32-bit little-endian
    dwordBigEndian            = 5
    link                      = 6    // symbolic link (Unicode)
    multiSz                   = 7    // sequence of NUL-terminated strings
    resourceList              = 8
    fullResourceDescriptor    = 9
    resourceRequirementsList  = 10
    qword                     = 11   // 64-bit little-endian
}

// ─── RegCreateKeyExA open/create disposition ─────────────────────
pub enum RegDisposition: u32 {
    createdNewKey    = 0x00000001
    openedExistingKey = 0x00000002
}

// ─── RegCreateKeyExA options ─────────────────────────────────────
@flags
pub enum RegOptions: u32 {
    nonVolatile    = 0x00000000   // survives reboot (default)
    volatile       = 0x00000001   // lost on reboot
    createLink     = 0x00000002
    backupRestore  = 0x00000004
    openLink       = 0x00000008
}

// ─── RegNotifyChangeKeyValue notify filter ────────────────────────
@flags
pub enum RegNotifyFilter: u32 {
    changeName      = 0x00000001   // subkey added or deleted
    changeAttributes = 0x00000002
    changeLastSet   = 0x00000004   // value set / deleted
    changeSecurity  = 0x00000008
}

// ─── Service type flags (SERVICE_*) ──────────────────────────────
@flags
pub enum ServiceType: u32 {
    kernelDriver      = 0x00000001
    filesystemDriver  = 0x00000002
    adapter           = 0x00000004
    recognizerDriver  = 0x00000008
    win32OwnProcess   = 0x00000010
    win32ShareProcess = 0x00000020
    interactiveProcess = 0x00000100
}

// ─── Service start type (SERVICE_*_START) ────────────────────────
pub enum ServiceStartType: u32 {
    bootStart   = 0
    systemStart = 1
    autoStart   = 2
    demandStart = 3
    disabled    = 4
}

// ─── Service error control ────────────────────────────────────────
pub enum ServiceErrorControl: u32 {
    ignore   = 0
    normal   = 1
    severe   = 2
    critical = 3
}

// ─── Service current state (SERVICE_*) ───────────────────────────
pub enum ServiceState: u32 {
    stopped          = 1
    startPending     = 2
    stopPending      = 3
    running          = 4
    continuePending  = 5
    pausePending     = 6
    paused           = 7
}

// ─── Service control codes (SERVICE_CONTROL_*) ───────────────────
pub enum ServiceControl: u32 {
    stop         = 0x00000001
    pause        = 0x00000002
    continue_    = 0x00000003
    interrogate  = 0x00000004
    shutdown     = 0x00000005
    paramChange  = 0x00000006
    netBindAdd   = 0x00000007
    netBindRemove = 0x00000008
    netBindEnable = 0x00000009
    netBindDisable = 0x0000000A
}

// ─── Service controls accepted (SERVICE_ACCEPT_*) ────────────────
@flags
pub enum ServiceControlsAccepted: u32 {
    stop                  = 0x00000001
    pauseContinue         = 0x00000002
    shutdown              = 0x00000004
    paramChange           = 0x00000008
    netBindChange         = 0x00000010
    hardwareProfileChange = 0x00000020
    powerEvent            = 0x00000040
    sessionChange         = 0x00000080
    preshutdown           = 0x00000100
}

// ─── SCManager access rights ─────────────────────────────────────
@flags
pub enum ScManagerAccess: u32 {
    connect           = 0x0001
    createService     = 0x0002
    enumerateService  = 0x0004
    lock              = 0x0008
    queryLockStatus   = 0x0010
    modifyBootConfig  = 0x0020
    allAccess         = 0xF003F
}

// ─── Service access rights ────────────────────────────────────────
@flags
pub enum ServiceAccess: u32 {
    queryConfig       = 0x0001
    changeConfig      = 0x0002
    queryStatus       = 0x0004
    enumerateDependents = 0x0008
    start             = 0x0010
    stop              = 0x0020
    pauseContinue     = 0x0040
    interrogate       = 0x0080
    userDefinedControl = 0x0100
    allAccess         = 0xF01FF
}

// ─── Event log types ─────────────────────────────────────────────
pub enum EventLogType: u16 {
    success         = 0x0000
    error           = 0x0001
    warning         = 0x0002
    information     = 0x0004
    auditSuccess    = 0x0008
    auditFailure    = 0x0010
}

@extern(C, "advapi32") {
    // ── Opaque handles ────────────────────────────────────────────

    /// Registry key handle. The predefined roots are cast from
    /// the PredefinedKey enum: `PredefinedKey.currentUser as i64 as HKEY`.
    @handle pub struct HKEY {}

    /// Service Control Manager handle.
    pub struct SC_HANDLE {}

    /// Event log handle.
    @handle pub struct HEVENTLOG {}

    // ── SERVICE_STATUS — 28 bytes ─────────────────────────────────

    pub struct SERVICE_STATUS {
        dwServiceType:             u32
        dwCurrentState:            u32   // ServiceState.*
        dwControlsAccepted:        u32   // ServiceControlsAccepted.*
        dwWin32ExitCode:           u32
        dwServiceSpecificExitCode: u32
        dwCheckPoint:              u32
        dwWaitHint:                u32
    }

    // ── Registry ──────────────────────────────────────────────────

    /// Opens an existing subkey. `samDesired`: KeyAccess.*.
    /// Returns ERROR_SUCCESS (0) on success.
    @lib pub fn RegOpenKeyExA(
        hKey:       HKEY,
        lpSubKey:   *const char,
        ulOptions:  u32,
        samDesired: u32,
        phkResult:  *HKEY
    ): i32

    /// Creates or opens a subkey.
    /// `lpdwDisposition`: receives RegDisposition.createdNewKey or
    /// .openedExistingKey. Pass `0 as *u32` if not needed.
    @lib pub fn RegCreateKeyExA(
        hKey:                HKEY,
        lpSubKey:            *const char,
        Reserved:            u32,
        lpClass:             *char,
        dwOptions:           u32,         // RegOptions.*
        samDesired:          u32,         // KeyAccess.*
        lpSecurityAttributes: *void,
        phkResult:           *HKEY,
        lpdwDisposition:     *u32
    ): i32

    @lib pub fn RegCloseKey(hKey: HKEY): i32

    /// Reads a value. Pass `0 as *u32` for `lpType` / `0 as *void`
    /// for `lpData` / `0 as *u32` for `lpcbData` to query size.
    @lib pub fn RegQueryValueExA(
        hKey:        HKEY,
        lpValueName: *const char,
        lpReserved:  *u32,
        lpType:      *u32,       // receives RegValueType.*
        lpData:      *void,
        lpcbData:    *u32
    ): i32

    /// Writes a value. `dwType`: RegValueType.*, `cbData`: byte count
    /// including NUL for string types.
    @lib pub fn RegSetValueExA(
        hKey:        HKEY,
        lpValueName: *const char,
        Reserved:    u32,
        dwType:      u32,
        lpData:      *const void,
        cbData:      u32
    ): i32

    @lib pub fn RegDeleteValueA(hKey: HKEY, lpValueName: *const char): i32
    @lib pub fn RegDeleteKeyA(hKey: HKEY, lpSubKey: *const char): i32

    /// Enumerates subkeys. `dwIndex` starts at 0 and increments until
    /// ERROR_NO_MORE_ITEMS (259) is returned.
    @lib pub fn RegEnumKeyExA(
        hKey:          HKEY,
        dwIndex:       u32,
        lpName:        *char,
        lpcName:       *u32,
        lpReserved:    *u32,
        lpClass:       *char,
        lpcClass:      *u32,
        lpftLastWriteTime: *void   // *FILETIME
    ): i32

    /// Enumerates values. `dwIndex` starts at 0.
    @lib pub fn RegEnumValueA(
        hKey:          HKEY,
        dwIndex:       u32,
        lpValueName:   *char,
        lpcchValueName: *u32,
        lpReserved:    *u32,
        lpType:        *u32,
        lpData:        *void,
        lpcbData:      *u32
    ): i32

    @lib pub fn RegFlushKey(hKey: HKEY): i32

    /// Watches a key for changes. `hEvent` is signalled when a change
    /// occurs. Pass `0 as *HANDLE` to use asynchronous notification.
    @lib pub fn RegNotifyChangeKeyValue(
        hKey:          HKEY,
        bWatchSubtree: i32,
        dwNotifyFilter: u32,       // RegNotifyFilter.*
        hEvent:        *void,      // *HANDLE
        fAsynchronous: i32
    ): i32

    @lib pub fn RegConnectRegistryA(
        lpMachineName: *const char,
        hKey:          HKEY,
        phkResult:     *HKEY
    ): i32

    // ── Service Control Manager ───────────────────────────────────

    /// Opens the SCM on `lpMachineName` (NULL = local machine).
    /// `dwDesiredAccess`: ScManagerAccess.*.
    @lib pub fn OpenSCManagerA(
        lpMachineName:   *const char,
        lpDatabaseName:  *const char,
        dwDesiredAccess: u32
    ): *SC_HANDLE

    @lib pub fn OpenServiceA(
        hSCManager:      *SC_HANDLE,
        lpServiceName:   *const char,
        dwDesiredAccess: u32         // ServiceAccess.*
    ): *SC_HANDLE

    @lib pub fn CreateServiceA(
        hSCManager:           *SC_HANDLE,
        lpServiceName:        *const char,
        lpDisplayName:        *const char,
        dwDesiredAccess:      u32,
        dwServiceType:        u32,
        dwStartType:          u32,
        dwErrorControl:       u32,
        lpBinaryPathName:     *const char,
        lpLoadOrderGroup:     *const char,
        lpdwTagId:            *u32,
        lpDependencies:       *const char,
        lpServiceStartName:   *const char,
        lpPassword:           *const char
    ): *SC_HANDLE

    @lib pub fn StartServiceA(
        hService:            *SC_HANDLE,
        dwNumServiceArgs:    u32,
        lpServiceArgVectors: **const char
    ): i32

    @lib pub fn ControlService(
        hService:        *SC_HANDLE,
        dwControl:       u32,         // ServiceControl.*
        lpServiceStatus: *SERVICE_STATUS
    ): i32

    @lib pub fn QueryServiceStatus(
        hService:        *SC_HANDLE,
        lpServiceStatus: *SERVICE_STATUS
    ): i32

    @lib pub fn DeleteService(hService: *SC_HANDLE): i32
    @lib pub fn CloseServiceHandle(hSCObject: *SC_HANDLE): i32

    // ── Event Log ─────────────────────────────────────────────────

    /// Opens the event log. `lpSourceName`: "Application", "System", etc.
    @lib pub fn OpenEventLogA(
        lpUNCServerName: *const char,
        lpSourceName:    *const char
    ): HEVENTLOG

    @lib pub fn CloseEventLog(hEventLog: HEVENTLOG): i32

    /// Registers an event source handle for `ReportEventA`.
    @lib pub fn RegisterEventSourceA(
        lpUNCServerName: *const char,
        lpSourceName:    *const char
    ): HEVENTLOG

    @lib pub fn DeregisterEventSource(hEventLog: HEVENTLOG): i32

    /// Writes an event to the log.
    /// `lpStrings`: array of `wNumStrings` insertion strings.
    /// `lpRawData`: raw data bytes, or NULL.
    @lib pub fn ReportEventA(
        hEventLog:   HEVENTLOG,
        wType:       u16,         // EventLogType.*
        wCategory:   u16,
        dwEventID:   u32,
        lpUserSid:   *void,
        wNumStrings: u16,
        dwDataSize:  u32,
        lpStrings:   **const char,
        lpRawData:   *void
    ): i32

    // ── Security / token ─────────────────────────────────────────

    /// Opens the access token of a process.
    /// `DesiredAccess`: 0x0008 = TOKEN_QUERY, 0x0020 = TOKEN_ADJUST_PRIVILEGES.
    @lib pub fn OpenProcessToken(
        ProcessHandle: *void,    // *HANDLE
        DesiredAccess: u32,
        TokenHandle:   **void    // **HANDLE
    ): i32

    /// Looks up the LUID of a privilege by name, e.g. "SeShutdownPrivilege".
    /// Fills the 8-byte LUID at `lpLuid`.
    @lib pub fn LookupPrivilegeValueA(
        lpSystemName: *const char,
        lpName:       *const char,
        lpLuid:       *void      // *LUID (2 × u32)
    ): i32

    /// Adjusts token privileges. `pNewState` points to a
    /// TOKEN_PRIVILEGES struct (see Win32 docs).
    @lib pub fn AdjustTokenPrivileges(
        TokenHandle:          *void,    // *HANDLE
        DisableAllPrivileges: i32,
        NewState:             *void,    // *TOKEN_PRIVILEGES
        BufferLength:         u32,
        PreviousState:        *void,
        ReturnLength:         *u32
    ): i32

    /// Looks up the account name corresponding to a SID.
    @lib pub fn LookupAccountSidA(
        lpSystemName:      *const char,
        Sid:               *void,
        Name:              *char,
        cchName:           *u32,
        ReferencedDomainName: *char,
        cchReferencedDomainName: *u32,
        peUse:             *u32
    ): i32

    // ── Encryption / hashing (CryptoAPI lightweight) ──────────────

    /// Acquires a CSP (Cryptographic Service Provider) handle.
    /// `dwProvType`: 1 = PROV_RSA_FULL. `dwFlags`: 0 for default.
    @lib pub fn CryptAcquireContextA(
        phProv:      *u64,       // *HCRYPTPROV
        szContainer: *const char,
        szProvider:  *const char,
        dwProvType:  u32,
        dwFlags:     u32
    ): i32

    @lib pub fn CryptReleaseContext(hProv: u64, dwFlags: u32): i32

    /// Creates a hash object. `Algid`: 0x8003 = CALG_MD5, 0x8004 = CALG_SHA1,
    /// 0x800C = CALG_SHA_256.
    @lib pub fn CryptCreateHash(
        hProv:     u64,
        Algid:     u32,
        hKey:      u64,    // 0 for non-keyed hashes
        dwFlags:   u32,
        phHash:    *u64    // *HCRYPTHASH
    ): i32

    @lib pub fn CryptHashData(
        hHash:     u64,
        pbData:    *const void,
        dwDataLen: u32,
        dwFlags:   u32
    ): i32

    /// Gets the hash value. `dwParam`: 2 = HP_HASHVAL, 4 = HP_HASHSIZE.
    @lib pub fn CryptGetHashParam(
        hHash:     u64,
        dwParam:   u32,
        pbData:    *void,
        pdwDataLen: *u32,
        dwFlags:   u32
    ): i32

    @lib pub fn CryptDestroyHash(hHash: u64): i32

    /// Generates random bytes into `pbBuffer`.
    @lib pub fn CryptGenRandom(
        hProv:    u64,
        dwLen:    u32,
        pbBuffer: *void
    ): i32
}
