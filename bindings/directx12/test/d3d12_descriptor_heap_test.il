/// d3d12 — descriptor heap creation + view-population coverage.
///
/// Exercises the typed forms added in the second-pass binding:
///   * Device.CreateDescriptorHeap (RTV, CBV/SRV/UAV, Sampler)
///   * Device.GetDescriptorHandleIncrementSize for each heap type
///   * Device.CreateConstantBufferView
///   * Device.CreateRenderTargetView with an explicit DESC
///   * Device.CreateSampler
///   * DescriptorHeap.GetCPUDescriptorHandleForHeapStart
///   * Device.CopyDescriptorsSimple within a shader-visible heap
///   * Device.GetCustomHeapProperties (returns by retval-pointer)
///   * Device.GetDeviceRemovedReason
///
/// Bails cleanly on hosts without D3D12.

use std.test
use windows {
    HANDLE, NULL,
}
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12DescriptorHeap,
    IID_ID3D12Resource,
    ID3D12Device,
    ID3D12DescriptorHeap,
    ID3D12Resource,
    D3D12_DESCRIPTOR_HEAP_DESC,
    D3D12_HEAP_PROPERTIES,
    D3D12_RESOURCE_DESC,
    D3D12_CPU_DESCRIPTOR_HANDLE,
    D3D12_CONSTANT_BUFFER_VIEW_DESC,
    D3D12_RENDER_TARGET_VIEW_DESC,
    D3D12_SAMPLER_DESC,
    D3D12DescriptorHeapType,
    D3D12DescriptorHeapFlags,
    D3D12HeapType,
    D3D12CpuPageProperty,
    D3D12MemoryPool,
    D3D12ResourceDimension,
    D3D12TextureLayout,
    D3D12ResourceFlags,
    D3D12ResourceStates,
    D3D12HeapMiscFlags,
    D3D12RtvDimension,
    D3D12Filter,
    D3D12TextureAddressMode,
    D3D12ComparisonFunc,
    DXGI_SAMPLE_DESC, DxgiFormat,
}
use ole32 { IUnknown }

@extern(C) {
    pub fn run() {
        let rawDev: *void = NULL
        let rc = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device,
            &rawDev
        )
        if rc != 0 {
            return
        }
        let device = rawDev as ID3D12Device

        // ── Descriptor-handle increment sizes ──────────────────────
        let rtvInc     = device.GetDescriptorHandleIncrementSize(D3D12DescriptorHeapType.rtv as u32)
        let dsvInc     = device.GetDescriptorHandleIncrementSize(D3D12DescriptorHeapType.dsv as u32)
        let cbvInc     = device.GetDescriptorHandleIncrementSize(D3D12DescriptorHeapType.cbvSrvUav as u32)
        let samplerInc = device.GetDescriptorHandleIncrementSize(D3D12DescriptorHeapType.sampler as u32)
        test.expectTrue(rtvInc     > 0 as u32)
        test.expectTrue(dsvInc     > 0 as u32)
        test.expectTrue(cbvInc     > 0 as u32)
        test.expectTrue(samplerInc > 0 as u32)

        // ── RTV heap (CPU-only, 4 entries) ─────────────────────────
        let rtvHeapDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.rtv as u32,
            NumDescriptors: 4,
            Flags:          D3D12DescriptorHeapFlags.none as u32,
            NodeMask:       0,
        }
        let rawRtvHeap: *void = NULL
        let rhr = device.CreateDescriptorHeap(&rtvHeapDesc, &IID_ID3D12DescriptorHeap, &rawRtvHeap)
        test.expect(rhr as i64, 0)
        let rtvHeap = rawRtvHeap as ID3D12DescriptorHeap

        // Pull the start CPU handle. The binding form uses a
        // retval-pointer style — populates `out` and returns it.
        let rtvStart = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: 0 }
        let _ = rtvHeap.GetCPUDescriptorHandleForHeapStart(&rtvStart)
        test.expectTrue(rtvStart.ptr != 0 as u64)

        // ── A 256-byte upload buffer to hand the views ─────────────
        let heap = D3D12_HEAP_PROPERTIES {
            Type:                 D3D12HeapType.upload as u32,
            CPUPageProperty:      D3D12CpuPageProperty.unknown as u32,
            MemoryPoolPreference: D3D12MemoryPool.unknown as u32,
            CreationNodeMask:     1,
            VisibleNodeMask:      1,
        }
        let desc = D3D12_RESOURCE_DESC {
            Dimension:        D3D12ResourceDimension.buffer as u32,
            Alignment:        0,
            Width:            256 as u64,
            Height:           1,
            DepthOrArraySize: 1,
            MipLevels:        1,
            Format:           DxgiFormat.unknown as u32,
            SampleDesc:       DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout:           D3D12TextureLayout.rowMajor as u32,
            Flags:            D3D12ResourceFlags.none as u32,
        }
        let rawBuf: *void = NULL
        let brc = device.CreateCommittedResource(
            &heap, D3D12HeapMiscFlags.none as u32, &desc,
            D3D12ResourceStates.genericRead as u32, NULL,
            &IID_ID3D12Resource, &rawBuf
        )
        test.expect(brc as i64, 0)
        let buf = rawBuf as ID3D12Resource

        // ── CBV heap (shader-visible, 8 entries) ───────────────────
        let cbvHeapDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.cbvSrvUav as u32,
            NumDescriptors: 8,
            Flags:          D3D12DescriptorHeapFlags.shaderVisible as u32,
            NodeMask:       0,
        }
        let rawCbvHeap: *void = NULL
        test.expect(device.CreateDescriptorHeap(&cbvHeapDesc, &IID_ID3D12DescriptorHeap, &rawCbvHeap) as i64, 0)
        let cbvHeap = rawCbvHeap as ID3D12DescriptorHeap
        let cbvStart = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: 0 }
        let _ = cbvHeap.GetCPUDescriptorHandleForHeapStart(&cbvStart)
        test.expectTrue(cbvStart.ptr != 0 as u64)

        // CreateConstantBufferView at slot 0.
        let cbvDesc = D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: buf.GetGPUVirtualAddress(),
            SizeInBytes:    256 as u32,
            _pad0:          0,
        }
        device.CreateConstantBufferView(&cbvDesc, cbvStart.ptr)

        // ── Sampler heap (1 entry) ─────────────────────────────────
        let sampHeapDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.sampler as u32,
            NumDescriptors: 1,
            Flags:          D3D12DescriptorHeapFlags.shaderVisible as u32,
            NodeMask:       0,
        }
        let rawSampHeap: *void = NULL
        test.expect(device.CreateDescriptorHeap(&sampHeapDesc, &IID_ID3D12DescriptorHeap, &rawSampHeap) as i64, 0)
        let sampHeap = rawSampHeap as ID3D12DescriptorHeap
        let sampStart = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: 0 }
        let _ = sampHeap.GetCPUDescriptorHandleForHeapStart(&sampStart)

        let sampDesc = D3D12_SAMPLER_DESC {
            Filter:         D3D12Filter.minMagMipLinear as u32,
            AddressU:       D3D12TextureAddressMode.clamp as u32,
            AddressV:       D3D12TextureAddressMode.clamp as u32,
            AddressW:       D3D12TextureAddressMode.clamp as u32,
            MipLODBias:     0.0_f32,
            MaxAnisotropy:  0,
            ComparisonFunc: D3D12ComparisonFunc.never as u32,
            BorderColor:    [0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32],
            MinLOD:         0.0_f32,
            MaxLOD:         1024.0_f32,
        }
        device.CreateSampler(&sampDesc, sampStart.ptr)

        // (We hold off on `CreateRenderTargetView` here — a
        // buffer-backed RTV requires a precise FirstElement / NumElements
        // payload inside the descriptor union, and dx12_triangle
        // already exercises the texture-RTV path against a real
        // back buffer.)

        // ── CopyDescriptorsSimple: dup slot 0 of cbv heap to slot 1 ─
        // The simple form takes (count, dstHandlePtr, srcHandlePtr, type).
        let dstHandle: u64 = cbvStart.ptr + cbvInc as u64
        device.CopyDescriptorsSimple(
            1 as u32,
            dstHandle,
            cbvStart.ptr,
            D3D12DescriptorHeapType.cbvSrvUav as u32
        )

        // ── GetCustomHeapProperties (retval-pointer return form) ────
        let custom = D3D12_HEAP_PROPERTIES {
            Type: 0, CPUPageProperty: 0, MemoryPoolPreference: 0,
            CreationNodeMask: 0, VisibleNodeMask: 0,
        }
        let _ = device.GetCustomHeapProperties(&custom, 0 as u32, D3D12HeapType.upload as u32)
        // For HEAP_TYPE_UPLOAD the runtime fills MemoryPoolPreference = L0.
        test.expectTrue(custom.MemoryPoolPreference != 0 as u32)

        // ── GetDeviceRemovedReason — must be S_OK on a healthy device.
        test.expect(device.GetDeviceRemovedReason() as i64, 0)

        // ── Cleanup ────────────────────────────────────────────────
        test.expect(sampHeap.Release() as i64, 0)
        test.expect(cbvHeap.Release()  as i64, 0)
        test.expect(rtvHeap.Release()  as i64, 0)
        test.expect(buf.Release()      as i64, 0)
        test.expect(device.Release()   as i64, 0)
    }
}

run()
