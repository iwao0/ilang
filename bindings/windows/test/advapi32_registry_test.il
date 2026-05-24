/// advapi32 — registry create / write / read / delete round-trip
/// under HKEY_CURRENT_USER\Software\IlangTest.

use std.test
use windows {
    HKEY, PredefinedKey, KeyAccess, RegValueType, RegOptions,
    RegCreateKeyExA, RegOpenKeyExA, RegCloseKey,
    RegSetValueExA, RegQueryValueExA,
    RegDeleteValueA, RegDeleteKeyA,
}

@extern(C) {
    pub fn run() {
        let hkcu = PredefinedKey.currentUser as i64 as *HKEY

        // Create the test key.
        let path = cstrFromString("Software\\IlangTest")
        let h: *HKEY = 0 as *HKEY
        let disposition: u32 = 0
        let rc = RegCreateKeyExA(
            hkcu,
            path,
            0 as u32,
            0 as *char,
            RegOptions.nonVolatile as u32,
            KeyAccess.allAccess as u32,
            0 as *void,
            &h,
            &disposition
        )
        test.expect(rc as i64, 0)
        test.expectTrue((h as i64) != 0)

        // Write a DWORD value.
        let valName = cstrFromString("TestNumber")
        let value: u32 = 0xDEADBEEF
        let rc2 = RegSetValueExA(
            h,
            valName,
            0 as u32,
            RegValueType.dword as u32,
            &value as *const void,
            4 as u32
        )
        test.expect(rc2 as i64, 0)

        // Read the DWORD back.
        let readValue: u32 = 0
        let valType: u32 = 0
        let dataSize: u32 = 4
        let rc3 = RegQueryValueExA(
            h,
            valName,
            0 as *u32,
            &valType,
            &readValue as *void,
            &dataSize
        )
        test.expect(rc3 as i64, 0)
        test.expect(valType as i64, RegValueType.dword as i64)
        test.expect(readValue as i64, 0xDEADBEEF)

        // Delete the value.
        test.expect(RegDeleteValueA(h, valName) as i64, 0)

        // Reading it back now returns ERROR_FILE_NOT_FOUND (2).
        let rc4 = RegQueryValueExA(
            h,
            valName,
            0 as *u32,
            &valType,
            &readValue as *void,
            &dataSize
        )
        test.expect(rc4 as i64, 2)

        RegCloseKey(h)

        // Re-open with read-only access — should still exist.
        let h2: *HKEY = 0 as *HKEY
        let rc5 = RegOpenKeyExA(hkcu, path, 0 as u32, KeyAccess.read as u32, &h2)
        test.expect(rc5 as i64, 0)
        RegCloseKey(h2)

        // Delete the key. Must use HKCU root (RegDeleteKeyA can't
        // remove keys that have subkeys; we have none here).
        test.expect(RegDeleteKeyA(hkcu, path) as i64, 0)

        // Now reopening fails.
        let h3: *HKEY = 0 as *HKEY
        let rc6 = RegOpenKeyExA(hkcu, path, 0 as u32, KeyAccess.read as u32, &h3)
        test.expect(rc6 as i64, 2)   // ERROR_FILE_NOT_FOUND

        freeCstr(path)
        freeCstr(valName)
    }
}

run()
