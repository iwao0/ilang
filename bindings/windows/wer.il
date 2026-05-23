/// wer.dll — Windows Error Reporting (WER).
///
/// Two practical scenarios:
///   1. Custom crash reports — produce a dump + send it via
///      WerReportSubmit.
///   2. Disable / enable WER for a process (services, sandboxes).

// ─── WER_REPORT_TYPE ─────────────────────────────────────────────
pub enum WerReportType: i32 {
    nonCritical          = 0
    critical             = 1
    applicationCrash     = 2
    applicationHang      = 3
    kernel               = 4
    invalid              = 5
}

// ─── WER_SUBMIT_FLAGS ────────────────────────────────────────────
@flags
pub enum WerSubmitFlags: u32 {
    honorRecovery     = 0x00000001
    honorRestart      = 0x00000002
    queue             = 0x00000004
    showDebug         = 0x00000008
    addRegisteredData = 0x00000010
    outOfProcess      = 0x00000020
    noClose           = 0x00000040
    startMinimized    = 0x00000080
    outOfProcessAsync = 0x00000100
    bypassDataThrottling = 0x00000200
    archiveParametersOnly = 0x00000400
    showDebugIfNoDefaultResponse = 0x00000800
}

// ─── WER_SUBMIT_RESULT ───────────────────────────────────────────
pub enum WerSubmitResult: i32 {
    reportQueued        = 1
    reportUploaded      = 2
    reportDebug         = 3
    reportFailed        = 4
    disabled            = 5
    reportCancelled     = 6
    disabledQueue       = 7
    reportAsync         = 8
    customAction        = 9
    throttled           = 10
    reportUploadedCab   = 11
    storageLocationNotFound = 12
    submittedElsewhere  = 13
}

// ─── WER_FAULT_REPORTING flags for WerSetFlags ───────────────────
@flags
pub enum WerFaultReporting: u32 {
    none              = 0
    noUi              = 0x0001
    sendSilentReport  = 0x0002
    queueUpload       = 0x0004
    disableSnapshot   = 0x0008
    criticalProcess   = 0x0010
    flushQueueOnExit  = 0x0020
    disableThreadSuspension = 0x0040
}

use winnt { HANDLE }
@extern(C, "wer") {
    /// `WerReportCreate(pwzEventType, repType, pReportInformation,
    ///                  &hReportHandle)`.
    /// `pwzEventType` is a short identifier ("MyApp.Crash"); the
    /// caller-side companion `WerReportSubmit` uploads it.
    @lib pub fn WerReportCreate(
        pwzEventType:        *const u16,
        repType:             i32,             // WerReportType
        pReportInformation:  *void,           // *WER_REPORT_INFORMATION
        phReportHandle:      *i64
    ): i32

    /// Attach a file or memory range to the report. `dwFileType`:
    ///   1 WerFileTypeMicrodump   2 WerFileTypeMinidump
    ///   3 WerFileTypeHeapdump    4 WerFileTypeUserDocument
    ///   5 WerFileTypeOther
    @lib pub fn WerReportAddFile(
        hReportHandle: i64,
        pwzPath:       *const u16,
        dwFileType:    i32,
        dwFileFlags:   u32
    ): i32

    /// Set a custom name/value pair on the report ("Build",
    /// "1.2.3").
    @lib pub fn WerReportSetParameter(
        hReportHandle: i64,
        dwParamID:     u32,       // P0..P9
        pwzName:       *const u16,
        pwzValue:      *const u16
    ): i32

    /// Submit the report. Returns a `WerSubmitResult` in
    /// `*pSubmitResult`.
    @lib pub fn WerReportSubmit(
        hReportHandle:  i64,
        consent:        i32,      // WER_CONSENT
        dwFlags:        u32,
        pSubmitResult:  *i32
    ): i32

    @lib pub fn WerReportCloseHandle(hReportHandle: i64): i32

    // ── Process-wide WER tuning ─────────────────────────────────

    /// Enable / disable WER for the calling process.
    /// 1 = enable, 0 = disable.
    @lib pub fn WerSetFlags(dwFlags: u32): i32
    @lib pub fn WerGetFlags(hProcess: HANDLE, pdwFlags: *u32): i32

    /// Add a file to be included in any crash dump for this process.
    @lib pub fn WerRegisterFile(
        pwzFile:    *const u16,
        regFileType: i32,
        dwFlags:    u32
    ): i32

    @lib pub fn WerUnregisterFile(pwzFilePath: *const u16): i32

    /// Add a free-form memory range — included verbatim in dumps.
    @lib pub fn WerRegisterMemoryBlock(pvAddress: *void, dwSize: u32): i32
    @lib pub fn WerUnregisterMemoryBlock(pvAddress: *void): i32

    /// Add a thread name. Helps post-mortem analysis.
    @lib pub fn WerRegisterCustomMetadata(
        key:    *const u16,
        value:  *const u16
    ): i32

    @lib pub fn WerUnregisterCustomMetadata(key: *const u16): i32
}
