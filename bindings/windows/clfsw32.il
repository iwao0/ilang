/// clfsw32.dll — Common Log File System (CLFS).
///
/// Kernel-managed log storage used by Distributed Transaction
/// Coordinator (DTC), file system journals, and SQL Server. Also
/// useful for application-level write-ahead logs that need
/// guarantee-of-durability and crash recovery without rolling your
/// own log file format.
///
/// Lifecycle:
///   let h = CreateLogFile(L"mylog.blf", GENERIC_WRITE | GENERIC_READ,
///                         FILE_SHARE_NONE, NULL, OPEN_ALWAYS,
///                         FILE_ATTRIBUTE_NORMAL)
///   AddLogContainer(h, &size, L"mylog.0001.blf", NULL)
///   InstallLogPolicy(h, &policy)
///   ... ReserveAndAppendLog / ReadLogRecord / ReadNextLogRecord ...
///   CloseHandle(h)
///
/// `CLFS_LSN` is an 8-byte log-sequence number.

@flags
pub enum ClfsLogOpenFlags: u32 {
    multiplexed     = 0x00000001
    dedicated       = 0x00000002
    asyncWrite      = 0x00000004
}

@flags
pub enum ClfsReserveAndAppendFlags: u32 {
    none                  = 0
    forceFlush            = 0x1
    forceAppend           = 0x2
    forceFlushAndUpdate   = 0x4
}

@flags
pub enum ClfsReadFlags: u32 {
    forward      = 0x1
    backward     = 0x2
    all          = 0x3
    forwardChain = 0x4
}

@extern(C, "clfsw32") {
    /// 8-byte LSN — sentinel `CLFS_LSN_NULL` = (0xFFFFFFFFFFFFFFFF).
    pub struct CLFS_LSN {
        Value: u64
    }

    /// `CreateLogFile(name, access, share, security, creation,
    ///                flags, template)`. Returns a CLFS file
    /// HANDLE (cast to i64).
    @lib pub fn CreateLogFile(
        pszLogFileName:        *const u16,
        fDesiredAccess:        u32,
        dwShareMode:           u32,
        psaLogFile:            *void,
        fCreateDisposition:    u32,
        dwFlagsAndAttributes:  u32
    ): i64

    /// Add a backing container file. `pcbContainer` is in/out:
    /// requested size on input, granted size on output.
    @lib pub fn AddLogContainer(
        hLog:           i64,
        pcbContainer:   *u64,
        pwszContainerPath: *const u16,
        pReserved:      *void
    ): i32

    @lib pub fn AddLogContainerSet(
        hLog:           i64,
        cContainer:     u16,
        pcbContainer:   *u64,
        rgwszContainerPath: **const u16,
        pReserved:      *void
    ): i32

    @lib pub fn DeleteLogFile(
        pszLogFileName: *const u16,
        pvReserved:     *void
    ): i32

    @lib pub fn DeleteLogByHandle(hLog: i64): i32

    // ── Read context lifecycle ──────────────────────────────────

    /// `CreateLogMarshallingArea` builds the in-memory writer ring.
    @lib pub fn CreateLogMarshallingArea(
        hLog:           i64,
        pfnAllocBuffer: *void,
        pfnFreeBuffer:  *void,
        pvBlockAllocContext: *void,
        cbMarshallingBuffer: u32,
        cMaxWriteBuffers: u32,
        cMaxReadBuffers:  u32,
        ppvMarshalContext: **void
    ): i32

    @lib pub fn DeleteLogMarshallingArea(pvMarshalContext: *void): i32

    /// Reserve + append in one shot. `rgWriteEntries` is an array
    /// of `CLFS_WRITE_ENTRY`. On success the new tail LSN is
    /// written to `*plsn`.
    @lib pub fn ReserveAndAppendLog(
        pvMarshal:        *void,
        rgWriteEntries:   *const void,         // *CLFS_WRITE_ENTRY[]
        cWriteEntries:    u32,
        plsnUndoNext:     *CLFS_LSN,
        plsnPrevious:     *CLFS_LSN,
        cReserveRecords:  u32,
        rgcbReservation:  *i64,
        fFlags:           u32,
        plsn:             *CLFS_LSN
    ): i32

    /// Flush + advance the base LSN.
    @lib pub fn FlushLogBuffers(
        pvMarshal:    *void,
        plsnFlush:    *CLFS_LSN
    ): i32

    @lib pub fn AdvanceLogBase(
        pvMarshal:        *void,
        plsnBase:         *CLFS_LSN,
        fFlags:           u32,
        pOverlapped:      *void
    ): i32

    // ── Read ────────────────────────────────────────────────────

    /// Open a read context anchored at `*plsnStart`.
    @lib pub fn ReadLogRecord(
        pvMarshal:        *void,
        plsnFirst:        *CLFS_LSN,
        eContextMode:     u32,
        ppvReadBuffer:    **void,
        pcbReadBuffer:    *u32,
        peRecordType:     *u8,
        plsnUndoNext:     *CLFS_LSN,
        plsnPrevious:     *CLFS_LSN,
        ppvReadContext:   **void
    ): i32

    @lib pub fn ReadNextLogRecord(
        pvReadContext:    *void,
        ppvBuffer:        **void,
        pcbBuffer:        *u32,
        peRecordType:     *u8,
        plsnUser:         *CLFS_LSN,
        plsnUndoNext:     *CLFS_LSN,
        plsnPrevious:     *CLFS_LSN,
        plsnRecord:       *CLFS_LSN
    ): i32

    @lib pub fn TerminateReadLog(pvReadContext: *void): i32

    // ── State queries ───────────────────────────────────────────

    /// Read CLFS-specific state info. `eInformationClass`:
    ///   1 ClfsLogBasicInformation   2 ClfsLogIoStatistics
    ///   4 ClfsLogPhysicalFormatInformation
    @lib pub fn GetLogFileInformation(
        hLog:           i64,
        pinfoBuffer:    *void,
        cbBuffer:       *u32
    ): i32

    @lib pub fn SetLogFileSizeWithPolicy(
        hLog:           i64,
        pDesiredSize:   *const u64,
        pResultingSize: *u64
    ): i32

    // ── Policy ──────────────────────────────────────────────────

    /// `InstallLogPolicy(hLog, pPolicy)` — set growth / archive
    /// behaviour.
    @lib pub fn InstallLogPolicy(
        hLog:    i64,
        pPolicy: *const void
    ): i32

    @lib pub fn QueryLogPolicy(
        hLog:           i64,
        ePolicyType:    u32,
        pPolicyBuffer:  *void,
        pcbPolicyBuffer: *u32
    ): i32

    @lib pub fn RemoveLogPolicy(
        hLog:           i64,
        ePolicyType:    u32
    ): i32
}
