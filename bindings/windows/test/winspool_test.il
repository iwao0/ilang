/// winspool — GetDefaultPrinter + EnumPrinters smoke test.

use std.test
use windows {
    GetDefaultPrinterA, EnumPrintersA, PrinterEnumFlags,
    GetProcessHeap, HeapAlloc, HeapFree,
}

@extern(C) {
    pub fn run() {
        // GetDefaultPrinter — returns 0 if no default printer is
        // configured, else writes the name. We just need the call
        // to link + return.
        let heap = GetProcessHeap()
        let buf = HeapAlloc(heap, 8 as u32, 512 as u64)
        let cch: u32 = 256
        let _ = GetDefaultPrinterA(buf as *char, &cch)

        // Probe-size query for local printers — pass 0 buffer first
        // to learn the required size.
        let needed: u32 = 0
        let returned: u32 = 0
        let _ = EnumPrintersA(
            PrinterEnumFlags.local_ as u32,
            0 as *const char,
            2 as u32,
            0 as *void,
            0 as u32,
            &needed,
            &returned
        )
        // `needed` becomes a positive size on a host with printers,
        // 0 on locked-down CI. We just verify the call links.

        HeapFree(heap, 0 as u32, buf)
    }
}

run()
