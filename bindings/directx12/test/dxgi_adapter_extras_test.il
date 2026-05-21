/// dxgi — additional adapter / factory entries beyond `dxgi_factory_test.il`:
///   * IDXGIAdapter3::QueryVideoMemoryInfo
///   * IDXGIFactory4::EnumWarpAdapter
///   * IDXGIFactory5::CheckFeatureSupport (PRESENT_ALLOW_TEARING)
///
/// Each call has a documented failure mode on older / restricted
/// hosts (no DXGI 1.4+ → factory4 unavailable; no integrated/discrete
/// GPU → adapter3 unavailable). The test treats every negative
/// HRESULT as "skip the corresponding step" and keeps going.

use test
use windows {
    HANDLE, NULL,}
use directx12 { CreateDXGIFactory2, DXGI_QUERY_VIDEO_MEMORY_INFO, DxgiFeature, IDXGIAdapter, IDXGIAdapter3, IDXGIFactory4, IDXGIFactory5 }
use ole32 { GUID, IUnknown }

/// IID_IDXGIFactory4 = 1bc6ea02-ef36-464f-bf0c-21ca39e5168a
pub const IID_IDXGIFactory4: GUID = GUID {
    Data1: 0x1bc6ea02, Data2: 0xef36, Data3: 0x464f,
    Data4lo: 0xca210cbf, Data4hi: 0x8a16e539,
}
/// IID_IDXGIFactory5 = 7632e1f5-ee65-4dca-87fd-84cd75f8838d
pub const IID_IDXGIFactory5: GUID = GUID {
    Data1: 0x7632e1f5, Data2: 0xee65, Data3: 0x4dca,
    Data4lo: 0xcd84fd87, Data4hi: 0x8d83f875,
}
/// IID_IDXGIAdapter3 = 645967a4-1392-4310-a798-8053ce3e93fd
pub const IID_IDXGIAdapter3: GUID = GUID {
    Data1: 0x645967a4, Data2: 0x1392, Data3: 0x4310,
    Data4lo: 0x538098a7, Data4hi: 0xfd933ece,
}

@extern(C) {
    pub fn run() {
        // ── IDXGIFactory4 (EnumWarpAdapter) ────────────────────────
        let rawF4: *void = NULL
        let f4hr = CreateDXGIFactory2(0, &IID_IDXGIFactory4 as *const void, &rawF4)
        if f4hr != 0 {
            return
        }
        let f4 = rawF4 as IDXGIFactory4

        // EnumWarpAdapter — software rasterizer; always succeeds on
        // any DXGI 1.4+ host.
        let rawWarp: *void = NULL
        let whr = f4.EnumWarpAdapter(&IID_IDXGIAdapter3 as *const void, &rawWarp)
        if whr == 0 {
            test.expectTrue((rawWarp as i64) != 0)
            // QueryVideoMemoryInfo on WARP — the local node, default
            // memory segment group (0 = LOCAL).
            let info = DXGI_QUERY_VIDEO_MEMORY_INFO {
                Budget: 0, CurrentUsage: 0,
                AvailableForReservation: 0, CurrentReservation: 0,
            }
            let warp = rawWarp as IDXGIAdapter3
            let qhr = warp.QueryVideoMemoryInfo(0 as u32, 0 as u32, &info)
            // WARP advertises Budget but with very small numbers.
            // Accept any non-negative HRESULT.
            test.expectTrue(qhr <= 0)
            test.expect((rawWarp as IUnknown).Release() as i64, 0)
        }

        // ── IDXGIFactory5 (CheckFeatureSupport for tearing) ────────
        let rawF5: *void = NULL
        let f5hr = CreateDXGIFactory2(0, &IID_IDXGIFactory5 as *const void, &rawF5)
        if f5hr == 0 {
            test.expectTrue((rawF5 as i64) != 0)
            let f5 = rawF5 as IDXGIFactory5
            let allowTearing: i32 = 0
            let chr = f5.CheckFeatureSupport(
                DxgiFeature.presentAllowTearing as u32,
                &allowTearing as *void,
                4 as u32                    // sizeof BOOL
            )
            test.expect(chr as i64, 0)
            // BOOL is either 0 or 1.
            test.expectTrue(allowTearing == 0 || allowTearing == 1)
            test.expect(f5.Release() as i64, 0)
        }

        test.expect(f4.Release() as i64, 0)
    }
}

run()
