/// d3d12 — exercise `ID3D12Device::CheckFeatureSupport` and
/// `CreateQueryHeap`, both newly-typed in the second-pass binding.
///
/// Bootstraps a device; bails cleanly on machines without a D3D12
/// GPU (same convention as `d3d12_device_test.il`). When a device
/// is available, queries D3D12_OPTIONS to read back tier values
/// and creates a 16-slot timestamp query heap.

use test
use windows { HANDLE }
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12QueryHeap,
    IUnknown,
    ID3D12Device,
    D3D12_FEATURE_DATA_D3D12_OPTIONS,
    D3D12_QUERY_HEAP_DESC,
    D3D12Feature,
    D3D12QueryHeapType,
    D3D12ResourceBindingTier,
    D3D12ResourceHeapTier,
}

@extern(C) {
    pub fn run() {
        let raw: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device,
            &raw
        )
        if rc != 0 {
            return
        }
        let device = raw as ID3D12Device

        // ── CheckFeatureSupport ─────────────────────────────────────
        // D3D12_OPTIONS is the broadest tier-query feature; every
        // D3D12-capable device returns it successfully.
        let opts = D3D12_FEATURE_DATA_D3D12_OPTIONS {
            DoublePrecisionFloatShaderOps:        0,
            OutputMergerLogicOp:                  0,
            MinPrecisionSupport:                  0,
            TiledResourcesTier:                   0,
            ResourceBindingTier:                  0,
            PSSpecifiedStencilRefSupported:       0,
            TypedUAVLoadAdditionalFormats:        0,
            ROVsSupported:                        0,
            ConservativeRasterizationTier:        0,
            MaxGPUVirtualAddressBitsPerResource:  0,
            StandardSwizzle64KBSupported:         0,
            CrossNodeSharingTier:                 0,
            CrossAdapterRowMajorTextureSupported: 0,
            VPAndRTArrayIndexFromAnyShaderFeedingRasterizerSupportedWithoutGSEmulation: 0,
            ResourceHeapTier:                     0,
        }
        let hr = device.CheckFeatureSupport(
            D3D12Feature.d3d12Options as u32,
            &opts as *void,
            60 as u32        // sizeof(D3D12_FEATURE_DATA_D3D12_OPTIONS) = 15 × u32
        )
        test.expect(hr as i64, 0)

        // Tier values must be in the documented ranges (1..=3) for
        // every D3D12-capable adapter.
        test.expectTrue(opts.ResourceBindingTier >= D3D12ResourceBindingTier.tier_1 as u32)
        test.expectTrue(opts.ResourceBindingTier <= D3D12ResourceBindingTier.tier_3 as u32)
        test.expectTrue(opts.ResourceHeapTier    >= D3D12ResourceHeapTier.tier_1 as u32)
        test.expectTrue(opts.ResourceHeapTier    <= D3D12ResourceHeapTier.tier_2 as u32)

        // ── CreateQueryHeap ─────────────────────────────────────────
        let qhDesc = D3D12_QUERY_HEAP_DESC {
            Type:     D3D12QueryHeapType.timestamp as u32,
            Count:    16,
            NodeMask: 0,
            _pad0:    0,
        }
        let rawQh: *void = 0 as *void
        let qhr = device.CreateQueryHeap(&qhDesc, &IID_ID3D12QueryHeap, &rawQh)
        test.expect(qhr as i64, 0)
        test.expectTrue((rawQh as i64) != 0)

        // Release through IUnknown — query heaps don't need a typed
        // interface for tear-down.
        let qh = rawQh as IUnknown
        test.expect(qh.Release() as i64, 0)
        test.expect(device.Release() as i64, 0)
    }
}

run()
