/// winnt.h — Win32 base type system: the universal kernel object
/// HANDLE, access-mask / security primitives, and the small SDK-wide
/// structures (GUID / FILETIME / SYSTEMTIME / LARGE_INTEGER / LUID)
/// that every other Win32 header transitively includes.
///
/// In the Windows SDK this header is `#include`d (directly or via
/// `windows.h`) by every other Win32 header; in ilang we mirror it as
/// its own file so "where does HANDLE / GUID / FILETIME live?" has a
/// single answer.

// ─── File attributes (FILE_ATTRIBUTE_*) ──────────────────────────
@flags
pub enum FileAttribute: u32 {
    readonly          = 0x00000001
    hidden            = 0x00000002
    system            = 0x00000004
    directory         = 0x00000010
    archive           = 0x00000020
    device            = 0x00000040
    normal            = 0x00000080
    temporary         = 0x00000100
    sparseFile        = 0x00000200
    reparsePoint      = 0x00000400
    compressed        = 0x00000800
    offline           = 0x00001000
    notContentIndexed = 0x00002000
    encrypted         = 0x00004000
}

// ─── Page-protection flags (PAGE_*) ──────────────────────────────
@flags
pub enum PageProtection: u32 {
    noAccess         = 0x001
    readonly         = 0x002
    readWrite        = 0x004
    writeCopy        = 0x008
    execute          = 0x010
    executeRead      = 0x020
    executeReadWrite = 0x040
    executeWriteCopy = 0x080
    guard            = 0x100
    noCache          = 0x200
    writeCombine     = 0x400
}

// ─── Standard / generic access rights (winnt.h) ──────────────────
// Pass to CreateFile, OpenProcess, OpenThread, RegOpenKeyEx, etc.
@flags
pub enum GenericAccess: u32 {
    /// GENERIC_READ — read access (file APIs: includes share + sync).
    read    = 0x80000000
    /// GENERIC_WRITE — write access.
    write   = 0x40000000
    /// GENERIC_EXECUTE — execute permission.
    execute = 0x20000000
    /// GENERIC_ALL — full access.
    all     = 0x10000000
}

// ─── Standard rights (STANDARD_RIGHTS_*) ─────────────────────────
@flags
pub enum StandardRights: u32 {
    delete       = 0x00010000
    readControl  = 0x00020000
    writeDac     = 0x00040000
    writeOwner   = 0x00080000
    synchronize  = 0x00100000
    /// `STANDARD_RIGHTS_REQUIRED` = delete | readControl | writeDac | writeOwner.
    required     = 0x000F0000
    /// `STANDARD_RIGHTS_ALL` = required | synchronize.
    all          = 0x001F0000
}

// ─── Token access rights (TOKEN_*) ───────────────────────────────
// Combine when calling OpenProcessToken / OpenThreadToken.
@flags
pub enum TokenAccess: u32 {
    assignPrimary    = 0x0001
    duplicate        = 0x0002
    impersonate      = 0x0004
    query            = 0x0008
    querySource      = 0x0010
    adjustPrivileges = 0x0020
    adjustGroups     = 0x0040
    adjustDefault    = 0x0080
    adjustSessionId  = 0x0100
    /// TOKEN_ALL_ACCESS_P (Vista+, less session-id).
    allAccessP       = 0x000F00FF
    /// TOKEN_ALL_ACCESS (Vista+).
    allAccess        = 0x000F01FF
    /// Read-only combination.
    read             = 0x00020008
    /// Write combination.
    write            = 0x000200E0
    /// Execute combination.
    execute          = 0x00020000
}

@extern(C) {
    // ── Universal kernel-object handle ───────────────────────────

    /// Generic kernel-object handle. Returned by CreateFile,
    /// CreateEvent, OpenProcess, OpenThread, CreateMutex,
    /// CreateSemaphore, etc. Closed via `kernel32.CloseHandle`.
    ///
    /// Special values:
    ///   INVALID_HANDLE_VALUE = (HANDLE)-1   — file-API failure sentinel
    ///   NULL                                 — most other APIs' failure sentinel
    @handle pub struct HANDLE {}

    // ── 64-bit integer unions (LARGE_INTEGER / ULARGE_INTEGER) ───

    /// 8-byte signed integer exposed as a `{ LowPart, HighPart }` pair.
    /// In C this is a union with `QuadPart: LONGLONG`. Read/write the
    /// whole value as `i64` by reinterpret-casting the pointer.
    pub struct LARGE_INTEGER {
        LowPart:  u32
        HighPart: i32
    }

    /// 8-byte unsigned integer exposed as a `{ LowPart, HighPart }` pair.
    pub struct ULARGE_INTEGER {
        LowPart:  u32
        HighPart: u32
    }

    // ── Locally-Unique Identifier (LUID) ─────────────────────────

    /// 8-byte locally-unique value, used by `LookupPrivilegeValue`
    /// and the TOKEN_PRIVILEGES table.
    pub struct LUID {
        LowPart:  u32
        HighPart: i32
    }

    // ── Globally Unique Identifier (GUID / IID / CLSID) ──────────

    /// 16-byte GUID — also used as IID (interface id) and CLSID
    /// (class id) for COM. Initialise with literal values, or parse
    /// from a string with `ole32.CLSIDFromString`.
    ///
    /// The `Data4` array (8 bytes) is exposed as two u32 words for
    /// alignment; the low byte of `Data4lo` is the first byte when
    /// the GUID is rendered as a string ("xxxxxxxx-xxxx-xxxx-...").
    pub struct GUID {
        Data1:   u32
        Data2:   u16
        Data3:   u16
        Data4lo: u32
        Data4hi: u32
    }

    // ── Date / time ──────────────────────────────────────────────

    /// `SYSTEMTIME` — 16 bytes. All fields are u16.
    pub struct SYSTEMTIME {
        wYear:         u16
        wMonth:        u16   // 1–12
        wDayOfWeek:    u16   // 0=Sunday … 6=Saturday
        wDay:          u16   // 1–31
        wHour:         u16   // 0–23
        wMinute:       u16   // 0–59
        wSecond:       u16   // 0–59
        wMilliseconds: u16   // 0–999
    }

    /// `FILETIME` — 8 bytes. 100-nanosecond intervals since
    /// 1601-01-01 00:00:00 UTC. Convert via FileTimeToSystemTime /
    /// SystemTimeToFileTime (kernel32).
    pub struct FILETIME {
        dwLowDateTime:  u32
        dwHighDateTime: u32
    }
}
