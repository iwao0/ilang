/// winsta.dll — Windows Station / Terminal Services API.
///
/// Practical use cases:
///   * Enumerate logged-in users / RDP sessions (`WinStationEnumerateW`)
///   * Disconnect / reset a session  (`WinStationDisconnect`)
///   * Send a message to a session    (`WinStationSendMessageW`)
///
/// Higher-level alternatives in wtsapi32 already cover most of
/// this — this file mirrors the lower-level surface for callers
/// that want the direct entry points (slightly faster path, no
/// extra marshalling).

pub enum WinstaState: i32 {
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

@extern(C, "winsta") {
    /// `SESSIONIDW` info per session.
    pub struct SESSIONIDW {
        SessionId:  u32
        WinStationName: u16[32]
        State:      i32        // WinstaState
    }

    /// Open a handle to the named server. NULL = local. Free with
    /// `WinStationCloseServer`.
    @lib pub fn WinStationOpenServerW(pServerName: *const u16): i64

    @lib pub fn WinStationCloseServer(hServer: i64): i32

    /// `WinStationEnumerateW(hServer, &pSessionIds, &count)` — count
    /// is in/out.
    @lib pub fn WinStationEnumerateW(
        hServer:        i64,
        ppSessionInfo:  **SESSIONIDW,
        pCount:         *u32
    ): i32

    @lib pub fn WinStationFreeMemory(pMemory: *void): i32

    /// Disconnect the named session. `bWait`: TRUE = block until
    /// disconnect actually happens.
    @lib pub fn WinStationDisconnect(
        hServer:    i64,
        LogonId:    u32,
        bWait:      i32
    ): i32

    @lib pub fn WinStationReset(
        hServer:    i64,
        LogonId:    u32,
        bWait:      i32
    ): i32

    /// Post a message box into the named session. Returns when the
    /// user dismisses it, or after `Timeout` seconds (0 = forever).
    @lib pub fn WinStationSendMessageW(
        hServer:        i64,
        SessionId:      u32,
        pTitle:         *const u16,
        TitleLength:    u32,
        pMessage:       *const u16,
        MessageLength:  u32,
        Style:          u32,
        Timeout:        u32,
        pResponse:      *u32,
        DoNotWait:      i32
    ): i32

    /// Query a session for a specific information class.
    /// `WinStationInformationClass`:
    ///   1 WinStationInformation
    ///   8 WinStationClientHardwareId
    ///   10 WinStationClient
    ///   24 WinStationCfgInfo
    @lib pub fn WinStationQueryInformationW(
        hServer:        i64,
        LogonId:        u32,
        WinStationInformationClass: i32,
        pWinStationInformation: *void,
        WinStationInformationLength: u32,
        pReturnLength:  *u32
    ): i32

    @lib pub fn WinStationGetTokenFromSessionId(
        hServer:        i64,
        LogonId:        u32,
        ppToken:        *i64
    ): i32

    /// `WinStationConnectW(serverHandle, sessionId, targetSessionId,
    ///                     passwd, wait)` — moves a session from
    /// disconnected to active.
    @lib pub fn WinStationConnectW(
        hServer:            i64,
        SessionId:          u32,
        TargetSessionId:    u32,
        pPassword:          *const u16,
        bWait:              i32
    ): i32

    /// Shadow another session (admin only).
    @lib pub fn WinStationShadow(
        hServer:        i64,
        pTargetServer:  *const u16,
        TargetSessionId: u32,
        HotkeyVk:       u8,
        HotkeyModifiers: u16
    ): i32
}
