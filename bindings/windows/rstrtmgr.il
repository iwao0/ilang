/// rstrtmgr.dll — Restart Manager.
///
/// Detects which processes are using a set of files (or modules)
/// and can ask them to restart so a deploy / patch can replace
/// those files. Installers built with the modern Windows Installer
/// use this internally; you can drive it directly to implement
/// "Application is using this file" UI before an in-place update.
///
/// Pattern:
///   let sessionKey: u16[64] = [0 as u16; 64]
///   let sessionId: u32 = 0
///   RmStartSession(&sessionId, 0, sessionKey)
///   RmRegisterResources(sessionId, 1, &filePtr, 0, NULL, 0, NULL)
///   RmGetList(sessionId, &nProcInfoNeeded, &nProcInfo,
///             procInfoBuf, &rebootReasons)
///   RmShutdown(sessionId, 0x10, NULL)   // RmForceShutdown |
///                                       // RmShutdownOnlyRegistered
///   RmRestart(sessionId, 0, NULL)
///   RmEndSession(sessionId)

@flags
pub enum RmShutdownType: u32 {
    forceShutdown          = 0x1
    shutdownOnlyRegistered = 0x10
}

pub enum RmAppType: i32 {
    unknown      = 0
    mainWindow   = 1
    otherWindow  = 2
    service      = 3
    explorer     = 4
    console      = 5
    critical     = 1000
}

@flags
pub enum RmRebootReason: u32 {
    none                = 0x0
    permissionDenied    = 0x1
    sessionMismatch     = 0x2
    criticalProcess     = 0x4
    criticalService     = 0x8
    detectedSelf        = 0x10
}

@extern(C, "rstrtmgr") {
    /// Unique-process identifier — TimeStamp + PID. Matches what
    /// `OpenProcess(...)` returns under the hood; used to scope
    /// notifications across reboots.
    pub struct RM_UNIQUE_PROCESS {
        dwProcessId:    u32
        ProcessStartTime: u8[8]      // FILETIME
    }

    /// Per-process info returned by `RmGetList`. `strAppName` is
    /// the localized friendly name; `strServiceShortName` is the
    /// service short name (empty when the process isn't a service).
    pub struct RM_PROCESS_INFO {
        Process:               RM_UNIQUE_PROCESS
        strAppName:            u16[256]      // CCH_RM_MAX_APP_NAME = 255
        strServiceShortName:   u16[64]       // CCH_RM_MAX_SVC_NAME = 63
        ApplicationType:       i32           // RmAppType
        AppStatus:             u32
        TSSessionId:           u32
        bRestartable:          i32
    }

    // ── Session lifecycle ───────────────────────────────────────

    /// `RmStartSession(&pSessionHandle, dwSessionFlags, strSessionKey)`.
    /// `strSessionKey` must point to a `WCHAR[CCH_RM_SESSION_KEY+1]`
    /// = 33 wchar buffer. Returns ERROR_SUCCESS (0) on success.
    @lib pub fn RmStartSession(
        pSessionHandle:   *u32,
        dwSessionFlags:   u32,        // reserved, pass 0
        strSessionKey:    *u16
    ): u32

    @lib pub fn RmEndSession(dwSessionHandle: u32): u32

    /// Reconnect to a pre-existing session by GUID key.
    @lib pub fn RmJoinSession(
        pSessionHandle:   *u32,
        strSessionKey:    *const u16
    ): u32

    // ── Resource registration ───────────────────────────────────

    /// Register files / processes / services with the session.
    /// `nFiles`: count in `rgsFileNames`. `nApplications`: count in
    /// `rgApplications` (use to tag specific process instances).
    /// `nServices`: count in `rgsServiceNames`.
    @lib pub fn RmRegisterResources(
        dwSessionHandle:   u32,
        nFiles:            u32,
        rgsFileNames:      **const u16,    // wchar_t**
        nApplications:     u32,
        rgApplications:    *const RM_UNIQUE_PROCESS,
        nServices:         u32,
        rgsServiceNames:   **const u16
    ): u32

    // ── Query ───────────────────────────────────────────────────

    /// `RmGetList(handle, &needed, &got, rgAffectedApps, &reasons)`.
    /// On first call pass `got = 0`, `rgAffectedApps = NULL` to
    /// learn the required count in `needed`; then reallocate.
    @lib pub fn RmGetList(
        dwSessionHandle:    u32,
        pnProcInfoNeeded:   *u32,
        pnProcInfo:         *u32,
        rgAffectedApps:     *RM_PROCESS_INFO,
        lpdwRebootReasons:  *u32
    ): u32

    // ── Shutdown / restart ───────────────────────────────────────

    /// Ask the registered processes to terminate. `lActionFlags`
    /// is `RmShutdownType` OR-ed. `fnStatus` is an optional
    /// progress callback (0..100).
    @lib pub fn RmShutdown(
        dwSessionHandle:   u32,
        lActionFlags:      u32,
        fnStatus:          fn(u32)
    ): u32

    /// Restart the apps that were shut down. `dwRestartFlags` is
    /// reserved; pass 0.
    @lib pub fn RmRestart(
        dwSessionHandle:   u32,
        dwRestartFlags:    u32,
        fnStatus:          fn(u32)
    ): u32

    /// Cancel a pending shutdown.
    @lib pub fn RmCancelCurrentTask(dwSessionHandle: u32): u32
}
