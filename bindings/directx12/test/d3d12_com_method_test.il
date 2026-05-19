// d3d12 — call ID3D12Device::GetNodeCount through the COM vtable.
//
// ID3D12Device inherits IUnknown (3 methods) + ID3D12Object (5 methods)
// + ID3D12DeviceChild (0 methods, despite the name) and then adds its
// own methods. The first ID3D12Device-specific method is GetNodeCount,
// at vtable offset:
//
//   IUnknown       (3 fns × 8 bytes) =  0..23   (QI / AddRef / Release)
//   ID3D12Object   (4 fns × 8 bytes) = 24..55   (GetPrivateData / Set
//                                                / SetPrivateDataInterface
//                                                / SetName)
//   ID3D12Device::GetNodeCount @ offset 56     (this)→ u32
//
// (ID3D12Object adds 4 methods, not 5: the SDK header counts the
//  vtable slots, and there are 4 between IUnknown.Release and
//  ID3D12Device.GetNodeCount.)

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    GUID,
}

@extern(C) {
    pub fn run() {
        let iid = GUID {
            Data1:   0x189819f1,
            Data2:   0x1db6,
            Data3:   0x4b57,
            Data4lo: 0x211854be,
            Data4hi: 0xf7859b33,
        }
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            iid,
            &device
        )
        if rc != 0 {
            return
        }

        let vtbl = readU64(device as *const void, 0 as i64)

        // ID3D12Device::GetNodeCount — vtable offset +56.
        let getNodeCountPtr = readU64(vtbl as i64 as *const void, 56 as i64)
        let getNodeCount = getNodeCountPtr as i64 as *void as fn(i64): u32
        let n = getNodeCount(device as i64)
        // Single-GPU systems report 1 node; multi-GPU systems report ≥1.
        test.expectTrue(n >= 1)
        test.expectTrue(n <= 64)

        // Cleanup via Release (offset +16 in the vtable).
        let releasePtr = readU64(vtbl as i64 as *const void, 16 as i64)
        let release = releasePtr as i64 as *void as fn(i64): u32
        let _ = release(device as i64)
    }
}

run()
