// d3d12 — call ID3D12Device::Release through the COM vtable.
//
// Verifies that the language-level mechanism for raw fn-pointer
// casts (`*void as fn(...)`) is composable with COM dispatch:
//   1. Create a COM object (ID3D12Device) via D3D12CreateDevice.
//   2. Read the first 8 bytes of the object → vtable pointer.
//   3. Read offset 16 of the vtable → IUnknown::Release fn ptr.
//   4. Cast to `fn(i64): u32` and call with `this = device`.
//   5. The released device's refcount drops to 0 on this call
//      (we held the only reference).
//
// IUnknown vtable layout:
//   +0   QueryInterface(this, riid, ppvObject): HRESULT
//   +8   AddRef(this): u32
//   +16  Release(this): u32

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    GUID,
}

@extern(C) {
    pub fn run() {
        // IID_ID3D12Device.
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
        // Bail cleanly on machines without D3D12.
        if rc != 0 {
            return
        }
        test.expectTrue((device as i64) != 0)

        // Step 1: read the vtable pointer from the object's first 8 bytes.
        let vtbl = readU64(device as *const void, 0 as i64)
        test.expectTrue(vtbl != 0 as u64)

        // Step 2: read the AddRef pointer (offset +8) — bump the
        // refcount to 2 so we can prove that path works too.
        let addRefPtr = readU64(vtbl as i64 as *const void, 8 as i64)
        let addRef = addRefPtr as i64 as *void as fn(i64): u32
        let after_addref = addRef(device as i64)
        test.expect(after_addref as i64, 2)

        // Step 3: read the Release pointer (offset +16) and call it
        // twice to drop the refcount from 2 → 0.
        let releasePtr = readU64(vtbl as i64 as *const void, 16 as i64)
        let release = releasePtr as i64 as *void as fn(i64): u32
        let after_first = release(device as i64)
        test.expect(after_first as i64, 1)
        let after_second = release(device as i64)
        test.expect(after_second as i64, 0)
    }
}

run()
