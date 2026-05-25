/// psapi — process / module enumeration.

use std.test as test
use windows {
    OpenProcess, CloseHandle, GetCurrentProcess,
    HeapAlloc, HeapFree, GetProcessHeap,
    lstrlenA,
}
use psapi {
    EnumProcesses,
    EnumProcessModules,
    GetModuleBaseNameA,
    GetModuleFileNameExA,
    GetMappedFileNameA,
}

@extern(C) {
    pub fn run() {
        let heap = GetProcessHeap()

        // EnumProcesses: 4096 bytes can hold 1024 PIDs.
        let pids = HeapAlloc(heap, 8 as u32, 4096 as u64)
        let needed: u32 = 0
        test.expectTrue(EnumProcesses(pids as *u32, 4096 as u32, &needed) != 0)
        test.expectTrue(needed > 0)
        // Must report at least the current process and System (4).
        test.expectTrue(needed >= 8)

        HeapFree(heap, 0 as u32, pids)

        // EnumProcessModules on the current process — first module is
        // the EXE itself.
        let hp = GetCurrentProcess()
        let mods = HeapAlloc(heap, 8 as u32, 1024 as u64)
        let modsNeeded: u32 = 0
        test.expectTrue(EnumProcessModules(
            hp as *void,
            mods,
            1024 as u32,
            &modsNeeded
        ) != 0)
        test.expectTrue(modsNeeded > 0)

        // EnumProcessModules wrote at least one HMODULE (8 bytes on x64).
        test.expectTrue(modsNeeded >= 8)

        HeapFree(heap, 0 as u32, mods)
    }
}

run()
