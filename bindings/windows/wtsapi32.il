/// wtsapi32.dll — Terminal Services / Remote Desktop / Fast User
/// Switching APIs. Useful for session-aware services, screen lock
/// detection (WTSRegisterSessionNotification), and discovering the
/// active console session.

// ─── WTS_CONNECTSTATE_CLASS — session state codes ────────────────
pub enum WtsConnectState: u32 {
    active        = 0
    connected     = 1
    connectQuery  = 2
    shadow        = 3
    disconnected  = 4
    idle          = 5
    listen        = 6
    reset         = 7
    down          = 8
    init          = 9
}

// ─── WTSQuerySessionInformation info-class values ────────────────
pub enum WtsInfoClass: u32 {
    initialProgram       = 0
    applicationName      = 1
    workingDirectory     = 2
    oemId                = 3
    sessionId            = 4
    userName             = 5
    winStationName       = 6
    domainName           = 7
    connectState         = 8
    clientBuildNumber    = 9
    clientName           = 10
    clientDirectory      = 11
    clientProductId      = 12
    clientHardwareId     = 13
    clientAddress        = 14
    clientDisplay        = 15
    clientProtocolType   = 16
    idleTime             = 17
    logonTime            = 18
    incomingBytes        = 19
    outgoingBytes        = 20
    incomingFrames       = 21
    outgoingFrames       = 22
    clientInfo           = 23
    sessionInfo          = 24
    sessionInfoEx        = 25
    configInfo           = 26
    validationInfo       = 27
    sessionAddressV4     = 28
    isRemoteSession      = 29
}

// ─── WTSRegisterSessionNotification dwFlags ──────────────────────
@flags
pub enum SessionNotifyFlags: u32 {
    /// Notifications for sessions on this computer only.
    thisSession  = 0
    /// Notifications for all sessions on the system.
    allSessions  = 1
}

// ─── WTS_CURRENT_SESSION / WTS_CURRENT_SERVER constants ──────────
// Use as integer parameters:
//   WTS_CURRENT_SESSION = 0xFFFFFFFF  → current session
//   WTS_CURRENT_SERVER  = 0 as *void  → local server

use user32 { HWND }

@extern(C, "wtsapi32") {
    // ── WTS_SESSION_INFO — 32 bytes on x64 ───────────────────────

    pub struct WTS_SESSION_INFO {
        SessionId:       u32
        _pad:            u32
        pWinStationName: *char
        State:           u32       // WtsConnectState.*
        _pad2:           u32
    }

    // ── Session lookup / enumeration ─────────────────────────────

    /// Returns the session ID of the user currently logged into the
    /// physical console. Returns 0xFFFFFFFF when nobody is logged in.
    @lib pub fn WTSGetActiveConsoleSessionId(): u32

    /// Enumerates sessions on `hServer` (NULL = local).  Free the
    /// returned array with `WTSFreeMemory`.
    /// `Reserved`: must be 0. `Version`: 1.
    @lib pub fn WTSEnumerateSessionsA(
        hServer:        *void,
        Reserved:       u32,
        Version:        u32,
        ppSessionInfo:  **WTS_SESSION_INFO,
        pCount:         *u32
    ): i32
    @lib pub fn WTSEnumerateSessionsW(
        hServer:        *void,
        Reserved:       u32,
        Version:        u32,
        ppSessionInfo:  **WTS_SESSION_INFO,
        pCount:         *u32
    ): i32

    /// Frees memory returned by any WTS* enumeration function.
    @lib pub fn WTSFreeMemory(pMemory: *void)

    // ── Per-session information ───────────────────────────────────

    /// Retrieves a specific piece of session information.
    /// `ppBuffer`: receives a heap-allocated buffer (free with WTSFreeMemory).
    /// `pBytesReturned`: receives the byte length of the data.
    @lib pub fn WTSQuerySessionInformationA(
        hServer:        *void,
        SessionId:      u32,            // or WTS_CURRENT_SESSION = -1 as u32
        WTSInfoClass:   u32,            // WtsInfoClass.*
        ppBuffer:       **char,
        pBytesReturned: *u32
    ): i32
    @lib pub fn WTSQuerySessionInformationW(
        hServer:        *void,
        SessionId:      u32,            // or WTS_CURRENT_SESSION = -1 as u32
        WTSInfoClass:   u32,            // WtsInfoClass.*
        ppBuffer:       **u16,
        pBytesReturned: *u32
    ): i32

    @lib pub fn WTSQueryUserToken(
        SessionId: u32,
        phToken:   **void          // **HANDLE
    ): i32

    // ── Session notifications ─────────────────────────────────────

    /// Registers `hwnd` to receive WM_WTSSESSION_CHANGE messages.
    /// `dwFlags`: SessionNotifyFlags.*.
    /// WM_WTSSESSION_CHANGE wParam values:
    ///   0x1=console connect, 0x2=disconnect, 0x5=remote connect,
    ///   0x6=remote disconnect, 0x7=session logon, 0x8=session logoff,
    ///   0x9=session lock, 0xA=session unlock, 0xB=session remote control.
    @lib pub fn WTSRegisterSessionNotification(
        hwnd:    HWND,
        dwFlags: u32
    ): i32

    @lib pub fn WTSUnRegisterSessionNotification(hwnd: HWND): i32

    // ── Session control ──────────────────────────────────────────

    /// Logs off a session. `bWait`: 1 = wait for completion.
    @lib pub fn WTSLogoffSession(
        hServer:   *void,
        SessionId: u32,
        bWait:     i32
    ): i32

    /// Disconnects a session (keeps it running).
    @lib pub fn WTSDisconnectSession(
        hServer:   *void,
        SessionId: u32,
        bWait:     i32
    ): i32

    /// Sends a message to a session via a dialog box on its desktop.
    /// `Style`: MB_* MessageBox flags.
    @lib pub fn WTSSendMessageA(
        hServer:        *void,
        SessionId:      u32,
        pTitle:         *const char,
        TitleLength:    u32,
        pMessage:       *const char,
        MessageLength:  u32,
        Style:          u32,
        Timeout:        u32,
        pResponse:      *u32,
        bWait:          i32
    ): i32
    @lib pub fn WTSSendMessageW(
        hServer:        *void,
        SessionId:      u32,
        pTitle:         *const u16,
        TitleLength:    u32,
        pMessage:       *const u16,
        MessageLength:  u32,
        Style:          u32,
        Timeout:        u32,
        pResponse:      *u32,
        bWait:          i32
    ): i32

    // ── Server enumeration ────────────────────────────────────────

    /// Connects to a remote terminal server.  Pass NULL for local.
    @lib pub fn WTSOpenServerA(pServerName: *const char): *void
    @lib pub fn WTSOpenServerW(pServerName: *const u16): *void

    @lib pub fn WTSCloseServer(hServer: *void)
}
