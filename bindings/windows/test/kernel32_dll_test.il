// kernel32 — runtime DLL loading and symbol lookup.

use test
use windows {
    LoadLibraryA, FreeLibrary, GetProcAddress, GetModuleHandleA,
}

@extern(C) {
    pub fn run() {
        // GetModuleHandleA for an already-loaded DLL.
        let k32 = cstrFromString("kernel32.dll")
        let hk32 = GetModuleHandleA(k32)
        test.expectTrue((hk32 as i64) != 0)
        freeCstr(k32)

        // LoadLibraryA for the same — bumps the refcount.
        let k32b = cstrFromString("kernel32.dll")
        let hk32b = LoadLibraryA(k32b)
        test.expectTrue((hk32b as i64) != 0)
        freeCstr(k32b)

        // GetProcAddress for a known export.
        let symName = cstrFromString("GetTickCount")
        let p = GetProcAddress(hk32b, symName)
        test.expectTrue((p as i64) != 0)
        freeCstr(symName)

        // Unknown symbol returns NULL.
        let bogus = cstrFromString("__not_a_real_export__")
        let pb = GetProcAddress(hk32b, bogus)
        test.expect(pb as i64, 0)
        freeCstr(bogus)

        // FreeLibrary on the LoadLibraryA reference.
        test.expectTrue(FreeLibrary(hk32b) != 0)

        // LoadLibraryA on a missing DLL returns NULL.
        let missing = cstrFromString("__definitely_not_a_real_dll__.dll")
        test.expect(LoadLibraryA(missing) as i64, 0)
        freeCstr(missing)
    }
}

run()
