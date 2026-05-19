// kernel32.dll — core OS services: processes, threads, files,
// memory, synchronisation, timing, console, and DLL loading.
//
// Notes:
//   INVALID_HANDLE_VALUE  = `0xFFFFFFFFFFFFFFFF as *HANDLE`
//   INFINITE (no timeout) = `0xFFFFFFFF` as u32 for WaitForSingleObject

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

// ─── File attributes (FILE_ATTRIBUTE_*) ──────────────────────
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

// ─── Page-protection flags (PAGE_*) ──────────────────────────
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

// ─── Standard handle IDs for GetStdHandle (STD_*) ────────────
pub enum StdHandle: u32 {
    input  = 0xFFFFFFF6   // STD_INPUT_HANDLE
    output = 0xFFFFFFF5   // STD_OUTPUT_HANDLE
    error  = 0xFFFFFFF4   // STD_ERROR_HANDLE
}

@extern(C, "kernel32") {
    /// Generic kernel-object handle (`void *` in the SDK).
    /// Used for files, threads, mutexes, events, etc.
    pub struct HANDLE {}

    /// Module / executable image handle (`HMODULE == HINSTANCE`
    /// in the SDK). Cast to `*HINSTANCE` when passing to user32.
    pub struct HMODULE {}

    // ── Module ────────────────────────────────────────────────
    /// Returns the HMODULE of the running EXE when
    /// `lpModuleName` is `0 as *const char` (NULL).
    @lib pub fn GetModuleHandleA(lpModuleName: *const char): *HMODULE

    // ── Error ─────────────────────────────────────────────────
    /// Returns the last Win32 error set on this thread.
    @lib pub fn GetLastError(): u32
    @lib pub fn SetLastError(dwErrCode: u32)

    // ── Handle ────────────────────────────────────────────────
    /// Closes a kernel object handle. Returns non-zero on success.
    @lib pub fn CloseHandle(hObject: *HANDLE): i32

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
    ): *HANDLE

    @lib pub fn ExitThread(dwExitCode: u32)

    /// Blocks until `hHandle` is signalled or the timeout expires.
    /// Pass `0xFFFFFFFF` (INFINITE) to wait forever.
    @lib pub fn WaitForSingleObject(hHandle: *HANDLE, dwMilliseconds: u32): u32

    // ── Synchronisation objects ───────────────────────────────
    /// Creates a mutex. Pass `0 as *const char` for an anonymous mutex.
    @lib pub fn CreateMutexA(
        lpMutexAttributes: *void,
        bInitialOwner:     i32,
        lpName:            *const char
    ): *HANDLE
    @lib pub fn ReleaseMutex(hMutex: *HANDLE): i32

    /// Creates a manual- or auto-reset event.
    @lib pub fn CreateEventA(
        lpEventAttributes: *void,
        bManualReset:      i32,
        bInitialState:     i32,
        lpName:            *const char
    ): *HANDLE
    @lib pub fn SetEvent(hEvent: *HANDLE): i32
    @lib pub fn ResetEvent(hEvent: *HANDLE): i32

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

    /// Returns the standard input/output/error handle.
    /// Pass `StdHandle.output as u32` etc.
    @lib pub fn GetStdHandle(nStdHandle: u32): *HANDLE

    @lib pub fn WriteConsoleA(
        hConsoleOutput:         *HANDLE,
        lpBuffer:               *const char,
        nNumberOfCharsToWrite:  u32,
        lpNumberOfCharsWritten: *u32,
        lpReserved:             *void
    ): i32

    /// Allocates a new console window for the current process.
    /// Useful for GUI apps that want debug console output.
    @lib pub fn AllocConsole(): i32
    @lib pub fn FreeConsole(): i32
    @lib pub fn SetConsoleTitleA(lpConsoleTitle: *const char): i32

    // ── DLL loading ───────────────────────────────────────────
    @lib pub fn LoadLibraryA(lpLibFileName: *const char): *HMODULE
    @lib pub fn FreeLibrary(hLibModule: *HMODULE): i32
    /// Returns NULL if the export is not found.
    @lib pub fn GetProcAddress(
        hModule:    *HMODULE,
        lpProcName: *const char
    ): *void

    // ── Memory ────────────────────────────────────────────────
    /// Returns the default heap for the current process.
    @lib pub fn GetProcessHeap(): *HANDLE
    /// `dwFlags`: 0 = default, 8 = HEAP_ZERO_MEMORY.
    @lib pub fn HeapAlloc(hHeap: *HANDLE, dwFlags: u32, dwBytes: u64): *void
    @lib pub fn HeapFree(hHeap: *HANDLE, dwFlags: u32, lpMem: *void): i32
    @lib pub fn HeapReAlloc(
        hHeap:   *HANDLE,
        dwFlags: u32,
        lpMem:   *void,
        dwBytes: u64
    ): *void
    @lib pub fn HeapSize(hHeap: *HANDLE, dwFlags: u32, lpMem: *const void): u64

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
    /// (`0xFFFFFFFFFFFFFFFF as *HANDLE`) on failure.
    /// `lpOverlapped`: pass `0 as *void` for synchronous I/O.
    @lib pub fn CreateFileA(
        lpFileName:            *const char,
        dwDesiredAccess:       u32,
        dwShareMode:           u32,
        lpSecurityAttributes:  *void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes:  u32,
        hTemplateFile:         *HANDLE
    ): *HANDLE

    @lib pub fn ReadFile(
        hFile:                *HANDLE,
        lpBuffer:             *void,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead:  *u32,
        lpOverlapped:         *void
    ): i32

    @lib pub fn WriteFile(
        hFile:                  *HANDLE,
        lpBuffer:               *const void,
        nNumberOfBytesToWrite:  u32,
        lpNumberOfBytesWritten: *u32,
        lpOverlapped:           *void
    ): i32

    /// Writes the file size as a 64-bit signed integer to `lpFileSize`.
    @lib pub fn GetFileSizeEx(hFile: *HANDLE, lpFileSize: *i64): i32
    @lib pub fn SetFilePointerEx(
        hFile:            *HANDLE,
        liDistanceToMove: i64,
        lpNewFilePointer: *i64,
        dwMoveMethod:     u32
    ): i32
    @lib pub fn FlushFileBuffers(hFile: *HANDLE): i32
    @lib pub fn DeleteFileA(lpFileName: *const char): i32
    @lib pub fn CopyFileA(
        lpExistingFileName: *const char,
        lpNewFileName:      *const char,
        bFailIfExists:      i32
    ): i32
    @lib pub fn MoveFileA(
        lpExistingFileName: *const char,
        lpNewFileName:      *const char
    ): i32
    @lib pub fn GetFileAttributesA(lpFileName: *const char): u32
    @lib pub fn SetFileAttributesA(
        lpFileName:     *const char,
        dwFileAttributes: u32
    ): i32

    // ── Directories ───────────────────────────────────────────
    @lib pub fn CreateDirectoryA(
        lpPathName:           *const char,
        lpSecurityAttributes: *void
    ): i32
    @lib pub fn RemoveDirectoryA(lpPathName: *const char): i32
    @lib pub fn GetCurrentDirectoryA(nBufferLength: u32, lpBuffer: *char): u32
    @lib pub fn SetCurrentDirectoryA(lpPathName: *const char): i32
    @lib pub fn GetTempPathA(nBufferLength: u32, lpBuffer: *char): u32
    @lib pub fn GetTempFileNameA(
        lpPathName:    *const char,
        lpPrefixString: *const char,
        uUnique:       u32,
        lpTempFileName: *char
    ): u32

    // ── System / environment ──────────────────────────────────
    @lib pub fn GetCommandLineA(): *const char
    @lib pub fn GetEnvironmentVariableA(
        lpName:   *const char,
        lpBuffer: *char,
        nSize:    u32
    ): u32
    @lib pub fn SetEnvironmentVariableA(
        lpName:  *const char,
        lpValue: *const char
    ): i32
    @lib pub fn ExpandEnvironmentStringsA(
        lpSrc:    *const char,
        lpDst:    *char,
        nSize:    u32
    ): u32
    @lib pub fn GetComputerNameA(lpBuffer: *char, nSize: *u32): i32
    @lib pub fn GetUserNameA(lpBuffer: *char, pcbBuffer: *u32): i32

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
        hStdInput:       *HANDLE
        hStdOutput:      *HANDLE
        hStdError:       *HANDLE
    }

    /// `PROCESS_INFORMATION` — 24 bytes.
    /// CloseHandle both `hProcess` and `hThread` when done.
    pub struct PROCESS_INFORMATION {
        hProcess:    *HANDLE
        hThread:     *HANDLE
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

    @lib pub fn OpenProcess(
        dwDesiredAccess: u32,
        bInheritHandle:  i32,
        dwProcessId:     u32
    ): *HANDLE

    @lib pub fn TerminateProcess(hProcess: *HANDLE, uExitCode: u32): i32

    @lib pub fn GetExitCodeProcess(hProcess: *HANDLE, lpExitCode: *u32): i32

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
        // szCSDVersion[128] as 32 × u32
        _v0: u32
        _v1: u32
        _v2: u32
        _v3: u32
        _v4: u32
        _v5: u32
        _v6: u32
        _v7: u32
        _v8: u32
        _v9: u32
        _va: u32
        _vb: u32
        _vc: u32
        _vd: u32
        _ve: u32
        _vf: u32
        _vg: u32
        _vh: u32
        _vi: u32
        _vj: u32
        _vk: u32
        _vl: u32
        _vm: u32
        _vn: u32
        _vo: u32
        _vp: u32
        _vq: u32
        _vr: u32
        _vs: u32
        _vt: u32
        _vu: u32
        _vv: u32
    }

    /// Deprecated on Windows 8.1+; returns 6.2 for later versions
    /// unless the app has a compatibility manifest.  Use
    /// `VerifyVersionInfoA` or `RtlGetVersion` (ntdll) for accurate
    /// version detection.
    @lib pub fn GetVersionExA(lpVersionInformation: *OSVERSIONINFOA): i32

    // ── Path utilities ────────────────────────────────────────────

    @lib pub fn GetModuleFileNameA(
        hModule:  *HMODULE,
        lpFilename: *char,
        nSize:    u32
    ): u32

    @lib pub fn GetFullPathNameA(
        lpFileName:   *const char,
        nBufferLength: u32,
        lpBuffer:     *char,
        lpFilePart:   **char
    ): u32

    @lib pub fn GetShortPathNameA(
        lpszLongPath:  *const char,
        lpszShortPath: *char,
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

    // ── Pipe / anonymous IPC ──────────────────────────────────────

    @lib pub fn CreatePipe(
        hReadPipe:        **HANDLE,
        hWritePipe:       **HANDLE,
        lpPipeAttributes: *void,
        nSize:            u32
    ): i32

    @lib pub fn ConnectNamedPipe(hNamedPipe: *HANDLE, lpOverlapped: *void): i32

    @lib pub fn CreateNamedPipeA(
        lpName:               *const char,
        dwOpenMode:           u32,
        dwPipeMode:           u32,
        nMaxInstances:        u32,
        nOutBufferSize:       u32,
        nInBufferSize:        u32,
        nDefaultTimeOut:      u32,
        lpSecurityAttributes: *void
    ): *HANDLE

    // ── Console extended ──────────────────────────────────────────

    @lib pub fn ReadConsoleA(
        hConsoleInput:          *HANDLE,
        lpBuffer:               *void,
        nNumberOfCharsToRead:   u32,
        lpNumberOfCharsRead:    *u32,
        pInputControl:          *void
    ): i32

    @lib pub fn SetConsoleTextAttribute(
        hConsoleOutput: *HANDLE,
        wAttributes:    u16
    ): i32

    @lib pub fn GetConsoleScreenBufferInfo(
        hConsoleOutput:       *HANDLE,
        lpConsoleScreenBufferInfo: *void
    ): i32
}
