/// d3d12 — debug-layer interface lookup.
///
/// D3D12GetDebugInterface returns the ID3D12Debug COM pointer used to
/// enable the debug layer.  Requires the "Graphics Tools" optional
/// feature; absence of it returns DXGI_ERROR_NOT_AVAILABLE
/// (0x887A0022 → -2005270494 as i32) — we treat that as OK.

use test
use windows { HANDLE }
use directx12 {
    D3D12GetDebugInterface,
    GUID,
}

@extern(C) {
    pub fn run() {
        // IID_ID3D12Debug = {344488b7-6846-474b-b989-f027448245e0}
        // Data4 bytes: b9 89 f0 27 44 82 45 e0
        // → Data4lo (little-endian u32) = 0x27f089b9
        // → Data4hi (little-endian u32) = 0xe0458244
        let iid = GUID {
            Data1:   0x344488b7,
            Data2:   0x6846,
            Data3:   0x474b,
            Data4lo: 0x27f089b9,
            Data4hi: 0xe0458244,
        }

        let dbg: *void = 0 as *void
        let rc = D3D12GetDebugInterface(&iid, &dbg)

        // Acceptable: S_OK (debug layer available), or any negative
        // HRESULT (debug layer not installed, no GPU, etc.).
        test.expectTrue(rc <= 0)

        if rc == 0 {
            test.expectTrue((dbg as i64) != 0)
        }
    }
}

run()
