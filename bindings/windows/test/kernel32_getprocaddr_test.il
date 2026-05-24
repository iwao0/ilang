/// kernel32 — call functions dynamically via GetProcAddress.
///
/// Demonstrates the *void → fn(...) cast that lets dlsym /
/// GetProcAddress results be invoked through their declared signature.
/// The MIR lowering tags the resulting value as `MirTy::RawFn` so the
/// call site uses `Inst::CallRawIndirect` (no closure dispatch, no env
/// arg) instead of the normal closure path.

use std.test
use windows {
    LoadLibraryA, GetProcAddress, FreeLibrary,
    GetCurrentProcessId, GetTickCount,
}

@extern(C) {
    pub fn run() {
        let kernel32 = cstrFromString("kernel32.dll")
        let h = LoadLibraryA(kernel32)
        test.expectTrue((h as i64) != 0)
        freeCstr(kernel32)

        // GetTickCount: no args, returns u32. Compare dynamic and
        // direct calls — they should report nearly the same time.
        let nameTC = cstrFromString("GetTickCount")
        let pTC = GetProcAddress(h, nameTC)
        test.expectTrue((pTC as i64) != 0)
        freeCstr(nameTC)

        let dynGetTickCount = pTC as fn(): u32
        let directBefore = GetTickCount()
        let dyn1 = dynGetTickCount()
        let directAfter = GetTickCount()
        test.expectTrue(dyn1 >= directBefore)
        test.expectTrue(dyn1 <= directAfter)

        // GetCurrentProcessId: no args, returns u32. Compare with the
        // direct call — must match exactly (process id is stable).
        let nameP = cstrFromString("GetCurrentProcessId")
        let pP = GetProcAddress(h, nameP)
        test.expectTrue((pP as i64) != 0)
        freeCstr(nameP)

        let dynGetPid = pP as fn(): u32
        test.expect(dynGetPid() as i64, GetCurrentProcessId() as i64)

        // Two casts of the same pointer give the same fn behaviour.
        let dynAgain = pP as fn(): u32
        test.expect(dynAgain() as i64, dynGetPid() as i64)

        FreeLibrary(h)
    }
}

run()
