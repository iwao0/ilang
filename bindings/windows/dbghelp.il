/// dbghelp.dll — debugging helper: mini-dumps, symbol resolution,
/// stack walking, and PE image inspection.
///
/// dbghelp.dll is shipped with Windows but the version bundled with
/// Debugging Tools for Windows (the one in the SDK) is usually
/// newer and more capable.

// ─── MINIDUMP_TYPE flags ─────────────────────────────────────────
@flags
pub enum MiniDumpType: u32 {
    normal                          = 0x00000000
    withDataSegs                    = 0x00000001
    withFullMemory                  = 0x00000002   // huge dumps
    withHandleData                  = 0x00000004
    filterMemory                    = 0x00000008
    scanMemory                      = 0x00000010
    withUnloadedModules             = 0x00000020
    withIndirectlyReferencedMemory  = 0x00000040
    filterModulePaths               = 0x00000080
    withProcessThreadData           = 0x00000100
    withPrivateReadWriteMemory      = 0x00000200
    withoutOptionalData             = 0x00000400
    withFullMemoryInfo              = 0x00000800
    withThreadInfo                  = 0x00001000
    withCodeSegs                    = 0x00002000
    withoutAuxiliaryState           = 0x00004000
    withFullAuxiliaryState          = 0x00008000
    withPrivateWriteCopyMemory      = 0x00010000
    ignoreInaccessibleMemory        = 0x00020000
    withTokenInformation            = 0x00040000
    withModuleHeaders               = 0x00080000
    filterTriage                    = 0x00100000
    withAvxXStateContext            = 0x00200000
    withIptTrace                    = 0x00400000
    scanInaccessiblePartialPages    = 0x00800000
}

// ─── SymSetOptions flags (SYMOPT_*) ──────────────────────────────
@flags
pub enum SymOpt: u32 {
    caseInsensitive  = 0x00000001
    undName          = 0x00000002
    deferredLoads    = 0x00000004
    noCpp            = 0x00000008
    loadLines        = 0x00000010
    omapFindNearest  = 0x00000020
    loadAnything     = 0x00000040
    ignoreCvRec      = 0x00000080
    noUnqualifiedLoads = 0x00000100
    failCriticalErrors = 0x00000200
    exactSymbols     = 0x00000400
    allowAbsoluteSymbols = 0x00000800
    ignoreNtSymPath  = 0x00001000
    includes32Bit    = 0x00002000
    publicsOnly      = 0x00004000
    noPublics        = 0x00008000
    autoPublics      = 0x00010000
    noImageSearch    = 0x00020000
    secureMode       = 0x00040000
    noPrompts        = 0x00080000
    overwrite        = 0x00100000
    ignoreImageDir   = 0x00200000
    flatDirectory    = 0x00400000
    favorCompressed  = 0x00800000
    allowZeroAddress = 0x01000000
    disableSymsrvAutodetect = 0x02000000
    debug            = 0x80000000
}

use kernel32 { HANDLE }

@extern(C, "dbghelp") {
    // ── SYMBOL_INFO — 88 bytes + Name buffer ─────────────────────

    /// Symbol information. Allocate a contiguous buffer of
    /// `sizeof(SYMBOL_INFO) + MAX_SYM_NAME * sizeof(char) - sizeof(char)`
    /// (typically 88 + 2000 = 2088 bytes), set `SizeOfStruct` = 88
    /// and `MaxNameLen` = MAX_SYM_NAME (2000).
    pub struct SYMBOL_INFO {
        SizeOfStruct: u32
        TypeIndex:    u32
        Reserved0:    u64
        Reserved1:    u64
        Index:        u32
        Size:         u32
        ModBase:      u64
        Flags:        u32
        _pad:         u32
        Value:        u64
        Address:      u64
        Register:     u32
        Scope:        u32
        Tag:          u32
        NameLen:      u32
        MaxNameLen:   u32
        _pad2:        u32
        // Name[1] — flexible array; allocate extra bytes after the struct
        Name0:        u8
    }

    // ── IMAGEHLP_LINE64 — 32 bytes ───────────────────────────────

    /// Source-file/line info filled by `SymGetLineFromAddr64`.
    /// `SizeOfStruct` = 32 must be set before the call.
    pub struct IMAGEHLP_LINE64 {
        SizeOfStruct: u32
        // 4 bytes implicit padding
        Key:          *void
        LineNumber:   u32
        _pad:         u32
        FileName:     *char
        Address:      u64
    }

    // ── Symbol management ─────────────────────────────────────────

    /// Initialises the symbol handler for `hProcess`.
    /// `UserSearchPath`: semicolon-separated symbol search path
    /// (e.g. "srv*c:\\symbols*https://msdl.microsoft.com/download/symbols")
    /// or NULL for the default (uses _NT_SYMBOL_PATH env var).
    /// `fInvadeProcess`: 1 = automatically load symbols for every
    /// already-loaded module.
    @lib pub fn SymInitialize(
        hProcess:       HANDLE,
        UserSearchPath: *const char,
        fInvadeProcess: i32
    ): i32

    @lib pub fn SymCleanup(hProcess: HANDLE): i32

    @lib pub fn SymSetOptions(SymOptions: u32): u32
    @lib pub fn SymGetOptions(): u32

    @lib pub fn SymRefreshModuleList(hProcess: HANDLE): i32

    /// Loads symbols for a single module at `BaseOfDll`.
    /// Returns the module base on success, 0 on failure.
    @lib pub fn SymLoadModule64(
        hProcess:    HANDLE,
        hFile:       HANDLE,
        ImageName:   *const char,
        ModuleName:  *const char,
        BaseOfDll:   u64,
        SizeOfDll:   u32
    ): u64

    @lib pub fn SymUnloadModule64(hProcess: HANDLE, BaseOfDll: u64): i32

    // ── Address → symbol ─────────────────────────────────────────

    /// Resolves an address to a symbol.
    /// `Address`: code address to look up.
    /// `Displacement`: receives offset from start of symbol.
    /// `Symbol`: filled SYMBOL_INFO; must have SizeOfStruct/MaxNameLen set.
    @lib pub fn SymFromAddr(
        hProcess:     HANDLE,
        Address:      u64,
        Displacement: *u64,
        Symbol:       *SYMBOL_INFO
    ): i32

    /// Resolves an address to a source file and line.
    @lib pub fn SymGetLineFromAddr64(
        hProcess:        HANDLE,
        qwAddr:          u64,
        pdwDisplacement: *u32,
        Line:            *IMAGEHLP_LINE64
    ): i32

    /// Returns a demangled / undecorated name for a C++ mangled symbol.
    @lib pub fn UnDecorateSymbolName(
        DecoratedName:   *const char,
        UnDecoratedName: *char,
        UndecoratedLength: u32,
        Flags:           u32
    ): u32

    // ── Stack walk ────────────────────────────────────────────────

    /// Walks one frame of a stack.  Set `StackFrame` and `ContextRecord`
    /// to the initial values from a CONTEXT and call repeatedly until
    /// the function returns 0.  `MachineType`: 0x8664 (IMAGE_FILE_MACHINE_AMD64)
    /// on x64.
    @lib pub fn StackWalk64(
        MachineType:    u32,
        hProcess:       HANDLE,
        hThread:        HANDLE,
        StackFrame:     *void,    // *STACKFRAME64 (264 bytes)
        ContextRecord:  *void,    // *CONTEXT
        ReadMemoryRoutine:    *void,
        FunctionTableAccessRoutine: *void,
        GetModuleBaseRoutine: *void,
        TranslateAddress: *void
    ): i32

    /// Standard symbol-handler helpers, suitable as the
    /// `FunctionTableAccessRoutine` / `GetModuleBaseRoutine` arguments
    /// to StackWalk64.
    @lib pub fn SymFunctionTableAccess64(hProcess: HANDLE, AddrBase: u64): *void
    @lib pub fn SymGetModuleBase64(hProcess: HANDLE, qwAddr: u64): u64

    // ── Mini-dump ─────────────────────────────────────────────────

    /// Writes a mini-dump of `hProcess` (typically the calling process
    /// when triggered from a top-level exception filter) to `hFile`.
    /// `ExceptionParam`: pointer to MINIDUMP_EXCEPTION_INFORMATION or NULL.
    /// `DumpType`: MiniDumpType.*.
    @lib pub fn MiniDumpWriteDump(
        hProcess:    HANDLE,
        ProcessId:   u32,
        hFile:       HANDLE,
        DumpType:    u32,
        ExceptionParam:    *const void,
        UserStreamParam:   *const void,
        CallbackParam:     *const void
    ): i32

    // ── PE image utilities ────────────────────────────────────────

    /// Returns the IMAGE_NT_HEADERS for a mapped PE image.
    @lib pub fn ImageNtHeader(Base: *void): *void

    /// Searches `Path` for `FileName`, optionally with `Extension`.
    @lib pub fn SearchTreeForFile(
        RootPath:     *const char,
        InputPathName: *const char,
        OutputPathBuffer: *char
    ): i32
}
