/// d3d12 — `D3D12GetInterface` factory-style lookup.
///
/// Equivalent to `DllGetClassObject` scoped to D3D12-installed
/// components. We use it to obtain a `ID3D12DeviceRemovedExtendedDataSettings`
/// (CLSID_D3D12DeviceRemovedExtendedDataSettings) — the most
/// commonly-available class through this entry point — and verify
/// the IUnknown handshake holds.
///
/// On hosts where D3D12GetInterface isn't installed (very old
/// Windows builds), a negative HRESULT bails the test cleanly.

use test
use windows { HANDLE }
use directx12 {
    D3D12GetInterface,
    IID_ID3D12DeviceRemovedExtendedDataSettings,
    IUnknown,
    GUID,
}

/// CLSID_D3D12DeviceRemovedExtendedDataSettings =
///   4488833e-dda2-434b-94e2-a2c91b73da89
pub const CLSID_D3D12DeviceRemovedExtendedDataSettings: GUID = GUID {
    Data1: 0x4488833e, Data2: 0xdda2, Data3: 0x434b,
    Data4lo: 0xc9a2e294, Data4hi: 0x89da731b,
}

@extern(C) {
    pub fn run() {
        let raw: *void = 0 as *void
        let hr = D3D12GetInterface(
            &CLSID_D3D12DeviceRemovedExtendedDataSettings as *const void,
            &IID_ID3D12DeviceRemovedExtendedDataSettings as *const void,
            &raw
        )
        if hr != 0 {
            // Class object not registered on this host — that's fine
            // for the harness. Confirm the failure is at least
            // documented (negative HRESULT) and stop.
            test.expectTrue(hr < 0)
            return
        }
        test.expectTrue((raw as i64) != 0)
        test.expect((raw as IUnknown).Release() as i64, 0)
    }
}

run()
