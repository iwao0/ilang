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

use winnt { HANDLE, GUID, SYSTEMTIME, FILETIME, FileAttribute, PageProtection }
use windef { HMODULE }

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
}
