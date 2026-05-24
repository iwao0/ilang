/// dxgi — bring up the modern DXGI factory chain and read adapter info.
///
/// Exercises:
///   * CreateDXGIFactory2 → IDXGIFactory6
///   * IDXGIFactory6::EnumAdapterByGpuPreference (the modern adapter
///     enumeration entry that honours the user's GPU preference list)
///   * IDXGIAdapter1::GetDesc1 → DXGI_ADAPTER_DESC1 (Flags field)
///
/// Bails cleanly on machines without DXGI 1.6 support: each
/// HRESULT-returning call exits the test on the first non-zero
/// status so CI without a discrete GPU still runs to completion.

use test
use windows {
    HANDLE,
    NULL,
}
use directx12 { CreateDXGIFactory2, DXGI_ADAPTER_DESC1, DxgiAdapterFlag, DxgiGpuPreference, IDXGIAdapter1, IDXGIFactory6 }
use winnt { GUID }
use ole32 { IUnknown }
// IID_IDXGIFactory6 = c1b6694f-ff09-44a9-b03c-77900a0a1d17
// IID_IDXGIAdapter1 = 29038f61-3839-4626-91fd-086879011a05
//
// Inlined here (rather than re-exported from the windows umbrella)
// to keep the windows test crate's dep list short — these are pure
// GUID literals.

pub const IID_IDXGIFactory6: GUID = GUID {
    Data1: 0xc1b6694f, Data2: 0xff09, Data3: 0x44a9,
    Data4lo: 0x90773cb0, Data4hi: 0x171d0a0a,
}
pub const IID_IDXGIAdapter1: GUID = GUID {
    Data1: 0x29038f61, Data2: 0x3839, Data3: 0x4626,
    Data4lo: 0x6808fd91, Data4hi: 0x051a0179,
}

@extern(C) {
    pub fn run() {
        let rawFactory: *void = NULL
        let hr = CreateDXGIFactory2(
            0,
            &IID_IDXGIFactory6 as *const void,
            &rawFactory
        )
        if hr != 0 {
            return    // factory6 not available on this host
        }
        let factory = rawFactory as IDXGIFactory6

        // Enumerate the highest-performance adapter. If even the
        // first slot isn't there we have nothing to exercise.
        let rawAdapter: *void = NULL
        let aHr = factory.EnumAdapterByGpuPreference(
            0 as u32,
            DxgiGpuPreference.highPerformance as u32,
            &IID_IDXGIAdapter1 as *const void,
            &rawAdapter
        )
        if aHr != 0 {
            test.expect(factory.Release() as i64, 0)
            return
        }
        let adapter = rawAdapter as IDXGIAdapter1

        // Read the adapter description; both VendorId and DeviceId
        // must be non-zero on any real GPU (including WARP).
        let desc = DXGI_ADAPTER_DESC1 {
            Description: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0
            ],
            VendorId: 0, DeviceId: 0, SubSysId: 0, Revision: 0,
            DedicatedVideoMemory: 0, DedicatedSystemMemory: 0,
            SharedSystemMemory: 0,
            AdapterLuid_lo: 0, AdapterLuid_hi: 0,
            Flags: 0, _pad0: 0,
        }
        let dHr = adapter.GetDesc1(&desc)
        test.expect(dHr as i64, 0)
        test.expectTrue(desc.VendorId != 0 as u32)
        // Flags must be a recognised bit subset (software / remote
        // are the only standard flags; 0 is a real hardware GPU).
        test.expectTrue(desc.Flags <= DxgiAdapterFlag.software as u32 + DxgiAdapterFlag.remote as u32)

        // Release through the dxgi-local IUnknown by re-casting via
        // *void — IDXGIAdapter1's vtable inherits from the IUnknown
        // declared inside `dxgi.il`'s @extern block, which differs in
        // identity from the umbrella `windows.IUnknown` import here.
        test.expect((adapter as *void as IUnknown).Release() as i64, 0)
        test.expect(factory.Release() as i64, 0)
    }
}

run()
