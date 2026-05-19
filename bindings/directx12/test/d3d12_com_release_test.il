// d3d12 — call ID3D12Device's IUnknown methods via the COM vtable.
//
// Uses the vtable-offset enums in `d3d12_vtables.il` so AddRef /
// Release lookups read as `IUnknownVtbl.AddRef as i64` /
// `IUnknownVtbl.Release as i64` instead of bare `+8` / `+16`.

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IUnknownVtbl,
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
        // Bail cleanly on machines without D3D12.
        if rc != 0 {
            return
        }
        test.expectTrue((device as i64) != 0)

        // Vtable pointer is the first 8 bytes of any COM object.
        let vtbl = readU64(device as *const void, 0 as i64)
        test.expectTrue(vtbl != 0 as u64)

        // AddRef — bump refcount 1 → 2.
        let addRefPtr = readU64(
            vtbl as i64 as *const void,
            IUnknownVtbl.AddRef as i64
        )
        let addRef = addRefPtr as i64 as *void as fn(i64): u32
        test.expect(addRef(device as i64) as i64, 2)

        // Release — drop 2 → 1 → 0.
        let releasePtr = readU64(
            vtbl as i64 as *const void,
            IUnknownVtbl.Release as i64
        )
        let release = releasePtr as i64 as *void as fn(i64): u32
        test.expect(release(device as i64) as i64, 1)
        test.expect(release(device as i64) as i64, 0)
    }
}

run()
