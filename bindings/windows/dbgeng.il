/// dbgeng.dll — Debugger Engine (DbgEng): scriptable debugging,
/// crash-dump analysis, kernel + user-mode introspection.
///
/// Powers `cdb.exe` / `windbg.exe` and dotnet-dump's analyzers.
///
/// Bootstrap pattern:
///   let client: *void = 0 as *void
///   DebugCreate(&IID_IDebugClient, &client)
///   client.AttachProcess(0, GetCurrentProcessId(),
///                        DEBUG_ATTACH_NONINVASIVE)
///   client.QueryInterface(&IID_IDebugControl, &ctrl)
///   ctrl.WaitForEvent(0, INFINITE)
///   ctrl.Execute(DEBUG_OUTCTL_THIS_CLIENT, "kn", DEBUG_EXECUTE_DEFAULT)
///
/// COM methods on IDebugClient / IDebugControl / IDebugSymbols /
/// IDebugRegisters aren't bound at the ilang call level — walk
/// the vtables.

@flags
pub enum DebugAttach: u32 {
    kernel                = 0x00000000
    nonInvasive           = 0x00000001
    existing              = 0x00000002
    nonInvasiveNoSuspend  = 0x00000004
    invasiveNoInitialBreak = 0x00000008
    invasiveResumeProcess = 0x00000010
    nonInvasiveAllowPartial = 0x00000020
}

@flags
pub enum DebugExecuteFlags: u32 {
    default_      = 0
    echo          = 1
    noEcho        = 2
    notLogged     = 4
}

@flags
pub enum DebugOutctl: u32 {
    thisClient        = 0
    allClients        = 1
    allOtherClients   = 2
    ignore            = 3
    logOnly           = 4
    sendMask          = 7
    notLogged         = 0x10
    overrideMask      = 0x20
    dmlOutput         = 0x40
}

@extern(C, "dbgeng") {
    /// Create the root `IDebugClient` instance. `riid` is the
    /// interface IID (IID_IDebugClient = {27FE5639-8407-4F47-8364-EE118FB08AC8}).
    @lib pub fn DebugCreate(
        riid:    *const void,           // *GUID
        ppvObject: **void
    ): i32

    /// Same but creates a deeper-versioned client (Win10+):
    /// IID_IDebugClient5 / 6 / 7 / 8.
    @lib pub fn DebugCreateEx(
        riid:    *const void,
        flags:   u32,                   // DEBUG_CLIENT_EX_*
        ppvObject: **void
    ): i32

    /// `DebugConnect(remoteOptions, riid, &interface)` — connect
    /// to a remote debugger session. `remoteOptions` parses like
    /// `-server tcp:port=1234,server=foo`.
    @lib pub fn DebugConnect(
        RemoteOptions: *const char,
        InterfaceId:   *const void,
        Interface:     **void
    ): i32

    @lib pub fn DebugConnectWide(
        RemoteOptions: *const u16,
        InterfaceId:   *const void,
        Interface:     **void
    ): i32
}
