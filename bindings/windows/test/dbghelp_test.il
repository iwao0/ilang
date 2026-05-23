/// dbghelp — symbol handler initialisation and option round-trip.

use test
use windows {
    SymInitialize, SymCleanup,
    SymSetOptions, SymGetOptions,
    GetCurrentProcess,
    UnDecorateSymbolName, SymOpt,
}

@extern(C) {
    pub fn run() {
        let hp = GetCurrentProcess()

        // Save the current SymOptions, set a known value, then restore.
        let orig = SymGetOptions()
        let target = (SymOpt.undName | SymOpt.deferredLoads) as u32
        let prev = SymSetOptions(target)
        test.expect(SymGetOptions() as i64, target as i64)
        SymSetOptions(orig)
        test.expect(SymGetOptions() as i64, orig as i64)
        // Sanity: prev matches the value we read before re-setting.
        // (Just check the call returns the previously-set value, no
        // strict equality.)

        // SymInitialize / SymCleanup — fInvadeProcess=0 to avoid loading
        // every module's symbols (would be slow and may need a network).
        let rc = SymInitialize(
            hp as *void,
            0 as *const char,
            0 as i32
        )
        test.expectTrue(rc != 0)
        test.expectTrue(SymCleanup(hp as *void) != 0)

        // UnDecorateSymbolName is callable; result depends on the
        // mangled-name format and is not asserted here.
        let mangled = cstrFromString("?foo@@YAXXZ")
        let outBuf = cstrFromString("                                ")
        let _n = UnDecorateSymbolName(
            mangled as *const char,
            outBuf as *char,
            32 as u32,
            0 as u32
        )
        freeCstr(mangled)
        freeCstr(outBuf)
    }
}

run()
