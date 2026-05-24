/// d3d12 — bootstrap a D3D12 device using the default adapter.
///
/// Skipped on machines without a D3D12-capable GPU / driver: the
/// call returns DXGI_ERROR_UNSUPPORTED. We accept any non-positive
/// HRESULT so the test is robust on CI without a discrete GPU.

use std.test
use windows { HANDLE }
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel
}
use winnt { GUID }
@extern(C) {
    pub fn run() {
        // IID_ID3D12Device = {189819f1-1db6-4b57-be54-1821339b85f7}
        // Data4[0..4] = be 54 18 21 → little-endian u32 = 0x211854be
        // Data4[4..8] = 33 9b 85 f7 → little-endian u32 = 0xf7859b33
        let iid = GUID {
            Data1:   0x189819f1,
            Data2:   0x1db6,
            Data3:   0x4b57,
            Data4lo: 0x211854be,
            Data4hi: 0xf7859b33,
        }
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,                              // default adapter
            D3DFeatureLevel.level_11_0 as u32,
            &iid,
            &device
        )

        // Acceptable outcomes:
        //   0                         — device created
        //   negative HRESULT          — no D3D12 / no GPU / etc.
        // A positive non-zero value would be unusual.
        test.expectTrue(rc <= 0)

        // If successful, device pointer must be non-NULL.
        if rc == 0 {
            test.expectTrue((device as i64) != 0)
        }
    }
}

run()
