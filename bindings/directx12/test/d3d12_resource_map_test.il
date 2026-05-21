/// d3d12 — buffer creation + Map / Unmap + GetGPUVirtualAddress.
///
/// Allocates a 256-byte upload buffer, maps it, writes a pattern,
/// unmaps it, and checks that GetGPUVirtualAddress returns
/// something non-zero. Bails cleanly when no D3D12 GPU.

use test
use windows {
    HANDLE,
    NULL,
    RtlMoveMemory,
}
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12Resource,
    ID3D12Device,
    ID3D12Resource,
    D3D12_HEAP_PROPERTIES,
    D3D12_RESOURCE_DESC,
    D3D12_RANGE,
    D3D12HeapType,
    D3D12CpuPageProperty,
    D3D12MemoryPool,
    D3D12ResourceDimension,
    D3D12TextureLayout,
    D3D12ResourceFlags,
    D3D12ResourceStates,
    D3D12HeapMiscFlags,
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
            &heap,
            D3D12HeapMiscFlags.none as u32,
            &desc,
            D3D12ResourceStates.genericRead as u32,
            NULL,
            &IID_ID3D12Resource,
            &rawBuf
        )
        test.expect(brc as i64, 0)
        let buf = rawBuf as ID3D12Resource

        // GetGPUVirtualAddress — must be non-zero for buffers.
        let gpuVa = buf.GetGPUVirtualAddress()
        test.expectTrue(gpuVa != 0 as u64)

        // Map subresource 0 (whole buffer), passing 0..0 read range
        // to declare "we won't read CPU-side from this map".
        let mapped: *void = NULL
        let readNone = D3D12_RANGE { Begin: 0, End: 0 }
        test.expect(buf.Map(0 as u32, &readNone, &mapped) as i64, 0)
        test.expectTrue((mapped as i64) != 0)

        // Stage a 16-byte pattern and copy it in via RtlMoveMemory —
        // exercises the upload-heap CPU/GPU shared mapping.
        let pattern: u32[4] = [0xDEADBEEF, 0xCAFEBABE, 0x12345678, 0xFEEDFACE]
        RtlMoveMemory(mapped, pattern, 16 as u64)

        buf.Unmap(0 as u32, NULL)

        // Re-map and read back to verify the bytes landed.
        let mapped2: *void = NULL
        test.expect(buf.Map(0 as u32, &readNone, &mapped2) as i64, 0)
        let echo: u32[4] = [0, 0, 0, 0]
        RtlMoveMemory(echo, mapped2, 16 as u64)
        buf.Unmap(0 as u32, NULL)
        test.expect(echo[0] as i64, 0xDEADBEEF)
        test.expect(echo[1] as i64, 0xCAFEBABE)
        test.expect(echo[2] as i64, 0x12345678)
        test.expect(echo[3] as i64, 0xFEEDFACE)

        test.expect(buf.Release() as i64, 0)
        test.expect(device.Release() as i64, 0)
    }
}

run()
