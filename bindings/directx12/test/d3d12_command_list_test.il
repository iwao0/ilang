/// d3d12 — record + submit a tiny direct command list.
///
/// Exercises the @com methods that the second-pass binding now
/// surfaces on `ID3D12GraphicsCommandList`:
///   * Reset / Close
///   * CopyBufferRegion / CopyResource
///   * ResourceBarrier (transition)
///   * IASetPrimitiveTopology / SetDescriptorHeaps
///
/// Submits via `ID3D12CommandQueue.ExecuteCommandLists`, then waits
/// on a fence to make sure the GPU made it through the list.

use std.test as test
use windows {
    HANDLE, NULL,
    CreateEventA, WaitForSingleObject, CloseHandle,
}
use directx12 {
    D3D12CreateDevice, D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12CommandQueue, IID_ID3D12CommandAllocator,
    IID_ID3D12GraphicsCommandList,
    IID_ID3D12Resource, IID_ID3D12DescriptorHeap, IID_ID3D12Fence,
    ID3D12Device,
    ID3D12CommandQueue,
    ID3D12CommandAllocator,
    ID3D12GraphicsCommandList,
    ID3D12CommandList,
    ID3D12Resource,
    ID3D12DescriptorHeap,
    ID3D12Fence,
    ID3D12PipelineState,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12_DESCRIPTOR_HEAP_DESC,
    D3D12_HEAP_PROPERTIES,
    D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_BARRIER,
    D3D12CommandListType,
    D3D12CommandQueueFlags,
    D3D12CommandQueuePriority,
    D3D12DescriptorHeapType,
    D3D12DescriptorHeapFlags,
    D3D12HeapType,
    D3D12CpuPageProperty,
    D3D12MemoryPool,
    D3D12ResourceDimension,
    D3D12TextureLayout,
    D3D12ResourceFlags,
    D3D12ResourceStates,
    D3D12ResourceBarrierType,
    D3D12ResourceBarrierFlags,
    D3D12HeapMiscFlags,
    D3D12FenceFlags,
    D3DPrimitiveTopology,
    DXGI_SAMPLE_DESC, DxgiFormat,
}
use ole32 { IUnknown }

@extern(C) {
    pub fn run() {
        let rawDev: *void = NULL
        let rc = D3D12CreateDevice(
            NULL, D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device, &rawDev
        )
        if rc != 0 {
            return
        }
        let device = rawDev as ID3D12Device

        // ── Queue ──────────────────────────────────────────────────
        let qDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }
        let rawQueue: *void = NULL
        test.expect(device.CreateCommandQueue(&qDesc, &IID_ID3D12CommandQueue, &rawQueue) as i64, 0)
        let queue = rawQueue as ID3D12CommandQueue

        // ── Allocator + command list ───────────────────────────────
        let rawAlloc: *void = NULL
        test.expect(device.CreateCommandAllocator(
            D3D12CommandListType.direct as i32,
            &IID_ID3D12CommandAllocator,
            &rawAlloc
        ) as i64, 0)
        let alloc = rawAlloc as ID3D12CommandAllocator

        let rawList: *void = NULL
        test.expect(device.CreateCommandList(
            0 as u32,
            D3D12CommandListType.direct as i32,
            &alloc,
            0 as *ID3D12PipelineState,    // no initial PSO
            &IID_ID3D12GraphicsCommandList,
            &rawList
        ) as i64, 0)
        let list = rawList as ID3D12GraphicsCommandList

        // ── Two default-heap buffers for Copy{BufferRegion,Resource} ─
        // Initial state COMMON so we can transition to COPY_DEST /
        // COPY_SOURCE via ResourceBarrier on the list.
        let dflt = D3D12_HEAP_PROPERTIES {
            Type:                 D3D12HeapType.default as u32,
            CPUPageProperty:      D3D12CpuPageProperty.unknown as u32,
            MemoryPoolPreference: D3D12MemoryPool.unknown as u32,
            CreationNodeMask:     1, VisibleNodeMask: 1,
        }
        let bufDesc = D3D12_RESOURCE_DESC {
            Dimension: D3D12ResourceDimension.buffer as u32,
            Alignment: 0, Width: 256 as u64, Height: 1,
            DepthOrArraySize: 1, MipLevels: 1,
            Format: DxgiFormat.unknown as u32,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12TextureLayout.rowMajor as u32,
            Flags:  D3D12ResourceFlags.none as u32,
        }
        let rawA: *void = NULL
        let rawB: *void = NULL
        test.expect(device.CreateCommittedResource(
            &dflt, D3D12HeapMiscFlags.none as u32, &bufDesc,
            D3D12ResourceStates.common as u32, NULL,
            &IID_ID3D12Resource, &rawA
        ) as i64, 0)
        test.expect(device.CreateCommittedResource(
            &dflt, D3D12HeapMiscFlags.none as u32, &bufDesc,
            D3D12ResourceStates.common as u32, NULL,
            &IID_ID3D12Resource, &rawB
        ) as i64, 0)
        let bufA = rawA as ID3D12Resource
        let bufB = rawB as ID3D12Resource

        // ── A shader-visible CBV heap to plug into SetDescriptorHeaps ─
        let cbvDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.cbvSrvUav as u32,
            NumDescriptors: 1,
            Flags:          D3D12DescriptorHeapFlags.shaderVisible as u32,
            NodeMask:       0,
        }
        let rawCbv: *void = NULL
        test.expect(device.CreateDescriptorHeap(&cbvDesc, &IID_ID3D12DescriptorHeap, &rawCbv) as i64, 0)
        let cbvHeap = rawCbv as ID3D12DescriptorHeap

        // ── Record: barriers → copy → IA / heaps ───────────────────
        // Transition bufA to COPY_DEST and bufB to COPY_SOURCE.
        let toDst = D3D12_RESOURCE_BARRIER {
            Type:    D3D12ResourceBarrierType.transition as u32,
            Flags:   D3D12ResourceBarrierFlags.none as u32,
            pResource:   &bufA,
            Subresource: 0xFFFFFFFF as u32,
            StateBefore: D3D12ResourceStates.common   as u32,
            StateAfter:  D3D12ResourceStates.copyDest as u32,
            _pad:    0,
        }
        let toSrc = D3D12_RESOURCE_BARRIER {
            Type:    D3D12ResourceBarrierType.transition as u32,
            Flags:   D3D12ResourceBarrierFlags.none as u32,
            pResource:   &bufB,
            Subresource: 0xFFFFFFFF as u32,
            StateBefore: D3D12ResourceStates.common     as u32,
            StateAfter:  D3D12ResourceStates.copySource as u32,
            _pad:    0,
        }
        list.ResourceBarrier(1 as u32, &toDst)
        list.ResourceBarrier(1 as u32, &toSrc)

        list.CopyBufferRegion(&bufA, 0 as u64, &bufB, 0 as u64, 64 as u64)
        list.CopyResource(&bufA, &bufB)

        list.IASetPrimitiveTopology(D3DPrimitiveTopology.triangleList as u32)

        // SetDescriptorHeaps wants an array of heap pointers; for
        // the one-heap case the standard pattern is `&heapPtr`.
        let heapPtr: *ID3D12DescriptorHeap = &cbvHeap as *ID3D12DescriptorHeap
        list.SetDescriptorHeaps(1 as u32, &heapPtr)

        test.expect(list.Close() as i64, 0)

        // ── Submit + fence-sync ────────────────────────────────────
        let listPtr: *ID3D12CommandList = &list as *ID3D12CommandList
        queue.ExecuteCommandLists(1 as u32, &listPtr)

        let rawFence: *void = NULL
        test.expect(device.CreateFence(
            0 as u64, D3D12FenceFlags.none as u32,
            &IID_ID3D12Fence, &rawFence
        ) as i64, 0)
        let fence = rawFence as ID3D12Fence

        test.expect(queue.Signal(&fence, 1 as u64) as i64, 0)
        let ev = CreateEventA(NULL, 0, 0, NULL)
        if fence.GetCompletedValue() < 1 as u64 {
            test.expect(fence.SetEventOnCompletion(1 as u64, ev) as i64, 0)
            let _ = WaitForSingleObject(ev, 1000 as u32)
        }
        test.expectTrue(fence.GetCompletedValue() >= 1 as u64)

        // ── Reset the allocator + list (covers both Reset methods) ──
        test.expect(alloc.Reset() as i64, 0)
        test.expect(list.Reset(&alloc, 0 as *ID3D12PipelineState) as i64, 0)
        test.expect(list.Close() as i64, 0)

        // ── Cleanup ────────────────────────────────────────────────
        let _ = CloseHandle(ev)
        test.expect(fence.Release()   as i64, 0)
        test.expect(cbvHeap.Release() as i64, 0)
        test.expect(bufB.Release()    as i64, 0)
        test.expect(bufA.Release()    as i64, 0)
        test.expect(list.Release()    as i64, 0)
        test.expect(alloc.Release()   as i64, 0)
        test.expect(queue.Release()   as i64, 0)
        test.expect(device.Release()  as i64, 0)
    }

}

run()
