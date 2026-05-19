// d3d12 — call ID3D12Device::GetNodeCount through the COM vtable.
//
// Uses `ID3D12DeviceVtbl.GetNodeCount as i64` instead of a bare
// integer offset.

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    ID3D12DeviceVtbl, IUnknownVtbl,
}

@extern(C) {
    pub fn run() {
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            IID_ID3D12Device,
            &device
        )
        if rc != 0 {
            return
        }

        let vtbl = readU64(device as *const void, 0 as i64)

        // ID3D12Device::GetNodeCount: (this) → u32
        let getNodeCountPtr = readU64(
            vtbl as i64 as *const void,
            ID3D12DeviceVtbl.GetNodeCount as i64
        )
        let getNodeCount = getNodeCountPtr as i64 as *void as fn(i64): u32
        let n = getNodeCount(device as i64)
        // Single-GPU systems report 1 node; multi-GPU systems report ≥ 1.
        test.expectTrue(n >= 1)
        test.expectTrue(n <= 64)

        // Cleanup via IUnknown::Release.
        let releasePtr = readU64(
            vtbl as i64 as *const void,
            IUnknownVtbl.Release as i64
        )
        let release = releasePtr as i64 as *void as fn(i64): u32
        let _ = release(device as i64)
    }
}

run()
