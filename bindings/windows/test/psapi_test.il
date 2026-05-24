/// psapi — process memory counters and image-file-name lookup.

use test
use windows {
    GetCurrentProcess,
    HeapAlloc, HeapFree, GetProcessHeap,
    lstrlenA,
}
use psapi { PROCESS_MEMORY_COUNTERS, GetProcessMemoryInfo, GetProcessImageFileNameA }

@extern(C) {
    pub fn run() {
        let hp = GetCurrentProcess()
        let pmc = PROCESS_MEMORY_COUNTERS {
            cb: 72,
            PageFaultCount: 0,
            PeakWorkingSetSize: 0 as u64,
            WorkingSetSize: 0 as u64,
            QuotaPeakPagedPoolUsage: 0 as u64,
            QuotaPagedPoolUsage: 0 as u64,
            QuotaPeakNonPagedPoolUsage: 0 as u64,
            QuotaNonPagedPoolUsage: 0 as u64,
            PagefileUsage: 0 as u64,
            PeakPagefileUsage: 0 as u64,
        }
        let ok = GetProcessMemoryInfo(hp as *void, &pmc, 72 as u32)
        test.expectTrue(ok != 0)
        test.expectTrue(pmc.WorkingSetSize > 0 as u64)
        test.expectTrue(pmc.PeakWorkingSetSize >= pmc.WorkingSetSize)

        // Image file name for this process.
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 1024 as u64)
        let n = GetProcessImageFileNameA(hp as *void, buf as *char, 1024 as u32)
        test.expectTrue(n > 0)
        test.expect(lstrlenA(buf as *const char) as i64, n as i64)
        HeapFree(heap, 0 as u32, buf)
    }
}

run()
