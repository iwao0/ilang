/// psapi.dll — Process Status API: enumerate processes, modules,
/// drivers, and query working-set / memory usage.
///
/// On Windows 7+ these functions are re-exported by kernel32.dll as
/// the K32* variants (e.g. K32EnumProcesses). The psapi.dll exports
/// remain valid and are the simpler binding target.

// ─── EnumProcessModulesEx filter flags (LIST_MODULES_*) ──────────
@flags
pub enum ListModulesFilter: u32 {
    default = 0      // platform default (LIST_MODULES_DEFAULT)
    bit32   = 0x01   // include 32-bit modules
    bit64   = 0x02   // include 64-bit modules
    all     = 0x03   // bit32 | bit64
}

use kernel32 { HMODULE, HANDLE }

@extern(C, "psapi") {
    // ── MODULEINFO — 24 bytes on x64 ─────────────────────────────

    pub struct MODULEINFO {
        lpBaseOfDll: *void   // base address of the loaded image
        SizeOfImage: u32     // size in bytes
        // 4 bytes implicit padding before EntryPoint
        EntryPoint:  *void
    }

    // ── PROCESS_MEMORY_COUNTERS — 72 bytes on x64 ────────────────

    /// Filled by `GetProcessMemoryInfo`. Set `cb` = 72 before calling.
    pub struct PROCESS_MEMORY_COUNTERS {
        cb:                          u32
        PageFaultCount:              u32
        PeakWorkingSetSize:          u64    // SIZE_T (bytes)
        WorkingSetSize:              u64
        QuotaPeakPagedPoolUsage:     u64
        QuotaPagedPoolUsage:         u64
        QuotaPeakNonPagedPoolUsage:  u64
        QuotaNonPagedPoolUsage:      u64
        PagefileUsage:               u64
        PeakPagefileUsage:           u64
    }

    /// Extended form with PrivateUsage. Set `cb` = 80.
    pub struct PROCESS_MEMORY_COUNTERS_EX {
        cb:                          u32
        PageFaultCount:              u32
        PeakWorkingSetSize:          u64
        WorkingSetSize:              u64
        QuotaPeakPagedPoolUsage:     u64
        QuotaPagedPoolUsage:         u64
        QuotaPeakNonPagedPoolUsage:  u64
        QuotaNonPagedPoolUsage:      u64
        PagefileUsage:               u64
        PeakPagefileUsage:           u64
        PrivateUsage:                u64    // commit charge
    }

    // ── PERFORMANCE_INFORMATION — 88 bytes on x64 ────────────────

    pub struct PERFORMANCE_INFORMATION {
        cb:                u32
        // 4 bytes implicit padding
        CommitTotal:       u64
        CommitLimit:       u64
        CommitPeak:        u64
        PhysicalTotal:     u64
        PhysicalAvailable: u64
        SystemCache:       u64
        KernelTotal:       u64
        KernelPaged:       u64
        KernelNonpaged:    u64
        PageSize:          u64
        HandleCount:       u32
        ProcessCount:      u32
        ThreadCount:       u32
        _pad:              u32
    }

    // ── Process enumeration ───────────────────────────────────────

    /// Fills `lpidProcess` with up to `cb`/4 process IDs.
    /// `lpcbNeeded` receives the actual byte count used.
    /// If `*lpcbNeeded == cb`, the buffer was too small — retry with a larger one.
    @lib pub fn EnumProcesses(
        lpidProcess: *u32,
        cb:          u32,
        lpcbNeeded:  *u32
    ): i32

    /// Returns the path to the executable image of the process.
    @lib pub fn GetProcessImageFileNameA(
        hProcess: HANDLE,   // PROCESS_QUERY_LIMITED_INFORMATION
        lpImageFileName: *char,
        nSize: u32
    ): u32

    // ── Module enumeration ────────────────────────────────────────

    /// Enumerates loaded modules in `hProcess`.
    /// `lphModule`: receives HMODULE values; `cb` is buffer byte size;
    /// `lpcbNeeded`: receives required byte count.
    @lib pub fn EnumProcessModules(
        hProcess:   HANDLE,
        lphModule:  *HMODULE,
        cb:         u32,
        lpcbNeeded: *u32
    ): i32

    /// Like EnumProcessModules but with explicit bitness filter.
    @lib pub fn EnumProcessModulesEx(
        hProcess:   HANDLE,
        lphModule:  *HMODULE,
        cb:         u32,
        lpcbNeeded: *u32,
        dwFilterFlag: u32      // ListModulesFilter.*
    ): i32

    @lib pub fn GetModuleBaseNameA(
        hProcess:   HANDLE,
        hModule:    HMODULE,
        lpBaseName: *char,
        nSize:      u32
    ): u32

    @lib pub fn GetModuleFileNameExA(
        hProcess:   HANDLE,
        hModule:    HMODULE,
        lpFilename: *char,
        nSize:      u32
    ): u32

    @lib pub fn GetModuleInformation(
        hProcess: HANDLE,
        hModule:  HMODULE,
        lpmodinfo: *MODULEINFO,
        cb:       u32
    ): i32

    // ── Memory info ───────────────────────────────────────────────

    /// Fills `ppsmemCounters`. `cb` = sizeof the chosen struct.
    /// For PROCESS_MEMORY_COUNTERS_EX, cast the pointer:
    /// `GetProcessMemoryInfo(hp, &pmcEx as *PROCESS_MEMORY_COUNTERS, 80)`.
    @lib pub fn GetProcessMemoryInfo(
        Process:        HANDLE,
        ppsmemCounters: *PROCESS_MEMORY_COUNTERS,
        cb:             u32
    ): i32

    /// Fills `pPerformanceInformation` with system-wide stats.
    @lib pub fn GetPerformanceInfo(
        pPerformanceInformation: *PERFORMANCE_INFORMATION,
        cb: u32
    ): i32

    /// Returns the file name of the executable that owns the mapped
    /// view at `lpv` in `hProcess`.
    @lib pub fn GetMappedFileNameA(
        hProcess: HANDLE,
        lpv:      *void,
        lpFilename: *char,
        nSize:    u32
    ): u32

    // ── Working set ───────────────────────────────────────────────

    /// Empties the working set, forcing pages to swap.  Useful for
    /// measuring true working-set growth on subsequent allocations.
    @lib pub fn EmptyWorkingSet(hProcess: HANDLE): i32

    // ── Device drivers ────────────────────────────────────────────

    /// Lists loaded kernel-mode drivers. Requires admin privileges
    /// on some systems.
    @lib pub fn EnumDeviceDrivers(
        lpImageBase: *void,
        cb:          u32,
        lpcbNeeded:  *u32
    ): i32

    @lib pub fn GetDeviceDriverBaseNameA(
        ImageBase: *void,
        lpFilename: *char,
        nSize:    u32
    ): u32

    @lib pub fn GetDeviceDriverFileNameA(
        ImageBase: *void,
        lpFilename: *char,
        nSize:    u32
    ): u32
}
