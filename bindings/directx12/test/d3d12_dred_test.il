/// d3d12 — DRED (Device-Removed Extended Data) setup smoke test.
///
/// Pre-device-creation flow:
///   1. D3D12GetDebugInterface(IID_ID3D12DeviceRemovedExtendedDataSettings)
///   2. Set the three enablement toggles (auto-breadcrumbs / page-fault /
///      Watson dump).
///   3. Create a device; DRED bookkeeping is now active.
///   4. QueryInterface ID3D12DeviceRemovedExtendedData off the device.
///      On a healthy (non-removed) device, the auto-breadcrumb /
///      page-fault accessors return DXGI_ERROR_NOT_FOUND — accept any
///      negative HRESULT.
///
/// Bails cleanly when DRED isn't installed (Graphics Tools missing).

use test
use windows { HANDLE, NULL }
use directx12 {
    D3D12GetDebugInterface, D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12DeviceRemovedExtendedDataSettings,
    IID_ID3D12DeviceRemovedExtendedData,
    IUnknown,
    ID3D12Device,
    ID3D12DeviceRemovedExtendedDataSettings,
    ID3D12DeviceRemovedExtendedData,
    D3D12DredEnablement,
}

@extern(C) {
    pub fn run() {
        let rawSettings: *void = NULL
        let shr = D3D12GetDebugInterface(
            &IID_ID3D12DeviceRemovedExtendedDataSettings,
            &rawSettings
        )
        if shr != 0 {
            // No Graphics Tools / DRED unavailable.
            return
        }
        test.expectTrue((rawSettings as i64) != 0)
        let settings = rawSettings as ID3D12DeviceRemovedExtendedDataSettings

        // All three knobs.  forcedOn = capture even when the env var
        // isn't set; the runtime accepts repeat calls.
        settings.SetAutoBreadcrumbsEnablement(D3D12DredEnablement.forcedOn as u32)
        settings.SetPageFaultEnablement(D3D12DredEnablement.forcedOn as u32)
        settings.SetWatsonDumpEnablement(D3D12DredEnablement.forcedOn as u32)

        // ── Create a device under DRED ─────────────────────────────
        let rawDev: *void = NULL
        let drc = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device,
            &rawDev
        )
        if drc != 0 {
            test.expect(settings.Release() as i64, 0)
            return
        }
        let device = rawDev as ID3D12Device

        // ── QueryInterface DRED-data accessor ─────────────────────
        // Available off ID3D12Device via IUnknown.QueryInterface.
        let rawData: *void = NULL
        let qhr = (device as *void as IUnknown).QueryInterface(
            &IID_ID3D12DeviceRemovedExtendedData,
            &rawData
        )
        if qhr == 0 {
            test.expectTrue((rawData as i64) != 0)
            let data = rawData as ID3D12DeviceRemovedExtendedData
            // Auto-breadcrumb / page-fault payloads are only populated
            // on a removed device.  GetAutoBreadcrumbsOutput here
            // returns DXGI_ERROR_NOT_FOUND (-2005270526).  Accept any
            // non-positive HRESULT.
            let dummy: i64 = 0
            let abhr = data.GetAutoBreadcrumbsOutput(&dummy as *void)
            test.expectTrue(abhr <= 0)
            let pfhr = data.GetPageFaultAllocationOutput(&dummy as *void)
            test.expectTrue(pfhr <= 0)
            // QueryInterface returned an alternate vtable on the
            // same underlying object, so Release here drops 2 → 1
            // (the device still owns the other ref). Accept any
            // non-negative result.
            test.expectTrue(data.Release() <= 1 as u32)
        }

        test.expect(device.Release()   as i64, 0)
        test.expect(settings.Release() as i64, 0)
    }
}

run()
