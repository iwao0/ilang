/// kernel32.dll — core OS services: processes, threads, files,
/// memory, synchronisation, timing, console, and DLL loading.
///
/// Notes:
///   INVALID_HANDLE_VALUE  = `0xFFFFFFFFFFFFFFFF as HANDLE`
///   INFINITE (no timeout) = `0xFFFFFFFF` as u32 for WaitForSingleObject

// ─── Generic-access rights (GENERIC_*) ───────────────────────
@flags
pub enum FileAccess: u32 {
    execute = 0x20000000
    write   = 0x40000000
    read    = 0x80000000
    all     = 0x10000000
}

// ─── File-sharing mode (FILE_SHARE_*) ────────────────────────
@flags
pub enum FileShare: u32 {
    read   = 0x00000001
    write  = 0x00000002
    delete = 0x00000004
}

// ─── Creation disposition for CreateFileA ────────────────────
pub enum FileCreation: u32 {
    createNew        = 1   // fail if file already exists
    createAlways     = 2   // truncate if file already exists
    openExisting     = 3   // fail if file does not exist
    openAlways       = 4   // create if file does not exist
    truncateExisting = 5   // truncate to zero; requires write access
}

// ─── WaitForSingleObject return values ───────────────────────
pub enum WaitResult: u32 {
    object0   = 0x00000000   // object became signalled
    abandoned = 0x00000080   // mutex: owning thread terminated
    timeout   = 0x00000102
    failed    = 0xFFFFFFFF   // call GetLastError for details
}

// ─── VirtualAlloc / VirtualFree type flags (MEM_*) ───────────
@flags
pub enum AllocationType: u32 {
    commit     = 0x00001000
    reserve    = 0x00002000
    decommit   = 0x00004000   // VirtualFree only
    release    = 0x00008000   // VirtualFree only; dwSize must be 0
    reset      = 0x00080000
    topDown    = 0x00100000
    writeWatch = 0x00200000
    physical   = 0x00400000
    resetUndo  = 0x01000000
    largePages = 0x20000000
}

// ─── Standard handle IDs for GetStdHandle (STD_*) ────────────
pub enum StdHandle: u32 {
    input  = 0xFFFFFFF6   // STD_INPUT_HANDLE
    output = 0xFFFFFFF5   // STD_OUTPUT_HANDLE
    error  = 0xFFFFFFF4   // STD_ERROR_HANDLE
}

// ─── Process access rights (PROCESS_*) ───────────────────────
@flags
pub enum ProcessAccess: u32 {
    terminate                = 0x0001
    createThread             = 0x0002
    vmOperation              = 0x0008
    vmRead                   = 0x0010
    vmWrite                  = 0x0020
    dupHandle                = 0x0040
    createProcess            = 0x0080
    setQuota                 = 0x0100
    setInformation           = 0x0200
    queryInformation         = 0x0400
    suspendResume            = 0x0800
    queryLimitedInformation  = 0x1000
    setLimitedInformation    = 0x2000
    synchronize              = 0x00100000
    /// PROCESS_ALL_ACCESS on Vista+.
    allAccess                = 0x001FFFFF
}

// ─── Thread access rights (THREAD_*) ─────────────────────────
@flags
pub enum ThreadAccess: u32 {
    terminate                = 0x0001
    suspendResume            = 0x0002
    getContext               = 0x0008
    setContext               = 0x0010
    setInformation           = 0x0020
    queryInformation         = 0x0040
    setThreadToken           = 0x0080
    impersonate              = 0x0100
    directImpersonation      = 0x0200
    setLimitedInformation    = 0x0400
    queryLimitedInformation  = 0x0800
    synchronize              = 0x00100000
    allAccess                = 0x001FFFFF
}

// ─── Process priority class ──────────────────────────────────
pub enum PriorityClass: u32 {
    idle         = 0x00000040
    belowNormal  = 0x00004000
    normal       = 0x00000020
    aboveNormal  = 0x00008000
    high         = 0x00000080
    realtime     = 0x00000100
}

// ─── Thread priority (THREAD_PRIORITY_*) ─────────────────────
pub enum ThreadPriority: i32 {
    idle          = -15
    lowest        = -2
    belowNormal   = -1
    normal        = 0
    aboveNormal   = 1
    highest       = 2
    timeCritical  = 15
}

// ─── Console input modes (ENABLE_*) ──────────────────────────
@flags
pub enum ConsoleInputMode: u32 {
    processedInput       = 0x0001
    lineInput            = 0x0002
    echoInput            = 0x0004
    windowInput          = 0x0008
    mouseInput           = 0x0010
    insertMode           = 0x0020
    quickEditMode        = 0x0040
    extendedFlags        = 0x0080
    autoPosition         = 0x0100
    virtualTerminalInput = 0x0200
}

// ─── Console output modes ────────────────────────────────────
@flags
pub enum ConsoleOutputMode: u32 {
    processedOutput           = 0x0001
    wrapAtEolOutput           = 0x0002
    /// Required to interpret ANSI escape sequences (Win10 1607+).
    virtualTerminalProcessing = 0x0004
    disableNewlineAutoReturn  = 0x0008
    lvbGridWorldwide          = 0x0010
}

// ─── Heap flags (HEAP_*) ─────────────────────────────────────
@flags
pub enum HeapFlags: u32 {
    noSerialize         = 0x00000001
    growable            = 0x00000002   // HeapCreate only
    generateExceptions  = 0x00000004
    zeroMemory          = 0x00000008
    reallocInPlaceOnly  = 0x00000010
    tailCheckingEnabled = 0x00000020
    freeCheckingEnabled = 0x00000040
    disableCoalesceOnFree = 0x00000080
    createEnableExecute = 0x00040000
}

// ─── Code pages (CP_*) ───────────────────────────────────────
pub enum CodePage: u32 {
    ansi       = 0       // CP_ACP — system ANSI
    oem        = 1       // CP_OEMCP
    macCP      = 2
    threadAnsi = 3       // CP_THREAD_ACP
    symbol     = 42
    utf7       = 65000
    utf8       = 65001
}

// ─── GetFileType return values (FILE_TYPE_*) ─────────────────
pub enum FileType: u32 {
    unknown = 0x0000
    disk    = 0x0001
    char    = 0x0002   // console, LPT, etc.
    pipe    = 0x0003
    remote  = 0x8000   // deprecated
}

// ─── GetDriveType return values (DRIVE_*) ────────────────────
pub enum DriveType: u32 {
    unknown   = 0
    noRootDir = 1
    removable = 2
    fixed     = 3
    remote    = 4
    cdrom     = 5
    ramdisk   = 6
}

// ─── Toolhelp32 snapshot flags (TH32CS_*) ────────────────────
@flags
pub enum Toolhelp32Flags: u32 {
    snapHeapList = 0x00000001
    snapProcess  = 0x00000002
    snapThread   = 0x00000004
    snapModule   = 0x00000008
    snapModule32 = 0x00000010
    snapAll      = 0x0000001F
    inherit      = 0x80000000
}

// ─── CreateProcess flags (CREATE_*) ──────────────────────────
@flags
pub enum CreationFlags: u32 {
    debugProcess               = 0x00000001
    debugOnlyThisProcess       = 0x00000002
    createSuspended            = 0x00000004
    detachedProcess            = 0x00000008
    createNewConsole           = 0x00000010
    createNewProcessGroup      = 0x00000200
    createUnicodeEnvironment   = 0x00000400
    createSeparateWowVdm       = 0x00000800
    createSharedWowVdm         = 0x00001000
    inheritParentAffinity      = 0x00010000
    createProtectedProcess     = 0x00040000
    extendedStartupinfoPresent = 0x00080000
    createBreakawayFromJob     = 0x01000000
    createPreserveCodeAuthZLevel = 0x02000000
    createDefaultErrorMode     = 0x04000000
    createNoWindow             = 0x08000000
}

// ─── STARTUPINFO flags (STARTF_*) ────────────────────────────
@flags
pub enum StartupInfoFlags: u32 {
    useShowWindow    = 0x00000001
    useSize          = 0x00000002
    usePosition      = 0x00000004
    useCountChars    = 0x00000008
    useFillAttribute = 0x00000010
    runFullscreen    = 0x00000020
    forceOnFeedback  = 0x00000040
    forceOffFeedback = 0x00000080
    useStdHandles    = 0x00000100
    useHotkey        = 0x00000200
    titleIsLinkName  = 0x00000800
    titleIsAppId     = 0x00001000
    preventPinning   = 0x00002000
    untrustedSource  = 0x00008000
}

// ─── FormatMessage flags (FORMAT_MESSAGE_*) ──────────────────
@flags
pub enum FormatMessageFlags: u32 {
    allocateBuffer = 0x00000100
    ignoreInserts  = 0x00000200
    fromString     = 0x00000400
    fromHmodule    = 0x00000800
    fromSystem     = 0x00001000
    argumentArray  = 0x00002000
}

// ─── LoadLibraryEx flags (LOAD_LIBRARY_*) ────────────────────
@flags
pub enum LoadLibraryFlags: u32 {
    asDatafile           = 0x00000002
    dontResolveDllRefs   = 0x00000008
    asImageResource      = 0x00000020
    asDatafileExclusive  = 0x00000040
    requireSignedTarget  = 0x00000080
    searchDefaultDirs    = 0x00001000   // SECURE — recommended
    searchApplicationDir = 0x00000200
    searchSystem32       = 0x00000800
    searchUserDirs       = 0x00000400
    searchDllLoadDir     = 0x00000100
    safeCurrentDirs      = 0x00002000
}

// ─── DuplicateHandle options (DUPLICATE_*) ───────────────────
@flags
pub enum DuplicateHandleOptions: u32 {
    closeSource = 0x00000001
    sameAccess  = 0x00000002
}

// ─── Handle flags (HANDLE_FLAG_*) ────────────────────────────
@flags
pub enum HandleFlags: u32 {
    inherit          = 0x00000001
    protectFromClose = 0x00000002
}

// ─── Memory state / type (VirtualQuery output) ───────────────
@flags
pub enum MemoryState: u32 {
    commit  = 0x00001000
    reserve = 0x00002000
    free    = 0x00010000
}

pub enum MemoryType: u32 {
    private = 0x00020000
    mapped  = 0x00040000
    image   = 0x01000000
}

// ─── File flag / attribute composites for CreateFile ─────────
@flags
pub enum FileFlag: u32 {
    writeThrough        = 0x80000000
    overlapped          = 0x40000000
    noBuffering         = 0x20000000
    randomAccess        = 0x10000000
    sequentialScan      = 0x08000000
    deleteOnClose       = 0x04000000
    backupSemantics     = 0x02000000   // open directories with CreateFile
    posixSemantics      = 0x01000000
    sessionAware        = 0x00800000
    openReparsePoint    = 0x00200000
    openNoRecall        = 0x00100000
    firstPipeInstance   = 0x00080000
}

// ─── Named-pipe modes ────────────────────────────────────────
@flags
pub enum PipeOpenMode: u32 {
    accessInbound    = 0x00000001
    accessOutbound   = 0x00000002
    accessDuplex     = 0x00000003
    writeThrough     = 0x80000000
    overlapped       = 0x40000000
    firstPipeInstance = 0x00080000
}

@flags
pub enum PipeMode: u32 {
    typeByte        = 0x00000000
    typeMessage     = 0x00000004
    readModeByte    = 0x00000000
    readModeMessage = 0x00000002
    wait            = 0x00000000
    nowait          = 0x00000001
    acceptRemoteClients = 0x00000000
    rejectRemoteClients = 0x00000008
}

// ─── Threadpool callback priority (Vista+) ───────────────────
pub enum ThreadpoolCallbackPriority: u32 {
    high   = 0
    normal = 1
    low    = 2
}

// ─── Waitable timer flags ────────────────────────────────────
@flags
pub enum CreateWaitableTimerFlags: u32 {
    manualReset    = 0x00000001
    /// High-resolution timer (Win10 1803+).
    highResolution = 0x00000002
}

// ─── SetThreadExecutionState flags (ES_*) ────────────────────
@flags
pub enum ExecutionState: u32 {
    continuous       = 0x80000000
    displayRequired  = 0x00000002
    systemRequired   = 0x00000001
    awayModeRequired = 0x00000040
}

// ─── Console control event codes (CTRL_*_EVENT) ──────────────
pub enum ConsoleCtrlEvent: u32 {
    cEvent        = 0
    breakEvent    = 1
    closeEvent    = 2
    logoffEvent   = 5
    shutdownEvent = 6
}

// ─── INPUT_RECORD event types ────────────────────────────────
pub enum InputEventType: u16 {
    keyEvent              = 0x0001
    mouseEvent            = 0x0002
    windowBufferSizeEvent = 0x0004
    menuEvent             = 0x0008
    focusEvent            = 0x0010
}

// ─── PROCESSOR_FEATURE_* (IsProcessorFeaturePresent) ─────────
pub enum ProcessorFeature: u32 {
    floatingPointPrecisionErrata = 0
    floatingPointEmulated        = 1
    compareExchangeDouble        = 2
    mmxInstructionsAvailable     = 3
    sseInstructionsAvailable     = 6
    threeDnowInstructionsAvailable = 7
    rdtscInstructionAvailable    = 8
    paeEnabled                   = 9
    sse2InstructionsAvailable    = 10
    nxEnabled                    = 12
    sse3InstructionsAvailable    = 13
    compareExchange128           = 14
    compare64Exchange128         = 15
    channelsEnabled              = 16
    xsaveEnabled                 = 17
    secondLevelAddressTranslation = 20
    virtEnabled                  = 21
    rdwrfsgsbase                 = 22
    fastreposmovsb               = 23
    armV8InstructionsAvailable   = 29
    armV8CryptoInstructionsAvailable = 30
}

// ─── Symlink creation flags (SYMBOLIC_LINK_FLAG_*) ───────────
@flags
pub enum SymbolicLinkFlags: u32 {
    file                    = 0x00000000
    directory               = 0x00000001
    allowUnprivilegedCreate = 0x00000002   // Win10 1703+, Developer Mode
}

// ─── FindFirstFileEx info levels / search ops ────────────────
pub enum FindexInfoLevel: i32 {
    standard     = 0
    basic        = 1   // skip cAlternateFileName (faster)
    maxInfoLevel = 2
}

pub enum FindexSearchOp: i32 {
    nameMatch          = 0
    limitToDirectories = 1
    limitToDevices     = 2
}

use winnt { HANDLE, GUID, SYSTEMTIME, FILETIME, FileAttribute, PageProtection }
use windef { HMODULE, HWND, HRSRC }

@extern(C, "kernel32") {
    // ── Module ────────────────────────────────────────────────
    /// Returns the HMODULE of the running EXE when
    /// `lpModuleName` is `0 as *const char` (NULL).
    @lib pub fn GetModuleHandleA(lpModuleName: *const char): HMODULE
    @lib pub fn GetModuleHandleW(lpModuleName: *const u16): HMODULE

    // ── Error ─────────────────────────────────────────────────
    /// Returns the last Win32 error set on this thread.
    @lib pub fn GetLastError(): u32
    @lib pub fn SetLastError(dwErrCode: u32)

    // ── Handle ────────────────────────────────────────────────
    /// Closes a kernel object handle. Returns non-zero on success.
    @lib pub fn CloseHandle(hObject: HANDLE): i32

    // ── Process / thread ──────────────────────────────────────
    /// Terminates the calling process with `uExitCode`.
    @lib pub fn ExitProcess(uExitCode: u32)
    @lib pub fn GetCurrentProcessId(): u32
    @lib pub fn GetCurrentThreadId(): u32

    /// Spawns a new thread starting at `lpStartAddress(lpParameter)`.
    /// Pass 0 for `dwStackSize` to use the default stack size.
    /// Returns NULL on failure; the caller must CloseHandle the result.
    @lib pub fn CreateThread(
        lpThreadAttributes: *void,
        dwStackSize:        u64,
        lpStartAddress:     fn(i64): u32,
        lpParameter:        i64,
        dwCreationFlags:    u32,
        lpThreadId:         *u32
    ): HANDLE

    @lib pub fn ExitThread(dwExitCode: u32)

    /// Blocks until `hHandle` is signalled or the timeout expires.
    /// Pass `0xFFFFFFFF` (INFINITE) to wait forever.
    @lib pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: u32): u32

    // ── Synchronisation objects ───────────────────────────────
    /// Creates a mutex. Pass `0 as *const char` for an anonymous mutex.
    @lib pub fn CreateMutexA(
        lpMutexAttributes: *void,
        bInitialOwner:     i32,
        lpName:            *const char
    ): HANDLE
    @lib pub fn CreateMutexW(
        lpMutexAttributes: *void,
        bInitialOwner:     i32,
        lpName:            *const u16
    ): HANDLE
    @lib pub fn ReleaseMutex(hMutex: HANDLE): i32

    /// Creates a manual- or auto-reset event.
    @lib pub fn CreateEventA(
        lpEventAttributes: *void,
        bManualReset:      i32,
        bInitialState:     i32,
        lpName:            *const char
    ): HANDLE
    @lib pub fn CreateEventW(
        lpEventAttributes: *void,
        bManualReset:      i32,
        bInitialState:     i32,
        lpName:            *const u16
    ): HANDLE
    @lib pub fn SetEvent(hEvent: HANDLE): i32
    @lib pub fn ResetEvent(hEvent: HANDLE): i32

    // ── Time ──────────────────────────────────────────────────
    /// Suspends the calling thread for `dwMilliseconds` ms.
    @lib pub fn Sleep(dwMilliseconds: u32)
    /// Milliseconds since system boot; wraps around ~49.7 days.
    @lib pub fn GetTickCount(): u32
    /// 64-bit tick count; does not wrap.
    @lib pub fn GetTickCount64(): u64
    /// High-resolution performance counter value.
    @lib pub fn QueryPerformanceCounter(lpPerformanceCount: *i64): i32
    /// Frequency (ticks per second) of the performance counter.
    @lib pub fn QueryPerformanceFrequency(lpFrequency: *i64): i32

    // ── Debug / console ───────────────────────────────────────
    /// Sends a string to an attached debugger (e.g. DebugView).
    @lib pub fn OutputDebugStringA(lpOutputString: *const char)
    @lib pub fn OutputDebugStringW(lpOutputString: *const u16)

    /// Returns the standard input/output/error handle.
    /// Pass `StdHandle.output as u32` etc.
    @lib pub fn GetStdHandle(nStdHandle: u32): HANDLE

    @lib pub fn WriteConsoleA(
        hConsoleOutput:         HANDLE,
        lpBuffer:               *const char,
        nNumberOfCharsToWrite:  u32,
        lpNumberOfCharsWritten: *u32,
        lpReserved:             *void
    ): i32
    @lib pub fn WriteConsoleW(
        hConsoleOutput:         HANDLE,
        lpBuffer:               *const u16,
        nNumberOfCharsToWrite:  u32,
        lpNumberOfCharsWritten: *u32,
        lpReserved:             *void
    ): i32

    /// Allocates a new console window for the current process.
    /// Useful for GUI apps that want debug console output.
    @lib pub fn AllocConsole(): i32
    @lib pub fn FreeConsole(): i32
    @lib pub fn SetConsoleTitleA(lpConsoleTitle: *const char): i32
    @lib pub fn SetConsoleTitleW(lpConsoleTitle: *const u16): i32

    // ── DLL loading ───────────────────────────────────────────
    @lib pub fn LoadLibraryA(lpLibFileName: *const char): HMODULE
    @lib pub fn LoadLibraryW(lpLibFileName: *const u16): HMODULE
    @lib pub fn FreeLibrary(hLibModule: HMODULE): i32
    /// Returns NULL if the export is not found.
    @lib pub fn GetProcAddress(
        hModule:    HMODULE,
        lpProcName: *const char
    ): *void

    // ── Memory ────────────────────────────────────────────────
    /// Returns the default heap for the current process.
    @lib pub fn GetProcessHeap(): HANDLE
    /// `dwFlags`: 0 = default, 8 = HEAP_ZERO_MEMORY.
    @lib pub fn HeapAlloc(hHeap: HANDLE, dwFlags: u32, dwBytes: u64): *void
    @lib pub fn HeapFree(hHeap: HANDLE, dwFlags: u32, lpMem: *void): i32
    @lib pub fn HeapReAlloc(
        hHeap:   HANDLE,
        dwFlags: u32,
        lpMem:   *void,
        dwBytes: u64
    ): *void
    @lib pub fn HeapSize(hHeap: HANDLE, dwFlags: u32, lpMem: *const void): u64

    /// Reserves and/or commits pages. Pass `0 as *void` for
    /// `lpAddress` to let the OS choose the base address.
    @lib pub fn VirtualAlloc(
        lpAddress:        *void,
        dwSize:           u64,
        flAllocationType: u32,
        flProtect:        u32
    ): *void

    /// Releases or decommits pages. When `dwFreeType` is
    /// `AllocationType.release as u32`, `dwSize` must be 0.
    @lib pub fn VirtualFree(
        lpAddress:  *void,
        dwSize:     u64,
        dwFreeType: u32
    ): i32

    // ── File I/O ──────────────────────────────────────────────
    /// Opens or creates a file. Returns `INVALID_HANDLE_VALUE`
    /// (`0xFFFFFFFFFFFFFFFF as HANDLE`) on failure.
    /// `lpOverlapped`: pass `0 as *void` for synchronous I/O.
    @lib pub fn CreateFileA(
        lpFileName:            *const char,
        dwDesiredAccess:       u32,
        dwShareMode:           u32,
        lpSecurityAttributes:  *void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes:  u32,
        hTemplateFile:         HANDLE
    ): HANDLE
    @lib pub fn CreateFileW(
        lpFileName:            *const u16,
        dwDesiredAccess:       u32,
        dwShareMode:           u32,
        lpSecurityAttributes:  *void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes:  u32,
        hTemplateFile:         HANDLE
    ): HANDLE

    @lib pub fn ReadFile(
        hFile:                HANDLE,
        lpBuffer:             *void,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead:  *u32,
        lpOverlapped:         *void
    ): i32

    @lib pub fn WriteFile(
        hFile:                  HANDLE,
        lpBuffer:               *const void,
        nNumberOfBytesToWrite:  u32,
        lpNumberOfBytesWritten: *u32,
        lpOverlapped:           *void
    ): i32

    /// Writes the file size as a 64-bit signed integer to `lpFileSize`.
    @lib pub fn GetFileSizeEx(hFile: HANDLE, lpFileSize: *i64): i32
    @lib pub fn SetFilePointerEx(
        hFile:            HANDLE,
        liDistanceToMove: i64,
        lpNewFilePointer: *i64,
        dwMoveMethod:     u32
    ): i32
    @lib pub fn FlushFileBuffers(hFile: HANDLE): i32
    @lib pub fn DeleteFileA(lpFileName: *const char): i32
    @lib pub fn DeleteFileW(lpFileName: *const u16): i32
    @lib pub fn CopyFileA(
        lpExistingFileName: *const char,
        lpNewFileName:      *const char,
        bFailIfExists:      i32
    ): i32
    @lib pub fn CopyFileW(
        lpExistingFileName: *const u16,
        lpNewFileName:      *const u16,
        bFailIfExists:      i32
    ): i32
    @lib pub fn MoveFileA(
        lpExistingFileName: *const char,
        lpNewFileName:      *const char
    ): i32
    @lib pub fn MoveFileW(
        lpExistingFileName: *const u16,
        lpNewFileName:      *const u16
    ): i32
    @lib pub fn GetFileAttributesA(lpFileName: *const char): u32
    @lib pub fn GetFileAttributesW(lpFileName: *const u16): u32
    @lib pub fn SetFileAttributesA(
        lpFileName:     *const char,
        dwFileAttributes: u32
    ): i32
    @lib pub fn SetFileAttributesW(
        lpFileName:     *const u16,
        dwFileAttributes: u32
    ): i32

    // ── Directories ───────────────────────────────────────────
    @lib pub fn CreateDirectoryA(
        lpPathName:           *const char,
        lpSecurityAttributes: *void
    ): i32
    @lib pub fn CreateDirectoryW(
        lpPathName:           *const u16,
        lpSecurityAttributes: *void
    ): i32
    @lib pub fn RemoveDirectoryA(lpPathName: *const char): i32
    @lib pub fn RemoveDirectoryW(lpPathName: *const u16): i32
    @lib pub fn GetCurrentDirectoryA(nBufferLength: u32, lpBuffer: *char): u32
    @lib pub fn GetCurrentDirectoryW(nBufferLength: u32, lpBuffer: *u16): u32
    @lib pub fn SetCurrentDirectoryA(lpPathName: *const char): i32
    @lib pub fn SetCurrentDirectoryW(lpPathName: *const u16): i32
    @lib pub fn GetTempPathA(nBufferLength: u32, lpBuffer: *char): u32
    @lib pub fn GetTempPathW(nBufferLength: u32, lpBuffer: *u16): u32
    @lib pub fn GetTempFileNameA(
        lpPathName:    *const char,
        lpPrefixString: *const char,
        uUnique:       u32,
        lpTempFileName: *char
    ): u32
    @lib pub fn GetTempFileNameW(
        lpPathName:    *const u16,
        lpPrefixString: *const u16,
        uUnique:       u32,
        lpTempFileName: *u16
    ): u32

    // ── Activation Context (side-by-side assemblies) ─────────

    /// `ACTCTXW` — input to `CreateActCtxW`. Layout (x64):
    /// 4 + 4 + 8 + 2 + 2 + 4 + 8 + 8 + 8 + 8 = 56 bytes.
    pub struct ACTCTXW {
        cbSize:                u32
        dwFlags:               u32
        lpSource:              *const u16
        wProcessorArchitecture: u16
        wLangId:               u16
        _pad:                  u32
        lpAssemblyDirectory:   *const u16
        lpResourceName:        *const u16
        lpApplicationName:     *const u16
        hModule:               HMODULE
    }

    /// Build a side-by-side activation context. Use this to load
    /// e.g. comctl32 v6 (the themed control set) from an in-memory
    /// or on-disk manifest without rebuilding the .exe.
    /// Returns INVALID_HANDLE_VALUE (-1) on failure.
    @lib pub fn CreateActCtxW(pActCtx: *const ACTCTXW): i64

    /// Push `hActCtx` onto the calling thread's activation stack.
    /// `*lpCookie` receives a value passed to `DeactivateActCtx`.
    /// Returns non-zero on success.
    @lib pub fn ActivateActCtx(hActCtx: i64, lpCookie: *u64): i32

    /// Pop the activation context with the matching `ulCookie`.
    /// `dwFlags` is typically 0.
    @lib pub fn DeactivateActCtx(dwFlags: u32, ulCookie: u64): i32

    /// Decrement the ref count of an activation context; the
    /// kernel frees it when the count hits 0.
    @lib pub fn ReleaseActCtx(hActCtx: i64)

    // ── System / environment ──────────────────────────────────
    @lib pub fn GetCommandLineA(): *const char
    @lib pub fn GetCommandLineW(): *const u16
    @lib pub fn GetEnvironmentVariableA(
        lpName:   *const char,
        lpBuffer: *char,
        nSize:    u32
    ): u32
    @lib pub fn GetEnvironmentVariableW(
        lpName:   *const u16,
        lpBuffer: *u16,
        nSize:    u32
    ): u32
    @lib pub fn SetEnvironmentVariableA(
        lpName:  *const char,
        lpValue: *const char
    ): i32
    @lib pub fn SetEnvironmentVariableW(
        lpName:  *const u16,
        lpValue: *const u16
    ): i32
    @lib pub fn ExpandEnvironmentStringsA(
        lpSrc:    *const char,
        lpDst:    *char,
        nSize:    u32
    ): u32
    @lib pub fn ExpandEnvironmentStringsW(
        lpSrc:    *const u16,
        lpDst:    *u16,
        nSize:    u32
    ): u32
    @lib pub fn GetComputerNameA(lpBuffer: *char, nSize: *u32): i32
    @lib pub fn GetComputerNameW(lpBuffer: *u16, nSize: *u32): i32
    @lib pub fn GetUserNameA(lpBuffer: *char, pcbBuffer: *u32): i32
    @lib pub fn GetUserNameW(lpBuffer: *u16, pcbBuffer: *u32): i32

    // ── Process creation ──────────────────────────────────────────

    /// `STARTUPINFOA` — 68 bytes on x64.
    /// Minimal use: zero the struct, set `cb` = 68.
    /// To hide the console window set `dwFlags |= 0x00000001` (STARTF_USESHOWWINDOW)
    /// and `wShowWindow = 0` (SW_HIDE).
    pub struct STARTUPINFOA {
        cb:              u32
        lpReserved:      *char
        lpDesktop:       *char
        lpTitle:         *char
        dwX:             u32
        dwY:             u32
        dwXSize:         u32
        dwYSize:         u32
        dwXCountChars:   u32
        dwYCountChars:   u32
        dwFillAttribute: u32
        dwFlags:         u32
        wShowWindow:     u16
        cbReserved2:     u16
        lpReserved2:     *u8
        hStdInput:       HANDLE
        hStdOutput:      HANDLE
        hStdError:       HANDLE
    }

    /// `PROCESS_INFORMATION` — 24 bytes.
    /// CloseHandle both `hProcess` and `hThread` when done.
    pub struct PROCESS_INFORMATION {
        hProcess:    HANDLE
        hThread:     HANDLE
        dwProcessId: u32
        dwThreadId:  u32
    }

    /// Spawns a new process. Pass `0 as *const char` for
    /// `lpApplicationName` and put the full command line in
    /// `lpCommandLine` (which must be writable — CreateProcessA
    /// may modify it in-place).
    /// Returns non-zero on success; fills `lpProcessInformation`.
    @lib pub fn CreateProcessA(
        lpApplicationName:    *const char,
        lpCommandLine:        *char,
        lpProcessAttributes:  *void,
        lpThreadAttributes:   *void,
        bInheritHandles:      i32,
        dwCreationFlags:      u32,
        lpEnvironment:        *void,
        lpCurrentDirectory:   *const char,
        lpStartupInfo:        *const STARTUPINFOA,
        lpProcessInformation: *PROCESS_INFORMATION
    ): i32
    @lib pub fn CreateProcessW(
        lpApplicationName:    *const u16,
        lpCommandLine:        *u16,
        lpProcessAttributes:  *void,
        lpThreadAttributes:   *void,
        bInheritHandles:      i32,
        dwCreationFlags:      u32,
        lpEnvironment:        *void,
        lpCurrentDirectory:   *const u16,
        lpStartupInfo:        *const STARTUPINFOA,
        lpProcessInformation: *PROCESS_INFORMATION
    ): i32

    @lib pub fn OpenProcess(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        dwProcessId:     u32
    ): HANDLE

    @lib pub fn TerminateProcess(hProcess: HANDLE, uExitCode: u32): i32

    @lib pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *u32): i32

    // ── String / text utilities ───────────────────────────────────

    /// Formats a Win32 error message into a newly allocated
    /// buffer (pass `FORMAT_MESSAGE_ALLOCATE_BUFFER = 0x100`
    /// combined with `FORMAT_MESSAGE_FROM_SYSTEM = 0x1000` in
    /// `dwFlags`).  Free the buffer with `LocalFree`.
    @lib pub fn FormatMessageA(
        dwFlags:      u32,
        lpSource:     *const void,
        dwMessageId:  u32,
        dwLanguageId: u32,
        lpBuffer:     *char,
        nSize:        u32,
        arguments:    *void
    ): u32
    @lib pub fn FormatMessageW(
        dwFlags:      u32,
        lpSource:     *const void,
        dwMessageId:  u32,
        dwLanguageId: u32,
        lpBuffer:     *u16,
        nSize:        u32,
        arguments:    *void
    ): u32

    /// Frees memory allocated by `LocalAlloc` or `FormatMessageA`
    /// with `FORMAT_MESSAGE_ALLOCATE_BUFFER`. Returns NULL on success.
    @lib pub fn LocalFree(hMem: *void): *void

    /// Translates `cbMultiByte` bytes of ANSI/UTF-8 `lpMultiByteStr`
    /// to UTF-16LE in `lpWideCharStr`. Pass -1 for `cbMultiByte`
    /// to include the NUL terminator. Returns the number of UTF-16
    /// code units written (including NUL when -1 was passed).
    @lib pub fn MultiByteToWideChar(
        codePage:        u32,
        dwFlags:         u32,
        lpMultiByteStr:  *const char,
        cbMultiByte:     i32,
        lpWideCharStr:   *u16,
        cchWideChar:     i32
    ): i32

    /// Translates `cchWideChar` UTF-16LE code units to ANSI/UTF-8.
    /// Pass -1 for `cchWideChar` to include the NUL terminator.
    @lib pub fn WideCharToMultiByte(
        codePage:           u32,
        dwFlags:            u32,
        lpWideCharStr:      *const u16,
        cchWideChar:        i32,
        lpMultiByteStr:     *char,
        cbMultiByte:        i32,
        lpDefaultChar:      *const char,
        lpUsedDefaultChar:  *i32
    ): i32

    // ── Memory utilities ──────────────────────────────────────────

    /// Copies `Length` bytes from `Source` to `Destination`.
    /// Safe for overlapping regions (unlike memcpy).
    @lib pub fn RtlMoveMemory(
        Destination: *void,
        Source:      *const void,
        Length:      u64
    )

    // ── System information ────────────────────────────────────────

    pub struct SYSTEM_INFO {
        dwOemId:                  u32
        dwPageSize:               u32
        lpMinimumApplicationAddress: *void
        lpMaximumApplicationAddress: *void
        dwActiveProcessorMask:    u64
        dwNumberOfProcessors:     u32
        dwProcessorType:          u32
        dwAllocationGranularity:  u32
        wProcessorLevel:          u16
        wProcessorRevision:       u16
    }

    @lib pub fn GetSystemInfo(lpSystemInfo: *SYSTEM_INFO)

    pub struct OSVERSIONINFOA {
        dwOSVersionInfoSize: u32
        dwMajorVersion:      u32
        dwMinorVersion:      u32
        dwBuildNumber:       u32
        dwPlatformId:        u32
        szCSDVersion:        u8[128]
    }

    /// Deprecated on Windows 8.1+; returns 6.2 for later versions
    /// unless the app has a compatibility manifest.  Use
    /// `VerifyVersionInfoA` or `RtlGetVersion` (ntdll) for accurate
    /// version detection.
    @lib pub fn GetVersionExA(lpVersionInformation: *OSVERSIONINFOA): i32
    @lib pub fn GetVersionExW(lpVersionInformation: *OSVERSIONINFOA): i32

    // ── Path utilities ────────────────────────────────────────────

    @lib pub fn GetModuleFileNameA(
        hModule:  HMODULE,
        lpFilename: *char,
        nSize:    u32
    ): u32
    @lib pub fn GetModuleFileNameW(
        hModule:  HMODULE,
        lpFilename: *u16,
        nSize:    u32
    ): u32

    @lib pub fn GetFullPathNameA(
        lpFileName:   *const char,
        nBufferLength: u32,
        lpBuffer:     *char,
        lpFilePart:   **char
    ): u32
    @lib pub fn GetFullPathNameW(
        lpFileName:   *const u16,
        nBufferLength: u32,
        lpBuffer:     *u16,
        lpFilePart:   **u16
    ): u32

    @lib pub fn GetShortPathNameA(
        lpszLongPath:  *const char,
        lpszShortPath: *char,
        cchBuffer:     u32
    ): u32
    @lib pub fn GetShortPathNameW(
        lpszLongPath:  *const u16,
        lpszShortPath: *u16,
        cchBuffer:     u32
    ): u32

    @lib pub fn SearchPathA(
        lpPath:      *const char,
        lpFileName:  *const char,
        lpExtension: *const char,
        nBufferLength: u32,
        lpBuffer:    *char,
        lpFilePart:  **char
    ): u32
    @lib pub fn SearchPathW(
        lpPath:      *const u16,
        lpFileName:  *const u16,
        lpExtension: *const u16,
        nBufferLength: u32,
        lpBuffer:    *u16,
        lpFilePart:  **u16
    ): u32

    // ── Pipe / anonymous IPC ──────────────────────────────────────

    @lib pub fn CreatePipe(
        hReadPipe:        *HANDLE,
        hWritePipe:       *HANDLE,
        lpPipeAttributes: *void,
        nSize:            u32
    ): i32

    @lib pub fn ConnectNamedPipe(hNamedPipe: HANDLE, lpOverlapped: *void): i32

    @lib pub fn CreateNamedPipeA(
        lpName:               *const char,
        dwOpenMode:           u32,
        dwPipeMode:           u32,
        nMaxInstances:        u32,
        nOutBufferSize:       u32,
        nInBufferSize:        u32,
        nDefaultTimeOut:      u32,
        lpSecurityAttributes: *void
    ): HANDLE
    @lib pub fn CreateNamedPipeW(
        lpName:               *const u16,
        dwOpenMode:           u32,
        dwPipeMode:           u32,
        nMaxInstances:        u32,
        nOutBufferSize:       u32,
        nInBufferSize:        u32,
        nDefaultTimeOut:      u32,
        lpSecurityAttributes: *void
    ): HANDLE

    // ── Console extended ──────────────────────────────────────────

    @lib pub fn ReadConsoleA(
        hConsoleInput:          HANDLE,
        lpBuffer:               *void,
        nNumberOfCharsToRead:   u32,
        lpNumberOfCharsRead:    *u32,
        pInputControl:          *void
    ): i32
    @lib pub fn ReadConsoleW(
        hConsoleInput:          HANDLE,
        lpBuffer:               *void,
        nNumberOfCharsToRead:   u32,
        lpNumberOfCharsRead:    *u32,
        pInputControl:          *void
    ): i32

    @lib pub fn SetConsoleTextAttribute(
        hConsoleOutput: HANDLE,
        wAttributes:    u16
    ): i32

    @lib pub fn GetConsoleScreenBufferInfo(
        hConsoleOutput:       HANDLE,
        lpConsoleScreenBufferInfo: *void
    ): i32

    // ── File enumeration ──────────────────────────────────────────

    /// `WIN32_FIND_DATAA` — 320 bytes.
    pub struct WIN32_FIND_DATAA {
        dwFileAttributes:   u32
        ftCreationTime_lo:  u32   // FILETIME lo-dword
        ftCreationTime_hi:  u32   // FILETIME hi-dword
        ftLastAccessTime_lo: u32
        ftLastAccessTime_hi: u32
        ftLastWriteTime_lo: u32
        ftLastWriteTime_hi: u32
        nFileSizeHigh:      u32
        nFileSizeLow:       u32
        dwReserved0:        u32
        dwReserved1:        u32
        cFileName:          u8[260]
        cAlternateFileName: u8[14]
        _pad:               u16     // trailing alignment to DWORD
    }

    /// Begins a directory enumeration. Returns INVALID_HANDLE_VALUE
    /// on failure. Iterate with `FindNextFileA`; close with `FindClose`.
    /// `lpFileName` may include wildcards, e.g. "C:\\*" or "*.txt".
    @lib pub fn FindFirstFileA(
        lpFileName:  *const char,
        lpFindFileData: *WIN32_FIND_DATAA
    ): HANDLE
    @lib pub fn FindFirstFileW(
        lpFileName:  *const u16,
        lpFindFileData: *WIN32_FIND_DATAA
    ): HANDLE

    /// Advances to the next matching entry. Returns 0 when no more.
    @lib pub fn FindNextFileA(
        hFindFile:   HANDLE,
        lpFindFileData: *WIN32_FIND_DATAA
    ): i32
    @lib pub fn FindNextFileW(
        hFindFile:   HANDLE,
        lpFindFileData: *WIN32_FIND_DATAA
    ): i32

    @lib pub fn FindClose(hFindFile: HANDLE): i32

    // ── Date / time ───────────────────────────────────────────────

    @lib pub fn GetLocalTime(lpSystemTime: *SYSTEMTIME)
    @lib pub fn GetSystemTime(lpSystemTime: *SYSTEMTIME)
    @lib pub fn SetLocalTime(lpSystemTime: *const SYSTEMTIME): i32
    @lib pub fn SetSystemTime(lpSystemTime: *const SYSTEMTIME): i32

    @lib pub fn GetSystemTimeAsFileTime(lpSystemTimeAsFileTime: *FILETIME)

    @lib pub fn SystemTimeToFileTime(
        lpSystemTime: *const SYSTEMTIME,
        lpFileTime:   *FILETIME
    ): i32

    @lib pub fn FileTimeToSystemTime(
        lpFileTime:   *const FILETIME,
        lpSystemTime: *SYSTEMTIME
    ): i32

    /// Converts a UTC FILETIME to a local FILETIME.
    @lib pub fn FileTimeToLocalFileTime(
        lpFileTime:      *const FILETIME,
        lpLocalFileTime: *FILETIME
    ): i32

    @lib pub fn LocalFileTimeToFileTime(
        lpLocalFileTime: *const FILETIME,
        lpFileTime:      *FILETIME
    ): i32

    /// Compares two FILETIMEs. Returns -1, 0, or 1.
    @lib pub fn CompareFileTime(
        lpFileTime1: *const FILETIME,
        lpFileTime2: *const FILETIME
    ): i32

    @lib pub fn GetFileTime(
        hFile:             HANDLE,
        lpCreationTime:    *FILETIME,
        lpLastAccessTime:  *FILETIME,
        lpLastWriteTime:   *FILETIME
    ): i32

    @lib pub fn SetFileTime(
        hFile:            HANDLE,
        lpCreationTime:   *const FILETIME,
        lpLastAccessTime: *const FILETIME,
        lpLastWriteTime:  *const FILETIME
    ): i32

    // ── File mapping / shared memory ──────────────────────────────

    /// Creates or opens a named or anonymous file-backed mapping.
    /// `hFile`: use `0xFFFFFFFFFFFFFFFF as HANDLE` for a pagefile-
    /// backed mapping. `dwMaximumSize*`: combined 64-bit size.
    @lib pub fn CreateFileMappingA(
        hFile:                HANDLE,
        lpFileMappingAttributes: *void,
        flProtect:            u32,
        dwMaximumSizeHigh:    u32,
        dwMaximumSizeLow:     u32,
        lpName:               *const char
    ): HANDLE
    @lib pub fn CreateFileMappingW(
        hFile:                HANDLE,
        lpFileMappingAttributes: *void,
        flProtect:            u32,
        dwMaximumSizeHigh:    u32,
        dwMaximumSizeLow:     u32,
        lpName:               *const u16
    ): HANDLE

    @lib pub fn OpenFileMappingA(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        lpName:          *const char
    ): HANDLE
    @lib pub fn OpenFileMappingW(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        lpName:          *const u16
    ): HANDLE

    /// Maps the file/pagefile into the address space.
    /// `dwFileOffsetHigh/Low`: start offset (usually 0).
    /// `dwNumberOfBytesToMap`: 0 = map the whole thing.
    @lib pub fn MapViewOfFile(
        hFileMappingObject:  HANDLE,
        dwDesiredAccess:     u32,
        dwFileOffsetHigh:    u32,
        dwFileOffsetLow:     u32,
        dwNumberOfBytesToMap: u64
    ): *void

    @lib pub fn UnmapViewOfFile(lpBaseAddress: *const void): i32
    @lib pub fn FlushViewOfFile(lpBaseAddress: *const void, dwNumberOfBytesToFlush: u64): i32

    // ── Semaphore ─────────────────────────────────────────────────

    @lib pub fn CreateSemaphoreA(
        lpSemaphoreAttributes: *void,
        lInitialCount:         i32,
        lMaximumCount:         i32,
        lpName:                *const char
    ): HANDLE
    @lib pub fn CreateSemaphoreW(
        lpSemaphoreAttributes: *void,
        lInitialCount:         i32,
        lMaximumCount:         i32,
        lpName:                *const u16
    ): HANDLE

    @lib pub fn OpenSemaphoreA(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        lpName:          *const char
    ): HANDLE
    @lib pub fn OpenSemaphoreW(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        lpName:          *const u16
    ): HANDLE

    @lib pub fn ReleaseSemaphore(
        hSemaphore:      HANDLE,
        lReleaseCount:   i32,
        lpPreviousCount: *i32
    ): i32

    // ── WaitForMultipleObjects ────────────────────────────────────

    /// Waits for one or all of `nCount` handles.
    /// `bWaitAll`: 1 = wait for all, 0 = wait for any.
    /// Returns index of signalled handle (WAIT_OBJECT_0 + i),
    /// or 0x80..0x9F for abandoned mutexes, 0x102 for timeout.
    @lib pub fn WaitForMultipleObjects(
        nCount:         u32,
        lpHandles:      *HANDLE,
        bWaitAll:       i32,
        dwMilliseconds: u32
    ): u32

    // ── Drive info ────────────────────────────────────────────────

    /// Returns a bitmask of available drives (bit 0 = A:, bit 2 = C:).
    @lib pub fn GetLogicalDrives(): u32

    /// Returns the drive type. Result codes:
    ///   0=unknown, 1=no root, 2=removable, 3=fixed, 4=network,
    ///   5=CD-ROM, 6=RAM disk.
    @lib pub fn GetDriveTypeA(lpRootPathName: *const char): u32
    @lib pub fn GetDriveTypeW(lpRootPathName: *const u16): u32

    /// Fills `lpFreeBytesAvailableToCaller`, `lpTotalNumberOfBytes`,
    /// and `lpTotalNumberOfFreeBytes` (each *i64) with disk space info.
    @lib pub fn GetDiskFreeSpaceExA(
        lpDirectoryName:             *const char,
        lpFreeBytesAvailableToCaller: *i64,
        lpTotalNumberOfBytes:         *i64,
        lpTotalNumberOfFreeBytes:     *i64
    ): i32
    @lib pub fn GetDiskFreeSpaceExW(
        lpDirectoryName:             *const u16,
        lpFreeBytesAvailableToCaller: *i64,
        lpTotalNumberOfBytes:         *i64,
        lpTotalNumberOfFreeBytes:     *i64
    ): i32

    // ── System directory paths ────────────────────────────────────

    @lib pub fn GetWindowsDirectoryA(lpBuffer: *char, uSize: u32): u32
    @lib pub fn GetWindowsDirectoryW(lpBuffer: *u16, uSize: u32): u32
    @lib pub fn GetSystemDirectoryA(lpBuffer: *char, uSize: u32): u32
    @lib pub fn GetSystemDirectoryW(lpBuffer: *u16, uSize: u32): u32
    @lib pub fn GetSystemWindowsDirectoryA(lpBuffer: *char, uSize: u32): u32
    @lib pub fn GetSystemWindowsDirectoryW(lpBuffer: *u16, uSize: u32): u32

    // ── C-string helpers ─────────────────────────────────────────
    // Note: prefer using these only at OS API boundaries. These
    // functions live in kernel32 on Windows (not the C runtime).

    /// Copies `lpString2` into `lpString1` (must have room).
    @lib pub fn lstrcpyA(lpString1: *char, lpString2: *const char): *char
    @lib pub fn lstrcpyW(lpString1: *u16, lpString2: *const u16): *u16

    /// Appends `lpString2` to `lpString1`.
    @lib pub fn lstrcatA(lpString1: *char, lpString2: *const char): *char
    @lib pub fn lstrcatW(lpString1: *u16, lpString2: *const u16): *u16

    /// Returns the length of `lpString` in characters (excluding NUL).
    @lib pub fn lstrlenA(lpString: *const char): i32
    @lib pub fn lstrlenW(lpString: *const u16): i32

    /// Case-sensitive comparison. Returns 0 if equal.
    @lib pub fn lstrcmpA(lpString1: *const char, lpString2: *const char): i32
    @lib pub fn lstrcmpW(lpString1: *const u16, lpString2: *const u16): i32

    /// Case-insensitive comparison.
    @lib pub fn lstrcmpiA(lpString1: *const char, lpString2: *const char): i32
    @lib pub fn lstrcmpiW(lpString1: *const u16, lpString2: *const u16): i32

    // ── INI file I/O ─────────────────────────────────────────────

    /// Reads a string value from a private .ini file.
    @lib pub fn GetPrivateProfileStringA(
        lpAppName:     *const char,
        lpKeyName:     *const char,
        lpDefault:     *const char,
        lpReturnedString: *char,
        nSize:         u32,
        lpFileName:    *const char
    ): u32
    @lib pub fn GetPrivateProfileStringW(
        lpAppName:     *const u16,
        lpKeyName:     *const u16,
        lpDefault:     *const u16,
        lpReturnedString: *u16,
        nSize:         u32,
        lpFileName:    *const u16
    ): u32

    @lib pub fn WritePrivateProfileStringA(
        lpAppName:  *const char,
        lpKeyName:  *const char,
        lpString:   *const char,
        lpFileName: *const char
    ): i32
    @lib pub fn WritePrivateProfileStringW(
        lpAppName:  *const u16,
        lpKeyName:  *const u16,
        lpString:   *const u16,
        lpFileName: *const u16
    ): i32

    @lib pub fn GetPrivateProfileIntA(
        lpAppName:  *const char,
        lpKeyName:  *const char,
        nDefault:   i32,
        lpFileName: *const char
    ): u32
    @lib pub fn GetPrivateProfileIntW(
        lpAppName:  *const u16,
        lpKeyName:  *const u16,
        nDefault:   i32,
        lpFileName: *const u16
    ): u32

    // ── Debugging ────────────────────────────────────────────────

    /// Returns TRUE if a user-mode debugger is attached.
    @lib pub fn IsDebuggerPresent(): i32

    /// Triggers a breakpoint (raises a debug exception).
    @lib pub fn DebugBreak()

    /// Causes a fatal error (access violation) for crash-dump capture.
    @lib pub fn RaiseException(
        dwExceptionCode:    u32,
        dwExceptionFlags:   u32,
        nNumberOfArguments: u32,
        lpArguments:        *const u64
    )

    // ── Critical section ─────────────────────────────────────────

    /// `CRITICAL_SECTION` — 40 bytes on x64.
    /// Allocate, Initialize once, then Enter/Leave around critical regions.
    /// Cheaper than a mutex; not shareable across processes.
    pub struct CRITICAL_SECTION {
        DebugInfo:      *void
        LockCount:      i32
        RecursionCount: i32
        OwningThread:   *void
        LockSemaphore:  *void
        SpinCount:      u64
    }

    @lib pub fn InitializeCriticalSection(lpCriticalSection: *CRITICAL_SECTION)

    /// Like InitializeCriticalSection but spins `dwSpinCount` times
    /// before falling back to a wait — best for high-contention locks.
    @lib pub fn InitializeCriticalSectionAndSpinCount(
        lpCriticalSection: *CRITICAL_SECTION,
        dwSpinCount:       u32
    ): i32

    @lib pub fn EnterCriticalSection(lpCriticalSection: *CRITICAL_SECTION)
    @lib pub fn LeaveCriticalSection(lpCriticalSection: *CRITICAL_SECTION)

    /// Returns non-zero if the section was acquired without waiting.
    @lib pub fn TryEnterCriticalSection(lpCriticalSection: *CRITICAL_SECTION): i32

    @lib pub fn DeleteCriticalSection(lpCriticalSection: *CRITICAL_SECTION)

    // ── Slim Reader/Writer lock (SRWLOCK) ────────────────────────

    /// `SRWLOCK` — 8 bytes. Initialise with `InitializeSRWLock`
    /// or with the constant 0 (SRWLOCK_INIT).
    pub struct SRWLOCK {
        Ptr: *void
    }

    @lib pub fn InitializeSRWLock(SRWLock: *SRWLOCK)
    @lib pub fn AcquireSRWLockExclusive(SRWLock: *SRWLOCK)
    @lib pub fn ReleaseSRWLockExclusive(SRWLock: *SRWLOCK)
    @lib pub fn AcquireSRWLockShared(SRWLock: *SRWLOCK)
    @lib pub fn ReleaseSRWLockShared(SRWLock: *SRWLOCK)
    @lib pub fn TryAcquireSRWLockExclusive(SRWLock: *SRWLOCK): u8
    @lib pub fn TryAcquireSRWLockShared(SRWLock: *SRWLOCK): u8

    // ── Condition variable ───────────────────────────────────────

    /// `CONDITION_VARIABLE` — 8 bytes. Use with a CRITICAL_SECTION
    /// or SRWLOCK; the matching SleepCondition* variant releases the
    /// lock, waits, and re-acquires it atomically.
    pub struct CONDITION_VARIABLE {
        Ptr: *void
    }

    @lib pub fn InitializeConditionVariable(ConditionVariable: *CONDITION_VARIABLE)

    @lib pub fn SleepConditionVariableCS(
        ConditionVariable: *CONDITION_VARIABLE,
        CriticalSection:   *CRITICAL_SECTION,
        dwMilliseconds:    u32
    ): i32

    @lib pub fn SleepConditionVariableSRW(
        ConditionVariable: *CONDITION_VARIABLE,
        SRWLock:           *SRWLOCK,
        dwMilliseconds:    u32,
        Flags:             u32     // 0 = exclusive, 1 (CONDITION_VARIABLE_LOCKMODE_SHARED)
    ): i32

    @lib pub fn WakeConditionVariable(ConditionVariable: *CONDITION_VARIABLE)
    @lib pub fn WakeAllConditionVariable(ConditionVariable: *CONDITION_VARIABLE)

    // ── One-time initialization ──────────────────────────────────

    /// `INIT_ONCE` — 8 bytes. Initialise to 0 (INIT_ONCE_STATIC_INIT).
    pub struct INIT_ONCE {
        Ptr: *void
    }

    @lib pub fn InitOnceInitialize(InitOnce: *INIT_ONCE)

    /// Runs `InitFn(InitOnce, Parameter, Context)` exactly once, even
    /// across threads. `InitFn` should return non-zero on success.
    @lib pub fn InitOnceExecuteOnce(
        InitOnce:  *INIT_ONCE,
        InitFn:    fn(*INIT_ONCE, *void, **void): i32,
        Parameter: *void,
        Context:   **void
    ): i32

    // ── Thread tuning ────────────────────────────────────────────

    /// Sets the priority class of a process (e.g. 0x20 = NORMAL,
    /// 0x40 = IDLE, 0x80 = HIGH, 0x100 = REALTIME).
    @lib pub fn SetPriorityClass(hProcess: HANDLE, dwPriorityClass: u32): i32
    @lib pub fn GetPriorityClass(hProcess: HANDLE): u32

    /// Per-thread priority (-15..+15 typical; THREAD_PRIORITY_*).
    @lib pub fn SetThreadPriority(hThread: HANDLE, nPriority: i32): i32
    @lib pub fn GetThreadPriority(hThread: HANDLE): i32

    /// Restricts a thread to the CPUs in `dwThreadAffinityMask`.
    @lib pub fn SetThreadAffinityMask(hThread: HANDLE, dwThreadAffinityMask: u64): u64

    @lib pub fn SuspendThread(hThread: HANDLE): u32
    @lib pub fn ResumeThread(hThread: HANDLE): u32

    @lib pub fn GetCurrentProcess(): HANDLE
    @lib pub fn GetCurrentThread(): HANDLE

    // ── Library loading (extended) ───────────────────────────────

    /// `LoadLibraryA` with flags.  Common `dwFlags`:
    ///   0                              = same as LoadLibraryA
    ///   0x00000002 (LOAD_LIBRARY_AS_DATAFILE)
    ///   0x00000008 (DONT_RESOLVE_DLL_REFERENCES)
    ///   0x00000020 (LOAD_LIBRARY_AS_IMAGE_RESOURCE)
    ///   0x00000100 (LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) — secure
    @lib pub fn LoadLibraryExA(
        lpLibFileName: *const char,
        hFile:         HANDLE,
        dwFlags:       u32
    ): HMODULE
    @lib pub fn LoadLibraryExW(
        lpLibFileName: *const u16,
        hFile:         HANDLE,
        dwFlags:       u32
    ): HMODULE

    // ── Precise time ─────────────────────────────────────────────

    /// 100-ns resolution UTC time (Windows 8+).  Same units as
    /// `GetSystemTimeAsFileTime` but with full timer resolution.
    @lib pub fn GetSystemTimePreciseAsFileTime(lpSystemTimeAsFileTime: *FILETIME)

    // ── Timer queue (light-weight kernel timers) ─────────────────

    /// Creates a timer queue.  Free with `DeleteTimerQueueEx`.
    @lib pub fn CreateTimerQueue(): HANDLE

    /// Adds a timer to the queue.  `Period`: 0 = one-shot timer,
    /// otherwise the period in ms after the first fire.
    @lib pub fn CreateTimerQueueTimer(
        phNewTimer:   *HANDLE,
        TimerQueue:   HANDLE,
        Callback:     fn(*void, i32),
        Parameter:    *void,
        DueTime:      u32,
        Period:       u32,
        Flags:        u32
    ): i32

    @lib pub fn DeleteTimerQueueTimer(
        TimerQueue:        HANDLE,
        Timer:             HANDLE,
        CompletionEvent:   HANDLE
    ): i32

    // ── Virtual memory query ─────────────────────────────────────

    /// `MEMORY_BASIC_INFORMATION` — 48 bytes on x64.
    /// Filled by `VirtualQuery` / `VirtualQueryEx`.
    pub struct MEMORY_BASIC_INFORMATION {
        BaseAddress:       *void
        AllocationBase:    *void
        AllocationProtect: u32   // PageProtection.* at time of original alloc
        _pad0:             u32
        RegionSize:        u64
        State:             u32   // 0x1000 commit, 0x2000 reserve, 0x10000 free
        Protect:           u32   // current PageProtection.*
        Type:              u32   // 0x20000 private, 0x40000 mapped, 0x1000000 image
        _pad1:             u32
    }

    /// Queries the protection / state of pages at `lpAddress`.
    /// Returns the byte count written (48), or 0 on failure.
    @lib pub fn VirtualQuery(
        lpAddress: *const void,
        lpBuffer:  *MEMORY_BASIC_INFORMATION,
        dwLength:  u64
    ): u64

    /// Same but for an arbitrary process.
    @lib pub fn VirtualQueryEx(
        hProcess:  HANDLE,
        lpAddress: *const void,
        lpBuffer:  *MEMORY_BASIC_INFORMATION,
        dwLength:  u64
    ): u64

    @lib pub fn VirtualProtect(
        lpAddress:     *void,
        dwSize:        u64,
        flNewProtect:  u32,
        lpflOldProtect: *u32
    ): i32

    @lib pub fn VirtualLock(lpAddress: *void, dwSize: u64): i32
    @lib pub fn VirtualUnlock(lpAddress: *void, dwSize: u64): i32

    // ── WOW64 ────────────────────────────────────────────────────

    /// Returns 1 in `*Wow64Process` if `hProcess` is a 32-bit process
    /// running on 64-bit Windows.
    @lib pub fn IsWow64Process(hProcess: HANDLE, Wow64Process: *i32): i32

    // ── Process times ────────────────────────────────────────────

    /// Returns the kernel / user CPU time consumed by `hProcess`
    /// plus its creation and exit FILETIMEs.
    @lib pub fn GetProcessTimes(
        hProcess:       HANDLE,
        lpCreationTime: *FILETIME,
        lpExitTime:     *FILETIME,
        lpKernelTime:   *FILETIME,
        lpUserTime:     *FILETIME
    ): i32

    @lib pub fn GetThreadTimes(
        hThread:        HANDLE,
        lpCreationTime: *FILETIME,
        lpExitTime:     *FILETIME,
        lpKernelTime:   *FILETIME,
        lpUserTime:     *FILETIME
    ): i32

    // ── Globally-unique-name allocation (legacy) ─────────────────

    /// Allocates moveable global memory (GMEM_MOVEABLE = 0x42).
    /// Used by the clipboard API — `SetClipboardData` requires
    /// GMEM_MOVEABLE handles. Free with GlobalFree.
    @lib pub fn GlobalAlloc(uFlags: u32, dwBytes: u64): *void
    @lib pub fn GlobalFree(hMem: *void): *void

    /// Locks a moveable HGLOBAL and returns a pointer to its bytes.
    /// Pair with GlobalUnlock.
    @lib pub fn GlobalLock(hMem: *void): *void
    @lib pub fn GlobalUnlock(hMem: *void): i32

    @lib pub fn GlobalSize(hMem: *void): u64
    @lib pub fn GlobalReAlloc(hMem: *void, dwBytes: u64, uFlags: u32): *void

    // ── Handle duplication / inheritance ─────────────────────────

    /// `DuplicateHandle(srcProc, src, dstProc, &dst, access, inherit, options)`.
    /// Use `GetCurrentProcess()` for `srcProc`/`dstProc` to clone within
    /// this process. Set `bInheritHandle = 1` to let children inherit.
    /// `dwOptions`: `DuplicateHandleOptions.sameAccess` ignores `dwDesiredAccess`.
    @lib pub fn DuplicateHandle(
        hSourceProcessHandle:  HANDLE,
        hSourceHandle:         HANDLE,
        hTargetProcessHandle:  HANDLE,
        lpTargetHandle:        *HANDLE,
        dwDesiredAccess:       u32,
        bInheritHandle:        i32,
        dwOptions:             u32
    ): i32

    /// Sets the HANDLE_FLAG_* bits on `hObject`.
    @lib pub fn SetHandleInformation(
        hObject: HANDLE,
        dwMask:  u32,    // HandleFlags.*
        dwFlags: u32
    ): i32

    @lib pub fn GetHandleInformation(hObject: HANDLE, lpdwFlags: *u32): i32

    /// Win10+ : returns non-zero if both handles refer to the same kernel object.
    @lib pub fn CompareObjectHandles(hFirst: HANDLE, hSecond: HANDLE): i32

    // ── Process / thread (extended) ──────────────────────────────

    /// Opens an existing thread. Same access-rights pattern as OpenProcess.
    @lib pub fn OpenThread(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        dwThreadId:      u32
    ): HANDLE

    @lib pub fn TerminateThread(hThread: HANDLE, dwExitCode: u32): i32
    @lib pub fn GetExitCodeThread(hThread: HANDLE, lpExitCode: *u32): i32

    /// Vista+: returns the PID of `hProcess`.
    @lib pub fn GetProcessId(hProcess: HANDLE): u32
    @lib pub fn GetThreadId(hThread: HANDLE): u32
    @lib pub fn GetProcessIdOfThread(hThread: HANDLE): u32

    /// Yields the remainder of the current scheduling quantum.
    @lib pub fn SwitchToThread(): i32

    /// Queues a user-mode APC for `hThread`. The callback runs only
    /// when the thread enters an alertable wait (Sleep / WaitFor* / etc.
    /// with `bAlertable = TRUE`).
    @lib pub fn QueueUserAPC(
        pfnAPC:   fn(u64),
        hThread:  HANDLE,
        dwData:   u64
    ): u32

    /// Alertable Sleep. `bAlertable = 1` lets queued APCs run.
    @lib pub fn SleepEx(dwMilliseconds: u32, bAlertable: i32): u32

    @lib pub fn WaitForSingleObjectEx(
        hHandle:        HANDLE,
        dwMilliseconds: u32,
        bAlertable:     i32
    ): u32

    @lib pub fn WaitForMultipleObjectsEx(
        nCount:         u32,
        lpHandles:      *HANDLE,
        bWaitAll:       i32,
        dwMilliseconds: u32,
        bAlertable:     i32
    ): u32

    /// Atomically signals `hObjectToSignal` then waits on `hObjectToWaitOn`.
    @lib pub fn SignalObjectAndWait(
        hObjectToSignal:  HANDLE,
        hObjectToWaitOn:  HANDLE,
        dwMilliseconds:   u32,
        bAlertable:       i32
    ): u32

    /// Registers a wait callback in the threadpool. Free with `UnregisterWait` /
    /// `UnregisterWaitEx`. `dwFlags`: 0 = WT_EXECUTEDEFAULT, 4 = WT_EXECUTEINIOTHREAD,
    /// 8 = WT_EXECUTEINPERSISTENTTHREAD, 0x40 = WT_EXECUTELONGFUNCTION,
    /// 0x80 = WT_EXECUTEINUITHREAD, 0x8 = WT_EXECUTEONLYONCE.
    @lib pub fn RegisterWaitForSingleObject(
        phNewWaitObject: *HANDLE,
        hObject:         HANDLE,
        Callback:        fn(*void, u8),
        Context:         *void,
        dwMilliseconds:  u32,
        dwFlags:         u32
    ): i32

    @lib pub fn UnregisterWait(WaitHandle: HANDLE): i32
    @lib pub fn UnregisterWaitEx(WaitHandle: HANDLE, CompletionEvent: HANDLE): i32

    // ── Process tuning / info (extended) ─────────────────────────

    @lib pub fn GetProcessAffinityMask(
        hProcess:          HANDLE,
        lpProcessAffinityMask: *u64,
        lpSystemAffinityMask:  *u64
    ): i32

    @lib pub fn SetProcessAffinityMask(hProcess: HANDLE, dwProcessAffinityMask: u64): i32

    @lib pub fn GetCurrentProcessorNumber(): u32

    /// Win8+: sets the per-thread description (visible in ETW / debuggers).
    @lib pub fn SetThreadDescription(hThread: HANDLE, lpThreadDescription: *const u16): i32
    @lib pub fn GetThreadDescription(hThread: HANDLE, ppszThreadDescription: **u16): i32

    @lib pub fn FlushInstructionCache(
        hProcess: HANDLE,
        lpBaseAddress: *const void,
        dwSize: u64
    ): i32

    @lib pub fn GetStartupInfoA(lpStartupInfo: *STARTUPINFOA)

    /// Wide twin of GetStartupInfoA — pass a STARTUPINFOA-shaped buffer
    /// where the string pointers are *u16 instead of *char.
    @lib pub fn GetStartupInfoW(lpStartupInfo: *STARTUPINFOA)

    // ── Environment block ────────────────────────────────────────

    @lib pub fn GetEnvironmentStrings(): *char
    @lib pub fn GetEnvironmentStringsW(): *u16
    @lib pub fn FreeEnvironmentStringsA(penv: *char): i32
    @lib pub fn FreeEnvironmentStringsW(penv: *u16): i32

    // ── TLS ──────────────────────────────────────────────────────

    /// Allocates a new TLS slot index. Returns 0xFFFFFFFF on failure.
    @lib pub fn TlsAlloc(): u32
    @lib pub fn TlsFree(dwTlsIndex: u32): i32
    @lib pub fn TlsGetValue(dwTlsIndex: u32): *void
    @lib pub fn TlsSetValue(dwTlsIndex: u32, lpTlsValue: *void): i32

    // ── Fibers (cooperative threading) ───────────────────────────

    /// Converts the calling thread into a fiber. Required before any
    /// other fiber API on this thread.
    @lib pub fn ConvertThreadToFiber(lpParameter: *void): *void
    @lib pub fn ConvertFiberToThread(): i32

    @lib pub fn CreateFiber(
        dwStackSize:    u64,
        lpStartAddress: fn(*void),
        lpParameter:    *void
    ): *void

    @lib pub fn CreateFiberEx(
        dwStackCommitSize:  u64,
        dwStackReserveSize: u64,
        dwFlags:            u32,
        lpStartAddress:     fn(*void),
        lpParameter:        *void
    ): *void

    @lib pub fn DeleteFiber(lpFiber: *void)
    @lib pub fn SwitchToFiber(lpFiber: *void)
    @lib pub fn GetCurrentFiber(): *void
    @lib pub fn GetFiberData(): *void

    // ── Console (extended) ───────────────────────────────────────

    /// Reads / sets the console mode bits (see ConsoleInputMode /
    /// ConsoleOutputMode). Setting `virtualTerminalProcessing` on
    /// the output handle is the standard way to enable ANSI escape
    /// support on Windows 10 1607+.
    @lib pub fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *u32): i32
    @lib pub fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: u32): i32

    /// Returns the HWND of the attached console window, or NULL.
    @lib pub fn GetConsoleWindow(): HWND

    /// Active code-page accessors.
    @lib pub fn GetConsoleCP(): u32
    @lib pub fn GetConsoleOutputCP(): u32
    @lib pub fn SetConsoleCP(wCodePageID: u32): i32
    @lib pub fn SetConsoleOutputCP(wCodePageID: u32): i32

    /// `COORD { SHORT x; SHORT y; }` — 4 bytes.
    pub struct COORD {
        X: i16
        Y: i16
    }

    /// `SMALL_RECT { SHORT Left, Top, Right, Bottom; }` — 8 bytes.
    pub struct SMALL_RECT {
        Left:   i16
        Top:    i16
        Right:  i16
        Bottom: i16
    }

    /// `CONSOLE_SCREEN_BUFFER_INFO` — 22 bytes.
    pub struct CONSOLE_SCREEN_BUFFER_INFO {
        dwSize:              COORD
        dwCursorPosition:    COORD
        wAttributes:         u16
        srWindow:            SMALL_RECT
        dwMaximumWindowSize: COORD
    }

    @lib pub fn SetConsoleCursorPosition(hConsoleOutput: HANDLE, dwCursorPosition: COORD): i32

    @lib pub fn FillConsoleOutputCharacterA(
        hConsoleOutput:  HANDLE,
        cCharacter:      u8,
        nLength:         u32,
        dwWriteCoord:    COORD,
        lpNumberOfCharsWritten: *u32
    ): i32
    @lib pub fn FillConsoleOutputCharacterW(
        hConsoleOutput:  HANDLE,
        cCharacter:      u16,
        nLength:         u32,
        dwWriteCoord:    COORD,
        lpNumberOfCharsWritten: *u32
    ): i32

    @lib pub fn FillConsoleOutputAttribute(
        hConsoleOutput: HANDLE,
        wAttribute:     u16,
        nLength:        u32,
        dwWriteCoord:   COORD,
        lpNumberOfAttrsWritten: *u32
    ): i32

    @lib pub fn ScrollConsoleScreenBufferA(
        hConsoleOutput:  HANDLE,
        lpScrollRectangle: *const SMALL_RECT,
        lpClipRectangle: *const SMALL_RECT,
        dwDestinationOrigin: COORD,
        lpFill:          *const void   // *CHAR_INFO
    ): i32
    @lib pub fn ScrollConsoleScreenBufferW(
        hConsoleOutput:  HANDLE,
        lpScrollRectangle: *const SMALL_RECT,
        lpClipRectangle: *const SMALL_RECT,
        dwDestinationOrigin: COORD,
        lpFill:          *const void
    ): i32

    @lib pub fn FlushConsoleInputBuffer(hConsoleInput: HANDLE): i32

    @lib pub fn GetConsoleTitleA(lpConsoleTitle: *char, nSize: u32): u32
    @lib pub fn GetConsoleTitleW(lpConsoleTitle: *u16, nSize: u32): u32

    @lib pub fn AttachConsole(dwProcessId: u32): i32

    // ── Heap (extended) ──────────────────────────────────────────

    /// Creates a private heap. `flOptions`: HeapFlags.* (zero is fine).
    /// `dwInitialSize`/`dwMaximumSize`: 0 = growable.
    @lib pub fn HeapCreate(
        flOptions:     u32,
        dwInitialSize: u64,
        dwMaximumSize: u64
    ): HANDLE

    @lib pub fn HeapDestroy(hHeap: HANDLE): i32

    /// Returns the largest committed free block (or 0 on failure).
    @lib pub fn HeapCompact(hHeap: HANDLE, dwFlags: u32): u64

    /// `0` = inconsistent.
    @lib pub fn HeapValidate(hHeap: HANDLE, dwFlags: u32, lpMem: *const void): i32

    @lib pub fn HeapLock(hHeap: HANDLE): i32
    @lib pub fn HeapUnlock(hHeap: HANDLE): i32

    /// Fills `ProcessHeaps` with up to `NumberOfHeaps` HANDLEs and
    /// returns the actual count (or 0 on failure).
    @lib pub fn GetProcessHeaps(NumberOfHeaps: u32, ProcessHeaps: *HANDLE): u32

    // ── File / I/O (extended) ────────────────────────────────────

    /// Returns one of FileType.*.
    @lib pub fn GetFileType(hFile: HANDLE): u32

    /// Truncates or extends the file to the current file pointer.
    @lib pub fn SetEndOfFile(hFile: HANDLE): i32

    @lib pub fn LockFile(
        hFile: HANDLE,
        dwFileOffsetLow:  u32,
        dwFileOffsetHigh: u32,
        nNumberOfBytesToLockLow:  u32,
        nNumberOfBytesToLockHigh: u32
    ): i32

    @lib pub fn UnlockFile(
        hFile: HANDLE,
        dwFileOffsetLow:  u32,
        dwFileOffsetHigh: u32,
        nNumberOfBytesToUnlockLow:  u32,
        nNumberOfBytesToUnlockHigh: u32
    ): i32

    /// 8-byte OVERLAPPED-style file lock. `dwFlags`:
    ///   1 = LOCKFILE_EXCLUSIVE_LOCK, 2 = LOCKFILE_FAIL_IMMEDIATELY.
    @lib pub fn LockFileEx(
        hFile:           HANDLE,
        dwFlags:         u32,
        dwReserved:      u32,
        nNumberOfBytesToLockLow:  u32,
        nNumberOfBytesToLockHigh: u32,
        lpOverlapped:    *OVERLAPPED
    ): i32

    @lib pub fn UnlockFileEx(
        hFile:           HANDLE,
        dwReserved:      u32,
        nNumberOfBytesToUnlockLow:  u32,
        nNumberOfBytesToUnlockHigh: u32,
        lpOverlapped:    *OVERLAPPED
    ): i32

    /// `BY_HANDLE_FILE_INFORMATION` — 52 bytes (with trailing pad).
    /// Filled by GetFileInformationByHandle.
    pub struct BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes:     u32
        ftCreationTime:       FILETIME
        ftLastAccessTime:     FILETIME
        ftLastWriteTime:      FILETIME
        dwVolumeSerialNumber: u32
        nFileSizeHigh:        u32
        nFileSizeLow:         u32
        nNumberOfLinks:       u32
        nFileIndexHigh:       u32
        nFileIndexLow:        u32
    }

    @lib pub fn GetFileInformationByHandle(
        hFile: HANDLE,
        lpFileInformation: *BY_HANDLE_FILE_INFORMATION
    ): i32

    /// `dwFlags`: 0 = absolute (default), 1 = VOLUME_NAME_GUID,
    /// 2 = VOLUME_NAME_NT, 4 = VOLUME_NAME_NONE; OR with 0x8 for
    /// FILE_NAME_OPENED (the name used when the file was opened).
    @lib pub fn GetFinalPathNameByHandleA(
        hFile: HANDLE,
        lpszFilePath: *char,
        cchFilePath:  u32,
        dwFlags:      u32
    ): u32
    @lib pub fn GetFinalPathNameByHandleW(
        hFile: HANDLE,
        lpszFilePath: *u16,
        cchFilePath:  u32,
        dwFlags:      u32
    ): u32

    // ── Overlapped I/O ───────────────────────────────────────────

    /// `OVERLAPPED` — 32 bytes on x64. Embeds an Offset/InternalHigh
    /// union exposed here as two u64 fields.
    pub struct OVERLAPPED {
        Internal:      u64
        InternalHigh:  u64
        Offset:        u32
        OffsetHigh:    u32
        hEvent:        HANDLE
    }

    /// `OVERLAPPED_ENTRY` — 32 bytes. Returned by
    /// GetQueuedCompletionStatusEx.
    pub struct OVERLAPPED_ENTRY {
        lpCompletionKey:            u64
        lpOverlapped:               *OVERLAPPED
        Internal:                   u64
        dwNumberOfBytesTransferred: u32
        _pad:                       u32
    }

    @lib pub fn ReadFileEx(
        hFile:        HANDLE,
        lpBuffer:     *void,
        nNumberOfBytesToRead: u32,
        lpOverlapped: *OVERLAPPED,
        lpCompletionRoutine: fn(u32, u32, *OVERLAPPED)
    ): i32

    @lib pub fn WriteFileEx(
        hFile:        HANDLE,
        lpBuffer:     *const void,
        nNumberOfBytesToWrite: u32,
        lpOverlapped: *OVERLAPPED,
        lpCompletionRoutine: fn(u32, u32, *OVERLAPPED)
    ): i32

    @lib pub fn GetOverlappedResult(
        hFile:        HANDLE,
        lpOverlapped: *OVERLAPPED,
        lpNumberOfBytesTransferred: *u32,
        bWait:        i32
    ): i32

    @lib pub fn GetOverlappedResultEx(
        hFile:        HANDLE,
        lpOverlapped: *OVERLAPPED,
        lpNumberOfBytesTransferred: *u32,
        dwMilliseconds: u32,
        bAlertable:     i32
    ): i32

    /// Cancels all pending I/O on `hFile` issued by the calling thread.
    @lib pub fn CancelIo(hFile: HANDLE): i32

    /// Cancels pending I/O across threads; pass NULL OVERLAPPED to
    /// cancel everything.
    @lib pub fn CancelIoEx(hFile: HANDLE, lpOverlapped: *OVERLAPPED): i32

    /// Win7+: cancels a synchronous I/O issued by another thread.
    @lib pub fn CancelSynchronousIo(hThread: HANDLE): i32

    // ── I/O Completion Ports ─────────────────────────────────────

    /// Creates / associates an IOCP. To create a fresh port, pass
    /// `INVALID_HANDLE_VALUE` for `FileHandle` and NULL for `ExistingCompletionPort`.
    /// To attach `hFile` to an existing port, pass both.
    @lib pub fn CreateIoCompletionPort(
        FileHandle:                HANDLE,
        ExistingCompletionPort:    HANDLE,
        CompletionKey:             u64,
        NumberOfConcurrentThreads: u32
    ): HANDLE

    @lib pub fn GetQueuedCompletionStatus(
        CompletionPort:             HANDLE,
        lpNumberOfBytesTransferred: *u32,
        lpCompletionKey:            *u64,
        lpOverlapped:               **OVERLAPPED,
        dwMilliseconds:             u32
    ): i32

    @lib pub fn GetQueuedCompletionStatusEx(
        CompletionPort:        HANDLE,
        lpCompletionPortEntries: *OVERLAPPED_ENTRY,
        ulCount:               u32,
        ulNumEntriesRemoved:   *u32,
        dwMilliseconds:        u32,
        fAlertable:            i32
    ): i32

    @lib pub fn PostQueuedCompletionStatus(
        CompletionPort:           HANDLE,
        dwNumberOfBytesTransferred: u32,
        dwCompletionKey:          u64,
        lpOverlapped:             *OVERLAPPED
    ): i32

    // ── DeviceIoControl ──────────────────────────────────────────

    /// Generic ioctl. Build `dwIoControlCode` with the CTL_CODE macro
    /// (DeviceType<<16 | Access<<14 | Function<<2 | Method).
    @lib pub fn DeviceIoControl(
        hDevice:        HANDLE,
        dwIoControlCode: u32,
        lpInBuffer:     *void,
        nInBufferSize:  u32,
        lpOutBuffer:    *void,
        nOutBufferSize: u32,
        lpBytesReturned: *u32,
        lpOverlapped:   *OVERLAPPED
    ): i32

    // ── Toolhelp32 snapshots ─────────────────────────────────────

    /// 568-byte process entry returned by `Process32FirstW` /
    /// `Process32NextW`. Set `dwSize = 568` before the first call.
    pub struct PROCESSENTRY32W {
        dwSize:              u32
        cntUsage:            u32
        th32ProcessID:       u32
        th32DefaultHeapID:   u64
        th32ModuleID:        u32
        cntThreads:          u32
        th32ParentProcessID: u32
        pcPriClassBase:      i32
        dwFlags:             u32
        szExeFile:           u16[260]
    }

    /// 28-byte thread entry. Set `dwSize = 28`.
    pub struct THREADENTRY32 {
        dwSize:             u32
        cntUsage:           u32
        th32ThreadID:       u32
        th32OwnerProcessID: u32
        tpBasePri:          i32
        tpDeltaPri:         i32
        dwFlags:            u32
    }

    /// 1080-byte module entry (Unicode). Set `dwSize = 1080`.
    pub struct MODULEENTRY32W {
        dwSize:         u32
        th32ModuleID:   u32
        th32ProcessID:  u32
        GlblcntUsage:   u32
        ProccntUsage:   u32
        modBaseAddr:    *u8
        modBaseSize:    u32
        hModule:        HMODULE
        szModule:       u16[256]
        szExePath:      u16[260]
    }

    /// Snapshots the requested kernel objects (TH32CS_*).
    /// `th32ProcessID = 0` for the system-wide process snapshot.
    @lib pub fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32): HANDLE

    @lib pub fn Process32FirstW(hSnapshot: HANDLE, lppe: *PROCESSENTRY32W): i32
    @lib pub fn Process32NextW(hSnapshot: HANDLE, lppe: *PROCESSENTRY32W): i32

    @lib pub fn Thread32First(hSnapshot: HANDLE, lpte: *THREADENTRY32): i32
    @lib pub fn Thread32Next(hSnapshot: HANDLE, lpte: *THREADENTRY32): i32

    @lib pub fn Module32FirstW(hSnapshot: HANDLE, lpme: *MODULEENTRY32W): i32
    @lib pub fn Module32NextW(hSnapshot: HANDLE, lpme: *MODULEENTRY32W): i32

    // ── Job objects ──────────────────────────────────────────────

    @lib pub fn CreateJobObjectA(lpJobAttributes: *void, lpName: *const char): HANDLE
    @lib pub fn CreateJobObjectW(lpJobAttributes: *void, lpName: *const u16): HANDLE

    @lib pub fn OpenJobObjectA(dwDesiredAccess: u32, bInheritHandles: i32, lpName: *const char): HANDLE
    @lib pub fn OpenJobObjectW(dwDesiredAccess: u32, bInheritHandles: i32, lpName: *const u16): HANDLE

    @lib pub fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE): i32
    @lib pub fn TerminateJobObject(hJob: HANDLE, uExitCode: u32): i32

    /// `JobObjectInformationClass` codes (selected):
    ///   2  JobObjectBasicLimitInformation
    ///   8  JobObjectBasicProcessIdList
    ///   9  JobObjectExtendedLimitInformation
    ///  11  JobObjectGroupInformation
    @lib pub fn QueryInformationJobObject(
        hJob:           HANDLE,
        JobObjectInformationClass: i32,
        lpJobObjectInformation: *void,
        cbJobObjectInformationLength: u32,
        lpReturnLength: *u32
    ): i32

    @lib pub fn SetInformationJobObject(
        hJob:           HANDLE,
        JobObjectInformationClass: i32,
        lpJobObjectInformation: *void,
        cbJobObjectInformationLength: u32
    ): i32

    @lib pub fn IsProcessInJob(
        ProcessHandle: HANDLE,
        JobHandle:     HANDLE,
        Result:        *i32
    ): i32

    // ── Volume info ──────────────────────────────────────────────

    @lib pub fn GetVolumeInformationA(
        lpRootPathName:           *const char,
        lpVolumeNameBuffer:       *char,
        nVolumeNameSize:          u32,
        lpVolumeSerialNumber:     *u32,
        lpMaximumComponentLength: *u32,
        lpFileSystemFlags:        *u32,
        lpFileSystemNameBuffer:   *char,
        nFileSystemNameSize:      u32
    ): i32
    @lib pub fn GetVolumeInformationW(
        lpRootPathName:           *const u16,
        lpVolumeNameBuffer:       *u16,
        nVolumeNameSize:          u32,
        lpVolumeSerialNumber:     *u32,
        lpMaximumComponentLength: *u32,
        lpFileSystemFlags:        *u32,
        lpFileSystemNameBuffer:   *u16,
        nFileSystemNameSize:      u32
    ): i32

    @lib pub fn GetVolumePathNameA(
        lpszFileName:       *const char,
        lpszVolumePathName: *char,
        cchBufferLength:    u32
    ): i32
    @lib pub fn GetVolumePathNameW(
        lpszFileName:       *const u16,
        lpszVolumePathName: *u16,
        cchBufferLength:    u32
    ): i32

    @lib pub fn FindFirstVolumeA(lpszVolumeName: *char,  cchBufferLength: u32): HANDLE
    @lib pub fn FindFirstVolumeW(lpszVolumeName: *u16,   cchBufferLength: u32): HANDLE
    @lib pub fn FindNextVolumeA(hFindVolume: HANDLE, lpszVolumeName: *char, cchBufferLength: u32): i32
    @lib pub fn FindNextVolumeW(hFindVolume: HANDLE, lpszVolumeName: *u16, cchBufferLength: u32): i32
    @lib pub fn FindVolumeClose(hFindVolume: HANDLE): i32

    // ── Locale / NLS ─────────────────────────────────────────────

    /// Returns the system default ANSI code page.
    @lib pub fn GetACP(): u32
    @lib pub fn GetOEMCP(): u32

    /// `CPINFO` — 12 bytes. MaxCharSize=1 means single-byte.
    pub struct CPINFO {
        MaxCharSize:    u32
        DefaultChar:    u8[2]
        LeadByte:       u8[12]
    }

    @lib pub fn GetCPInfo(CodePage: u32, lpCPInfo: *CPINFO): i32

    @lib pub fn IsValidCodePage(CodePage: u32): i32

    /// Compares two strings using the rules of `Locale`. Returns
    /// CSTR_LESS_THAN (1), CSTR_EQUAL (2), CSTR_GREATER_THAN (3),
    /// or 0 on failure.
    @lib pub fn CompareStringA(
        Locale:     u32,
        dwCmpFlags: u32,
        lpString1:  *const char,
        cchCount1:  i32,
        lpString2:  *const char,
        cchCount2:  i32
    ): i32
    @lib pub fn CompareStringW(
        Locale:     u32,
        dwCmpFlags: u32,
        lpString1:  *const u16,
        cchCount1:  i32,
        lpString2:  *const u16,
        cchCount2:  i32
    ): i32

    /// Win7+ locale-name variant.
    @lib pub fn CompareStringEx(
        lpLocaleName: *const u16,
        dwCmpFlags:   u32,
        lpString1:    *const u16,
        cchCount1:    i32,
        lpString2:    *const u16,
        cchCount2:    i32,
        lpVersionInformation: *void,
        lpReserved:   *void,
        lParam:       i64
    ): i32

    @lib pub fn LCMapStringA(
        Locale:      u32,
        dwMapFlags:  u32,
        lpSrcStr:    *const char,
        cchSrc:      i32,
        lpDestStr:   *char,
        cchDest:     i32
    ): i32
    @lib pub fn LCMapStringW(
        Locale:      u32,
        dwMapFlags:  u32,
        lpSrcStr:    *const u16,
        cchSrc:      i32,
        lpDestStr:   *u16,
        cchDest:     i32
    ): i32

    @lib pub fn GetUserDefaultLocaleName(lpLocaleName: *u16, cchLocaleName: i32): i32
    @lib pub fn GetSystemDefaultLocaleName(lpLocaleName: *u16, cchLocaleName: i32): i32
    @lib pub fn GetUserDefaultLangID(): u16
    @lib pub fn GetSystemDefaultLangID(): u16
    @lib pub fn GetThreadUILanguage(): u16
    @lib pub fn SetThreadUILanguage(LangId: u16): u16
    @lib pub fn GetUserDefaultUILanguage(): u16
    @lib pub fn GetSystemDefaultUILanguage(): u16

    // ── Time zone ────────────────────────────────────────────────

    /// `TIME_ZONE_INFORMATION` — 172 bytes.
    pub struct TIME_ZONE_INFORMATION {
        Bias:         i32
        StandardName: u16[32]
        StandardDate: SYSTEMTIME
        StandardBias: i32
        DaylightName: u16[32]
        DaylightDate: SYSTEMTIME
        DaylightBias: i32
    }

    /// Returns TIME_ZONE_ID_UNKNOWN (0), TIME_ZONE_ID_STANDARD (1),
    /// or TIME_ZONE_ID_DAYLIGHT (2). 0xFFFFFFFF on failure.
    @lib pub fn GetTimeZoneInformation(lpTimeZoneInformation: *TIME_ZONE_INFORMATION): u32

    @lib pub fn SetTimeZoneInformation(lpTimeZoneInformation: *const TIME_ZONE_INFORMATION): i32

    @lib pub fn SystemTimeToTzSpecificLocalTime(
        lpTimeZoneInformation: *const TIME_ZONE_INFORMATION,
        lpUniversalTime:       *const SYSTEMTIME,
        lpLocalTime:           *SYSTEMTIME
    ): i32

    @lib pub fn TzSpecificLocalTimeToSystemTime(
        lpTimeZoneInformation: *const TIME_ZONE_INFORMATION,
        lpLocalTime:           *const SYSTEMTIME,
        lpUniversalTime:       *SYSTEMTIME
    ): i32

    // ── WaitOnAddress / WakeByAddress (Win8+) ────────────────────

    /// Blocks until `*Address` differs from `*CompareAddress`.
    /// `AddressSize`: 1, 2, 4, or 8.
    @lib pub fn WaitOnAddress(
        Address:        *const void,
        CompareAddress: *const void,
        AddressSize:    u64,
        dwMilliseconds: u32
    ): i32

    @lib pub fn WakeByAddressSingle(Address: *const void)
    @lib pub fn WakeByAddressAll(Address: *const void)

    // ── Threadpool (Vista+) ──────────────────────────────────────

    /// Submits `Callback(InstanceMaybe, Context, WorkOrNull)` to the
    /// default threadpool. For more control use CreateThreadpoolWork.
    @lib pub fn TrySubmitThreadpoolCallback(
        pfns:     fn(*void, *void),
        pv:       *void,
        pcbe:     *void   // *TP_CALLBACK_ENVIRON
    ): i32

    /// `PTP_POOL` / `PTP_WORK` etc. are opaque handles. We expose
    /// them as *void to keep this surface compact.
    @lib pub fn CreateThreadpool(reserved: *void): *void
    @lib pub fn CloseThreadpool(ptpp: *void)
    @lib pub fn SetThreadpoolThreadMinimum(ptpp: *void, cthrdMic: u32): i32
    @lib pub fn SetThreadpoolThreadMaximum(ptpp: *void, cthrdMost: u32)

    @lib pub fn CreateThreadpoolWork(
        pfnwk: fn(*void, *void, *void),
        pv:    *void,
        pcbe:  *void
    ): *void
    @lib pub fn SubmitThreadpoolWork(pwk: *void)
    @lib pub fn WaitForThreadpoolWorkCallbacks(pwk: *void, fCancelPendingCallbacks: i32)
    @lib pub fn CloseThreadpoolWork(pwk: *void)

    @lib pub fn CreateThreadpoolTimer(
        pfnti: fn(*void, *void, *void),
        pv:    *void,
        pcbe:  *void
    ): *void
    @lib pub fn SetThreadpoolTimer(
        pti:           *void,
        pftDueTime:    *const FILETIME,   // NULL = cancel
        msPeriod:      u32,
        msWindowLength: u32
    )
    @lib pub fn WaitForThreadpoolTimerCallbacks(pti: *void, fCancelPendingCallbacks: i32)
    @lib pub fn CloseThreadpoolTimer(pti: *void)

    // ── Stack walking ────────────────────────────────────────────

    /// Captures up to `FramesToCapture` return addresses into
    /// `BackTrace`. `BackTraceHash` is optional (NULL = skip).
    @lib pub fn RtlCaptureStackBackTrace(
        FramesToSkip:    u32,
        FramesToCapture: u32,
        BackTrace:       **void,
        BackTraceHash:   *u32
    ): u16

    // ── Misc utilities ───────────────────────────────────────────

    /// Returns the host's logical-processor count (sum across groups).
    @lib pub fn GetActiveProcessorCount(GroupNumber: u16): u32

    @lib pub fn GetActiveProcessorGroupCount(): u16

    /// `QueryUnbiasedInterruptTime` — Win7+, returns the count of
    /// 100-ns intervals the system has been awake (excludes sleep).
    @lib pub fn QueryUnbiasedInterruptTime(UnbiasedTime: *u64): i32

    @lib pub fn QueryInterruptTime(InterruptTime: *u64)

    /// Re-raises the last error as an SEH exception (for top-level
    /// crash filters that want to attach a debugger).
    @lib pub fn RaiseFailFastException(
        pExceptionRecord: *void,
        pContextRecord:   *void,
        dwFlags:          u32
    )

    @lib pub fn Beep(dwFreq: u32, dwDuration: u32): i32

    /// Returns `0` if no error mode is set; otherwise the bitmask.
    /// `SetErrorMode` returns the previous value.
    @lib pub fn SetErrorMode(uMode: u32): u32
    @lib pub fn GetErrorMode(): u32

    /// Sets / queries the thread error mode (Vista+).
    @lib pub fn SetThreadErrorMode(NewMode: u32, OldMode: *u32): i32
    @lib pub fn GetThreadErrorMode(): u32

    // ── Atomic / interlocked (linker-exported wrappers) ──────────
    // Most compilers inline these as intrinsics; the kernel32 wrappers
    // exist for FFI callers and for parity with the InterlockedXxx
    // names used in the SDK.

    @lib pub fn InterlockedIncrement(Addend: *i32): i32
    @lib pub fn InterlockedDecrement(Addend: *i32): i32
    @lib pub fn InterlockedExchange(Target: *i32, Value: i32): i32
    @lib pub fn InterlockedExchangeAdd(Addend: *i32, Value: i32): i32
    @lib pub fn InterlockedCompareExchange(Destination: *i32, Exchange: i32, Comparand: i32): i32
    @lib pub fn InterlockedCompareExchange64(
        Destination: *i64, Exchange: i64, Comparand: i64
    ): i64

    // ── Waitable timers ──────────────────────────────────────────

    @lib pub fn CreateWaitableTimerA(
        lpTimerAttributes: *void,
        bManualReset:      i32,
        lpTimerName:       *const char
    ): HANDLE
    @lib pub fn CreateWaitableTimerW(
        lpTimerAttributes: *void,
        bManualReset:      i32,
        lpTimerName:       *const u16
    ): HANDLE

    @lib pub fn CreateWaitableTimerExA(
        lpTimerAttributes: *void,
        lpTimerName:       *const char,
        dwFlags:           u32,    // CreateWaitableTimerFlags.*
        dwDesiredAccess:   u32
    ): HANDLE
    @lib pub fn CreateWaitableTimerExW(
        lpTimerAttributes: *void,
        lpTimerName:       *const u16,
        dwFlags:           u32,
        dwDesiredAccess:   u32
    ): HANDLE

    @lib pub fn OpenWaitableTimerA(dwDesiredAccess: u32, bInheritHandle: i32, lpTimerName: *const char): HANDLE
    @lib pub fn OpenWaitableTimerW(dwDesiredAccess: u32, bInheritHandle: i32, lpTimerName: *const u16): HANDLE

    /// `lpDueTime`: negative = relative, 100-ns units; positive = absolute UTC FILETIME.
    /// `lPeriod`: 0 = one-shot, otherwise period in ms after the first fire.
    @lib pub fn SetWaitableTimer(
        hTimer:           HANDLE,
        lpDueTime:        *const i64,
        lPeriod:          i32,
        pfnCompletionRoutine: fn(*void, u32, u32),
        lpArgToCompletionRoutine: *void,
        fResume:          i32
    ): i32

    @lib pub fn SetWaitableTimerEx(
        hTimer:           HANDLE,
        lpDueTime:        *const i64,
        lPeriod:          i32,
        pfnCompletionRoutine: fn(*void, u32, u32),
        lpArgToCompletionRoutine: *void,
        WakeContext:      *void,
        TolerableDelay:   u32
    ): i32

    @lib pub fn CancelWaitableTimer(hTimer: HANDLE): i32

    // ── Synchronization-object opens ─────────────────────────────

    @lib pub fn OpenMutexA(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const char): HANDLE
    @lib pub fn OpenMutexW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16): HANDLE

    @lib pub fn OpenEventA(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const char): HANDLE
    @lib pub fn OpenEventW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16): HANDLE

    // ── Cross-process memory ─────────────────────────────────────

    @lib pub fn ReadProcessMemory(
        hProcess:           HANDLE,
        lpBaseAddress:      *const void,
        lpBuffer:           *void,
        nSize:              u64,
        lpNumberOfBytesRead: *u64
    ): i32

    @lib pub fn WriteProcessMemory(
        hProcess:           HANDLE,
        lpBaseAddress:      *void,
        lpBuffer:           *const void,
        nSize:              u64,
        lpNumberOfBytesWritten: *u64
    ): i32

    @lib pub fn VirtualAllocEx(
        hProcess:         HANDLE,
        lpAddress:        *void,
        dwSize:           u64,
        flAllocationType: u32,
        flProtect:        u32
    ): *void

    @lib pub fn VirtualFreeEx(
        hProcess:   HANDLE,
        lpAddress:  *void,
        dwSize:     u64,
        dwFreeType: u32
    ): i32

    @lib pub fn VirtualProtectEx(
        hProcess:       HANDLE,
        lpAddress:      *void,
        dwSize:         u64,
        flNewProtect:   u32,
        lpflOldProtect: *u32
    ): i32

    // ── Process image / version ──────────────────────────────────

    /// Vista+: returns the full image path. `dwFlags`: 0 = Win32 path,
    /// 1 = native (`\Device\HarddiskVolumeN\...`).
    @lib pub fn QueryFullProcessImageNameA(
        hProcess:  HANDLE,
        dwFlags:   u32,
        lpExeName: *char,
        lpdwSize:  *u32
    ): i32
    @lib pub fn QueryFullProcessImageNameW(
        hProcess:  HANDLE,
        dwFlags:   u32,
        lpExeName: *u16,
        lpdwSize:  *u32
    ): i32

    @lib pub fn GetProcessHandleCount(hProcess: HANDLE, pdwHandleCount: *u32): i32

    /// Returns the major/minor version of the Win32 subsystem the
    /// process was built for. Pass 0 for the calling process.
    @lib pub fn GetProcessVersion(ProcessId: u32): u32

    /// Returns one of: 0 = unknown, 0x0001 = user objects,
    /// 0x0002 = GDI objects. Useful for diagnosing handle leaks.
    @lib pub fn GetGuiResources(hProcess: HANDLE, uiFlags: u32): u32

    // ── Module extras ────────────────────────────────────────────

    /// `dwFlags`: 0 (default), 1 = GET_MODULE_HANDLE_EX_FLAG_PIN
    /// (refuses to unload), 2 = FROM_ADDRESS, 4 = UNCHANGED_REFCOUNT.
    @lib pub fn GetModuleHandleExA(
        dwFlags:     u32,
        lpModuleName: *const char,
        phModule:    *HMODULE
    ): i32
    @lib pub fn GetModuleHandleExW(
        dwFlags:     u32,
        lpModuleName: *const u16,
        phModule:    *HMODULE
    ): i32

    /// Stops the OS from calling `DllMain(DLL_THREAD_*)` for `hLibModule`.
    @lib pub fn DisableThreadLibraryCalls(hLibModule: HMODULE): i32

    // ── Resources (read-only access to .rsrc) ────────────────────

    @lib pub fn FindResourceA(
        hModule: HMODULE,
        lpName:  *const char,
        lpType:  *const char
    ): HRSRC
    @lib pub fn FindResourceW(
        hModule: HMODULE,
        lpName:  *const u16,
        lpType:  *const u16
    ): HRSRC

    @lib pub fn FindResourceExA(
        hModule:   HMODULE,
        lpType:    *const char,
        lpName:    *const char,
        wLanguage: u16
    ): HRSRC
    @lib pub fn FindResourceExW(
        hModule:   HMODULE,
        lpType:    *const u16,
        lpName:    *const u16,
        wLanguage: u16
    ): HRSRC

    @lib pub fn LoadResource(hModule: HMODULE, hResInfo: HRSRC): HANDLE
    @lib pub fn LockResource(hResData: HANDLE): *void
    @lib pub fn SizeofResource(hModule: HMODULE, hResInfo: HRSRC): u32
    @lib pub fn FreeResource(hResData: HANDLE): i32

    @lib pub fn EnumResourceTypesA(
        hModule:    HMODULE,
        lpEnumFunc: fn(HMODULE, *char, i64): i32,
        lParam:     i64
    ): i32
    @lib pub fn EnumResourceTypesW(
        hModule:    HMODULE,
        lpEnumFunc: fn(HMODULE, *u16, i64): i32,
        lParam:     i64
    ): i32

    @lib pub fn EnumResourceNamesA(
        hModule:    HMODULE,
        lpType:     *const char,
        lpEnumFunc: fn(HMODULE, *const char, *char, i64): i32,
        lParam:     i64
    ): i32
    @lib pub fn EnumResourceNamesW(
        hModule:    HMODULE,
        lpType:     *const u16,
        lpEnumFunc: fn(HMODULE, *const u16, *u16, i64): i32,
        lParam:     i64
    ): i32

    @lib pub fn BeginUpdateResourceA(pFileName: *const char, bDeleteExistingResources: i32): HANDLE
    @lib pub fn BeginUpdateResourceW(pFileName: *const u16, bDeleteExistingResources: i32): HANDLE

    @lib pub fn UpdateResourceA(
        hUpdate:   HANDLE,
        lpType:    *const char,
        lpName:    *const char,
        wLanguage: u16,
        lpData:    *const void,
        cb:        u32
    ): i32
    @lib pub fn UpdateResourceW(
        hUpdate:   HANDLE,
        lpType:    *const u16,
        lpName:    *const u16,
        wLanguage: u16,
        lpData:    *const void,
        cb:        u32
    ): i32

    @lib pub fn EndUpdateResourceA(hUpdate: HANDLE, fDiscard: i32): i32
    @lib pub fn EndUpdateResourceW(hUpdate: HANDLE, fDiscard: i32): i32

    // ── Console input / control handler ──────────────────────────

    /// 16-byte key event record (matches the C union layout).
    pub struct KEY_EVENT_RECORD {
        bKeyDown:          i32
        wRepeatCount:      u16
        wVirtualKeyCode:   u16
        wVirtualScanCode:  u16
        /// Union of UnicodeChar / AsciiChar — we expose the wide view.
        uChar:             u16
        dwControlKeyState: u32
    }

    pub struct MOUSE_EVENT_RECORD {
        dwMousePosition:   COORD
        dwButtonState:     u32
        dwControlKeyState: u32
        dwEventFlags:      u32
    }

    pub struct WINDOW_BUFFER_SIZE_RECORD {
        dwSize: COORD
    }

    pub struct MENU_EVENT_RECORD {
        dwCommandId: u32
    }

    pub struct FOCUS_EVENT_RECORD {
        bSetFocus: i32
    }

    /// `INPUT_RECORD` — 20 bytes. The Event union is exposed as
    /// an opaque 16-byte buffer; reinterpret based on `EventType`
    /// (one of `InputEventType.*`).
    pub struct INPUT_RECORD {
        EventType: u16
        _pad:      u16
        Event:     u8[16]
    }

    @lib pub fn PeekConsoleInputA(
        hConsoleInput:        HANDLE,
        lpBuffer:             *INPUT_RECORD,
        nLength:              u32,
        lpNumberOfEventsRead: *u32
    ): i32
    @lib pub fn PeekConsoleInputW(
        hConsoleInput:        HANDLE,
        lpBuffer:             *INPUT_RECORD,
        nLength:              u32,
        lpNumberOfEventsRead: *u32
    ): i32

    @lib pub fn ReadConsoleInputA(
        hConsoleInput:        HANDLE,
        lpBuffer:             *INPUT_RECORD,
        nLength:              u32,
        lpNumberOfEventsRead: *u32
    ): i32
    @lib pub fn ReadConsoleInputW(
        hConsoleInput:        HANDLE,
        lpBuffer:             *INPUT_RECORD,
        nLength:              u32,
        lpNumberOfEventsRead: *u32
    ): i32

    @lib pub fn WriteConsoleInputA(
        hConsoleInput:           HANDLE,
        lpBuffer:                *const INPUT_RECORD,
        nLength:                 u32,
        lpNumberOfEventsWritten: *u32
    ): i32
    @lib pub fn WriteConsoleInputW(
        hConsoleInput:           HANDLE,
        lpBuffer:                *const INPUT_RECORD,
        nLength:                 u32,
        lpNumberOfEventsWritten: *u32
    ): i32

    @lib pub fn GetNumberOfConsoleInputEvents(
        hConsoleInput:    HANDLE,
        lpNumberOfEvents: *u32
    ): i32

    /// Installs a Ctrl+C / Ctrl+Break / window-close handler.
    /// Pass `0 as fn(u32): i32` for HandlerRoutine and `Add = 1`
    /// to ignore Ctrl+C in the current process.
    @lib pub fn SetConsoleCtrlHandler(
        HandlerRoutine: fn(u32): i32,
        Add:            i32
    ): i32

    @lib pub fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32): i32

    /// `CONSOLE_CURSOR_INFO` — 8 bytes.
    pub struct CONSOLE_CURSOR_INFO {
        dwSize:   u32
        bVisible: i32
    }

    @lib pub fn GetConsoleCursorInfo(
        hConsoleOutput:        HANDLE,
        lpConsoleCursorInfo:   *CONSOLE_CURSOR_INFO
    ): i32
    @lib pub fn SetConsoleCursorInfo(
        hConsoleOutput:        HANDLE,
        lpConsoleCursorInfo:   *const CONSOLE_CURSOR_INFO
    ): i32

    @lib pub fn GetConsoleProcessList(lpdwProcessList: *u32, dwProcessCount: u32): u32

    @lib pub fn SetConsoleScreenBufferSize(hConsoleOutput: HANDLE, dwSize: COORD): i32

    @lib pub fn SetConsoleWindowInfo(
        hConsoleOutput:  HANDLE,
        bAbsolute:       i32,
        lpConsoleWindow: *const SMALL_RECT
    ): i32

    @lib pub fn GetLargestConsoleWindowSize(hConsoleOutput: HANDLE): COORD

    @lib pub fn SetConsoleActiveScreenBuffer(hConsoleOutput: HANDLE): i32

    @lib pub fn CreateConsoleScreenBuffer(
        dwDesiredAccess:      u32,
        dwShareMode:          u32,
        lpSecurityAttributes: *void,
        dwFlags:              u32,   // CONSOLE_TEXTMODE_BUFFER = 1
        lpScreenBufferData:   *void
    ): HANDLE

    // ── Named pipe extended ──────────────────────────────────────

    @lib pub fn WaitNamedPipeA(lpNamedPipeName: *const char, nTimeOut: u32): i32
    @lib pub fn WaitNamedPipeW(lpNamedPipeName: *const u16, nTimeOut: u32): i32

    @lib pub fn SetNamedPipeHandleState(
        hNamedPipe:           HANDLE,
        lpMode:               *u32,
        lpMaxCollectionCount: *u32,
        lpCollectDataTimeout: *u32
    ): i32

    @lib pub fn PeekNamedPipe(
        hNamedPipe:             HANDLE,
        lpBuffer:               *void,
        nBufferSize:            u32,
        lpBytesRead:            *u32,
        lpTotalBytesAvail:      *u32,
        lpBytesLeftThisMessage: *u32
    ): i32

    @lib pub fn GetNamedPipeInfo(
        hNamedPipe:      HANDLE,
        lpFlags:         *u32,
        lpOutBufferSize: *u32,
        lpInBufferSize:  *u32,
        lpMaxInstances:  *u32
    ): i32

    @lib pub fn DisconnectNamedPipe(hNamedPipe: HANDLE): i32

    @lib pub fn TransactNamedPipe(
        hNamedPipe:     HANDLE,
        lpInBuffer:     *void,
        nInBufferSize:  u32,
        lpOutBuffer:    *void,
        nOutBufferSize: u32,
        lpBytesRead:    *u32,
        lpOverlapped:   *OVERLAPPED
    ): i32

    @lib pub fn CallNamedPipeA(
        lpNamedPipeName: *const char,
        lpInBuffer:      *void,
        nInBufferSize:   u32,
        lpOutBuffer:     *void,
        nOutBufferSize:  u32,
        lpBytesRead:     *u32,
        nTimeOut:        u32
    ): i32
    @lib pub fn CallNamedPipeW(
        lpNamedPipeName: *const u16,
        lpInBuffer:      *void,
        nInBufferSize:   u32,
        lpOutBuffer:     *void,
        nOutBufferSize:  u32,
        lpBytesRead:     *u32,
        nTimeOut:        u32
    ): i32

    // ── System info / memory (extended) ──────────────────────────

    /// Like GetSystemInfo but always reports the *native* machine info
    /// even from a WOW64 process.
    @lib pub fn GetNativeSystemInfo(lpSystemInfo: *SYSTEM_INFO)

    /// `MEMORYSTATUSEX` — 64 bytes. Set `dwLength = 64` before calling.
    pub struct MEMORYSTATUSEX {
        dwLength:                u32
        dwMemoryLoad:            u32
        ullTotalPhys:            u64
        ullAvailPhys:            u64
        ullTotalPageFile:        u64
        ullAvailPageFile:        u64
        ullTotalVirtual:         u64
        ullAvailVirtual:         u64
        ullAvailExtendedVirtual: u64
    }

    @lib pub fn GlobalMemoryStatusEx(lpBuffer: *MEMORYSTATUSEX): i32

    /// Vista+: returns physically installed RAM in kilobytes.
    @lib pub fn GetPhysicallyInstalledSystemMemory(TotalMemoryInKilobytes: *u64): i32

    @lib pub fn IsProcessorFeaturePresent(ProcessorFeature: u32): i32

    @lib pub fn GetLogicalProcessorInformation(
        Buffer:         *void,
        ReturnedLength: *u32
    ): i32

    /// `RelationshipType`: 0 = ProcessorCore, 1 = NumaNode, 2 = Cache,
    /// 3 = ProcessorPackage, 4 = Group, 7 = Module, 0xFFFF = All.
    @lib pub fn GetLogicalProcessorInformationEx(
        RelationshipType: i32,
        Buffer:           *void,
        ReturnedLength:   *u32
    ): i32

    /// Reads an ACPI/SMBIOS/firmware table. Provider sigs:
    ///   'ACPI' (0x41435049), 'FIRM' (0x4649524D), 'RSMB' (0x52534D42).
    @lib pub fn GetSystemFirmwareTable(
        FirmwareTableProviderSignature: u32,
        FirmwareTableID:                u32,
        pFirmwareTableBuffer:           *void,
        BufferSize:                     u32
    ): u32

    // ── Power / sleep prevention ─────────────────────────────────

    /// Prevents display sleep / system sleep while the calling thread
    /// is alive (or until cleared with `ES_CONTINUOUS` alone).
    /// Returns the previous state flags.
    @lib pub fn SetThreadExecutionState(esFlags: u32): u32

    // ── WOW64 / 32-on-64 redirection ─────────────────────────────

    /// Win10+: returns the host (native) and process (emulated) machine
    /// IDs (IMAGE_FILE_MACHINE_*).
    @lib pub fn IsWow64Process2(
        hProcess:        HANDLE,
        pProcessMachine: *u16,
        pNativeMachine:  *u16
    ): i32

    @lib pub fn Wow64DisableWow64FsRedirection(OldValue: **void): i32
    @lib pub fn Wow64RevertWow64FsRedirection(OldValue: *void): i32

    @lib pub fn GetSystemWow64DirectoryA(lpBuffer: *char, uSize: u32): u32
    @lib pub fn GetSystemWow64DirectoryW(lpBuffer: *u16, uSize: u32): u32

    // ── Atom tables (clipboard / RPC helpers — legacy) ───────────

    @lib pub fn GlobalAddAtomA(lpString: *const char): u16
    @lib pub fn GlobalAddAtomW(lpString: *const u16): u16
    @lib pub fn GlobalDeleteAtom(nAtom: u16): u16
    @lib pub fn GlobalFindAtomA(lpString: *const char): u16
    @lib pub fn GlobalFindAtomW(lpString: *const u16): u16
    @lib pub fn GlobalGetAtomNameA(nAtom: u16, lpBuffer: *char, nSize: i32): u32
    @lib pub fn GlobalGetAtomNameW(nAtom: u16, lpBuffer: *u16, nSize: i32): u32

    @lib pub fn AddAtomA(lpString: *const char): u16
    @lib pub fn AddAtomW(lpString: *const u16): u16
    @lib pub fn DeleteAtom(nAtom: u16): u16
    @lib pub fn FindAtomA(lpString: *const char): u16
    @lib pub fn FindAtomW(lpString: *const u16): u16

    // ── Hard links / symbolic links / file replace / compress ────

    @lib pub fn CreateHardLinkA(
        lpFileName:           *const char,
        lpExistingFileName:   *const char,
        lpSecurityAttributes: *void
    ): i32
    @lib pub fn CreateHardLinkW(
        lpFileName:           *const u16,
        lpExistingFileName:   *const u16,
        lpSecurityAttributes: *void
    ): i32

    /// Requires SE_CREATE_SYMBOLIC_LINK_NAME privilege unless
    /// `SymbolicLinkFlags.allowUnprivilegedCreate` is OR'd in and
    /// Developer Mode is on.
    @lib pub fn CreateSymbolicLinkA(
        lpSymlinkFileName: *const char,
        lpTargetFileName:  *const char,
        dwFlags:           u32
    ): u8
    @lib pub fn CreateSymbolicLinkW(
        lpSymlinkFileName: *const u16,
        lpTargetFileName:  *const u16,
        dwFlags:           u32
    ): u8

    /// Atomically swaps `lpReplacedFileName` with `lpReplacementFileName`.
    /// `lpBackupFileName`: optional renamed copy of the original (NULL to discard).
    /// `dwReplaceFlags`: 0 = default; 1 = REPLACEFILE_WRITE_THROUGH,
    /// 2 = REPLACEFILE_IGNORE_MERGE_ERRORS, 4 = REPLACEFILE_IGNORE_ACL_ERRORS.
    @lib pub fn ReplaceFileA(
        lpReplacedFileName:    *const char,
        lpReplacementFileName: *const char,
        lpBackupFileName:      *const char,
        dwReplaceFlags:        u32,
        lpExclude:             *void,
        lpReserved:            *void
    ): i32
    @lib pub fn ReplaceFileW(
        lpReplacedFileName:    *const u16,
        lpReplacementFileName: *const u16,
        lpBackupFileName:      *const u16,
        dwReplaceFlags:        u32,
        lpExclude:             *void,
        lpReserved:            *void
    ): i32

    @lib pub fn GetCompressedFileSizeA(lpFileName: *const char, lpFileSizeHigh: *u32): u32
    @lib pub fn GetCompressedFileSizeW(lpFileName: *const u16, lpFileSizeHigh: *u32): u32

    // ── FindFirstFileEx ──────────────────────────────────────────

    /// Faster directory listing. `fInfoLevelId`: `FindexInfoLevel.basic`
    /// skips the `cAlternateFileName` field. `dwAdditionalFlags`:
    /// 1 = FIND_FIRST_EX_CASE_SENSITIVE, 2 = FIND_FIRST_EX_LARGE_FETCH.
    @lib pub fn FindFirstFileExA(
        lpFileName:        *const char,
        fInfoLevelId:      i32,
        lpFindFileData:    *WIN32_FIND_DATAA,
        fSearchOp:         i32,
        lpSearchFilter:    *void,
        dwAdditionalFlags: u32
    ): HANDLE
    @lib pub fn FindFirstFileExW(
        lpFileName:        *const u16,
        fInfoLevelId:      i32,
        lpFindFileData:    *WIN32_FIND_DATAA,
        fSearchOp:         i32,
        lpSearchFilter:    *void,
        dwAdditionalFlags: u32
    ): HANDLE

    // ── DOS device names ─────────────────────────────────────────

    /// Translates a DOS device name (e.g. "C:") to its NT target path.
    @lib pub fn QueryDosDeviceA(lpDeviceName: *const char, lpTargetPath: *char, ucchMax: u32): u32
    @lib pub fn QueryDosDeviceW(lpDeviceName: *const u16, lpTargetPath: *u16, ucchMax: u32): u32

    @lib pub fn DefineDosDeviceA(dwFlags: u32, lpDeviceName: *const char, lpTargetPath: *const char): i32
    @lib pub fn DefineDosDeviceW(dwFlags: u32, lpDeviceName: *const u16, lpTargetPath: *const u16): i32

    // ── Volume / drive ───────────────────────────────────────────

    /// Returns the volume GUIDs for all paths sharing the volume of
    /// `lpszVolumeName` (NUL-separated, double-NUL terminated).
    @lib pub fn GetVolumePathNamesForVolumeNameA(
        lpszVolumeName:      *const char,
        lpszVolumePathNames: *char,
        cchBufferLength:     u32,
        lpcchReturnLength:   *u32
    ): i32
    @lib pub fn GetVolumePathNamesForVolumeNameW(
        lpszVolumeName:      *const u16,
        lpszVolumePathNames: *u16,
        cchBufferLength:     u32,
        lpcchReturnLength:   *u32
    ): i32

    @lib pub fn GetVolumeNameForVolumeMountPointA(
        lpszVolumeMountPoint: *const char,
        lpszVolumeName:       *char,
        cchBufferLength:      u32
    ): i32
    @lib pub fn GetVolumeNameForVolumeMountPointW(
        lpszVolumeMountPoint: *const u16,
        lpszVolumeName:       *u16,
        cchBufferLength:      u32
    ): i32

    @lib pub fn SetVolumeMountPointA(
        lpszVolumeMountPoint: *const char,
        lpszVolumeName:       *const char
    ): i32
    @lib pub fn SetVolumeMountPointW(
        lpszVolumeMountPoint: *const u16,
        lpszVolumeName:       *const u16
    ): i32

    @lib pub fn DeleteVolumeMountPointA(lpszVolumeMountPoint: *const char): i32
    @lib pub fn DeleteVolumeMountPointW(lpszVolumeMountPoint: *const u16): i32

    // ── Locale formatting ────────────────────────────────────────

    @lib pub fn GetDateFormatA(
        Locale:    u32,
        dwFlags:   u32,
        lpDate:    *const SYSTEMTIME,
        lpFormat:  *const char,
        lpDateStr: *char,
        cchDate:   i32
    ): i32
    @lib pub fn GetDateFormatW(
        Locale:    u32,
        dwFlags:   u32,
        lpDate:    *const SYSTEMTIME,
        lpFormat:  *const u16,
        lpDateStr: *u16,
        cchDate:   i32
    ): i32

    /// Win7+: locale-name variant.
    @lib pub fn GetDateFormatEx(
        lpLocaleName: *const u16,
        dwFlags:      u32,
        lpDate:       *const SYSTEMTIME,
        lpFormat:     *const u16,
        lpDateStr:    *u16,
        cchDate:      i32,
        lpCalendar:   *const u16
    ): i32

    @lib pub fn GetTimeFormatA(
        Locale:    u32,
        dwFlags:   u32,
        lpTime:    *const SYSTEMTIME,
        lpFormat:  *const char,
        lpTimeStr: *char,
        cchTime:   i32
    ): i32
    @lib pub fn GetTimeFormatW(
        Locale:    u32,
        dwFlags:   u32,
        lpTime:    *const SYSTEMTIME,
        lpFormat:  *const u16,
        lpTimeStr: *u16,
        cchTime:   i32
    ): i32

    @lib pub fn GetTimeFormatEx(
        lpLocaleName: *const u16,
        dwFlags:      u32,
        lpTime:       *const SYSTEMTIME,
        lpFormat:     *const u16,
        lpTimeStr:    *u16,
        cchTime:      i32
    ): i32

    @lib pub fn GetNumberFormatA(
        Locale:       u32,
        dwFlags:      u32,
        lpValue:      *const char,
        lpFormat:     *const void,   // *NUMBERFMTA (not bound)
        lpNumberStr:  *char,
        cchNumber:    i32
    ): i32
    @lib pub fn GetNumberFormatW(
        Locale:       u32,
        dwFlags:      u32,
        lpValue:      *const u16,
        lpFormat:     *const void,
        lpNumberStr:  *u16,
        cchNumber:    i32
    ): i32

    @lib pub fn GetCurrencyFormatA(
        Locale:        u32,
        dwFlags:       u32,
        lpValue:       *const char,
        lpFormat:      *const void,
        lpCurrencyStr: *char,
        cchCurrency:   i32
    ): i32
    @lib pub fn GetCurrencyFormatW(
        Locale:        u32,
        dwFlags:       u32,
        lpValue:       *const u16,
        lpFormat:      *const void,
        lpCurrencyStr: *u16,
        cchCurrency:   i32
    ): i32

    @lib pub fn GetGeoInfoA(
        Location:    u32,
        GeoType:     u32,
        lpGeoData:   *char,
        cchData:     i32,
        LangId:      u16
    ): i32
    @lib pub fn GetGeoInfoW(
        Location:    u32,
        GeoType:     u32,
        lpGeoData:   *u16,
        cchData:     i32,
        LangId:      u16
    ): i32

    @lib pub fn GetUserGeoID(GeoClass: u32): u32
    @lib pub fn SetUserGeoID(GeoId: u32): i32

    // ── File information by handle (extended) ────────────────────

    /// `FileInformationClass` codes (selected): 0 FileBasicInfo,
    /// 1 FileStandardInfo, 2 FileNameInfo, 3 FileRenameInfo (set),
    /// 5 FileStreamInfo, 6 FileCompressionInfo, 7 FileAttributeTagInfo,
    /// 8 FileIdBothDirectoryInfo (Vista+), 18 FileDispositionInfo (delete).
    @lib pub fn GetFileInformationByHandleEx(
        hFile:                HANDLE,
        FileInformationClass: i32,
        lpFileInformation:    *void,
        dwBufferSize:         u32
    ): i32

    @lib pub fn SetFileInformationByHandle(
        hFile:                HANDLE,
        FileInformationClass: i32,
        lpFileInformation:    *void,
        dwBufferSize:         u32
    ): i32

    // ── Misc time / path ─────────────────────────────────────────

    /// Win11+: per-user temp path that survives session resets.
    /// Falls back to GetTempPath on older systems via the loader.
    @lib pub fn GetTempPath2A(BufferLength: u32, Buffer: *char): u32
    @lib pub fn GetTempPath2W(BufferLength: u32, Buffer: *u16): u32

    @lib pub fn GetSystemTimeAdjustment(
        lpTimeAdjustment:         *u32,
        lpTimeIncrement:          *u32,
        lpTimeAdjustmentDisabled: *i32
    ): i32

    // ── Activation context (A variant + ref counting) ────────────

    @lib pub fn CreateActCtxA(pActCtx: *const void): i64
    @lib pub fn AddRefActCtx(hActCtx: i64)

    @lib pub fn FindActCtxSectionStringA(
        dwFlags:           u32,
        lpExtensionGuid:   *const GUID,
        ulSectionId:       u32,
        lpStringToFind:    *const char,
        ReturnedData:      *void
    ): i32
    @lib pub fn FindActCtxSectionStringW(
        dwFlags:           u32,
        lpExtensionGuid:   *const GUID,
        ulSectionId:       u32,
        lpStringToFind:    *const u16,
        ReturnedData:      *void
    ): i32

    // ── Synchronization barriers (Vista+) ────────────────────────

    /// `SYNCHRONIZATION_BARRIER` — 32 bytes; opaque.
    pub struct SYNCHRONIZATION_BARRIER {
        Reserved1: u32
        Reserved2: u32
        Reserved3: u64[2]
        Reserved4: u32
        Reserved5: u32
    }

    @lib pub fn InitializeSynchronizationBarrier(
        lpBarrier:     *SYNCHRONIZATION_BARRIER,
        lTotalThreads: i32,
        lSpinCount:    i32
    ): i32

    /// `dwFlags`: 0, 1 = SYNCHRONIZATION_BARRIER_FLAGS_SPIN_ONLY,
    /// 2 = SYNCHRONIZATION_BARRIER_FLAGS_BLOCK_ONLY,
    /// 4 = SYNCHRONIZATION_BARRIER_FLAGS_NO_DELETE.
    @lib pub fn EnterSynchronizationBarrier(
        lpBarrier: *SYNCHRONIZATION_BARRIER,
        dwFlags:   u32
    ): i32

    @lib pub fn DeleteSynchronizationBarrier(lpBarrier: *SYNCHRONIZATION_BARRIER): i32

    // ── Misc utilities ───────────────────────────────────────────

    /// Computes `(nNumber * nNumerator + nDenominator/2) / nDenominator`
    /// with 64-bit intermediates. Returns -1 on overflow.
    @lib pub fn MulDiv(nNumber: i32, nNumerator: i32, nDenominator: i32): i32

    /// Set process working-set bounds (advisory). Pass `-1` for either
    /// value to keep it unchanged.
    @lib pub fn SetProcessWorkingSetSize(
        hProcess:           HANDLE,
        dwMinimumWorkingSetSize: u64,
        dwMaximumWorkingSetSize: u64
    ): i32

    @lib pub fn GetProcessWorkingSetSize(
        hProcess:           HANDLE,
        lpMinimumWorkingSetSize: *u64,
        lpMaximumWorkingSetSize: *u64
    ): i32

    /// `Vista+`: returns 16-bit Windows version of the process the
    /// thread that called Sleep(0) yields to.
    @lib pub fn GetProcessIoCounters(hProcess: HANDLE, lpIoCounters: *void): i32

    /// Sets the working-set bounds and policy class. `Flags`:
    /// 1 = QUOTA_LIMITS_HARDWS_MIN_ENABLE, 2 = MIN_DISABLE,
    /// 4 = MAX_ENABLE, 8 = MAX_DISABLE.
    @lib pub fn SetProcessWorkingSetSizeEx(
        hProcess:           HANDLE,
        dwMinimumWorkingSetSize: u64,
        dwMaximumWorkingSetSize: u64,
        Flags:              u32
    ): i32

    /// Returns the calling thread's stack guarantee (Vista+).
    @lib pub fn SetThreadStackGuarantee(StackSizeInBytes: *u32): i32
}
