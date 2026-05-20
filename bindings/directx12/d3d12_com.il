/// COM-style interface bindings — IUnknown / ID3D12Device etc.
///
/// `@com interface I { ... }` declares a virtual contract whose
/// dispatch goes through a raw COM vtable
/// (`(*recv)[slot](recv, args...)`), not ilang's class registry.
/// Slot numbers come from declaration order, concatenated with any
/// parent interface, matching the C++ COM ABI verbatim. Receivers
/// are 8-byte handles (no ARC header); lifetime is managed by
/// `Release()`.
///
/// `@com interface` lives inside `@extern(C) { ... }` so its method
/// signatures can reference raw pointers and other C-only types
/// under the same scope rule that `fn` / `struct` decls follow.
///
/// Cast `*void`/`i64` ↔ interface to bring a COM pointer in or
/// out of FFI scope, then dispatch with `device.Method(args)`.
///
/// Slot-number rule: ilang's @com numbering walks `: Parent` chains
/// and appends own methods. To stay binary-compatible with the
/// SDK vtable, every interim slot is declared — methods this repo
/// never calls are kept as no-arg `_slotN()` placeholders so the
/// numbering stays anchored to the C++ COM ABI's positions.

use d3d12 {
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12_DESCRIPTOR_HEAP_DESC,
    D3D12_CPU_DESCRIPTOR_HANDLE,
    D3D12_GPU_DESCRIPTOR_HANDLE,
    D3D12_RESOURCE_BARRIER,
    D3D12_RESOURCE_DESC,
    D3D12_VERTEX_BUFFER_VIEW,
    D3D12_VIEWPORT,
    D3D12_RANGE,
    D3D12_RECT,
    D3D12_HEAP_PROPERTIES,
    D3D12_CLEAR_VALUE,
    D3D12_GRAPHICS_PIPELINE_STATE_DESC,
}

@extern(C) {
    /// Universal IUnknown vtable — every COM interface starts here.
    @com pub interface IUnknown {
        QueryInterface(riid: *const GUID, ppvObject: **void): i32
        AddRef(): u32
        Release(): u32
    }

    /// ID3D12Object — IUnknown + 4 debug/naming methods. Most
    /// D3D12 interfaces inherit from this.
    @com pub interface ID3D12Object : IUnknown {
        GetPrivateData(guid: *const GUID, pDataSize: *u32, pData: *void): i32
        SetPrivateData(guid: *const GUID, DataSize: u32, pData: *const void): i32
        SetPrivateDataInterface(guid: *const GUID, pData: i64): i32
        SetName(Name: *const u16): i32
    }

    /// ID3D12DeviceChild — every resource / pipeline state / etc.
    /// can hand back its owning device. One slot.
    @com pub interface ID3D12DeviceChild : ID3D12Object {
        GetDevice(riid: *const GUID, ppvDevice: **void): i32
    }

    /// ID3D12Pageable — marker (no own methods); pageable resources
    /// like buffers / textures / heaps inherit from here.
    @com pub interface ID3D12Pageable : ID3D12DeviceChild {
    }

    /// ID3D12RootSignature — opaque object created from a
    /// serialised root signature blob. No own methods.
    @com pub interface ID3D12RootSignature : ID3D12DeviceChild {
    }

    /// ID3D12CommandList — base for graphics / compute / copy
    /// lists. One own method.
    @com pub interface ID3D12CommandList : ID3D12DeviceChild {
        GetType(): u32
    }

    /// ID3D12Resource — buffers + textures.
    @com pub interface ID3D12Resource : ID3D12Pageable {
        Map(Subresource: u32, pReadRange: *const D3D12_RANGE, ppData: **void): i32
        Unmap(Subresource: u32, pWrittenRange: *const D3D12_RANGE)
        GetDesc(retval: *D3D12_RESOURCE_DESC): *D3D12_RESOURCE_DESC
        GetGPUVirtualAddress(): u64
        _slot12_WriteToSubresource()
        _slot13_ReadFromSubresource()
        _slot14_GetHeapProperties()
    }

    /// ID3D12CommandAllocator — backing memory for command lists.
    @com pub interface ID3D12CommandAllocator : ID3D12Pageable {
        Reset(): i32
    }

    /// ID3D12Fence — CPU/GPU sync primitive.
    @com pub interface ID3D12Fence : ID3D12Pageable {
        GetCompletedValue(): u64
        SetEventOnCompletion(Value: u64, hEvent: i64): i32
        Signal(Value: u64): i32
    }

    /// ID3D12DescriptorHeap — descriptor-handle heap (RTV / DSV /
    /// SRV / sampler). The GetCPU/GPU handle methods write the
    /// result into the caller-supplied retval pointer and also
    /// return that pointer (C-style IDL convention).
    @com pub interface ID3D12DescriptorHeap : ID3D12Pageable {
        GetDesc(retval: *D3D12_DESCRIPTOR_HEAP_DESC): *D3D12_DESCRIPTOR_HEAP_DESC
        GetCPUDescriptorHandleForHeapStart(retval: *D3D12_CPU_DESCRIPTOR_HANDLE): *D3D12_CPU_DESCRIPTOR_HANDLE
        GetGPUDescriptorHandleForHeapStart(retval: *D3D12_GPU_DESCRIPTOR_HANDLE): *D3D12_GPU_DESCRIPTOR_HANDLE
    }

    /// ID3D12PipelineState — compiled graphics / compute pipeline.
    @com pub interface ID3D12PipelineState : ID3D12Pageable {
        _slot8_GetCachedBlob()
    }

    /// ID3D12CommandQueue — owns the GPU timeline; submit lists,
    /// signal fences.
    @com pub interface ID3D12CommandQueue : ID3D12Pageable {
        _slot8_UpdateTileMappings()
        _slot9_CopyTileMappings()
        ExecuteCommandLists(NumCommandLists: u32, ppCommandLists: *const *void)
        _slot11_SetMarker()
        _slot12_BeginEvent()
        _slot13_EndEvent()
        Signal(pFence: i64, Value: u64): i32
        _slot15_Wait()
        _slot16_GetTimestampFrequency()
        _slot17_GetClockCalibration()
        GetDesc(retval: *D3D12_COMMAND_QUEUE_DESC): *D3D12_COMMAND_QUEUE_DESC
    }

    /// ID3D12Device — main GPU device. Slots match the SDK vtable
    /// 1:1 so call sites that re-use the C SDK's slot number
    /// (typical Microsoft tutorials) line up exactly. Methods this
    /// repo doesn't call yet keep their position as `_slotN_Name`
    /// placeholders.
    @com pub interface ID3D12Device : ID3D12Object {
        GetNodeCount(): u32
        CreateCommandQueue(
            pDesc: *const D3D12_COMMAND_QUEUE_DESC,
            riid: *const GUID,
            ppCommandQueue: **void
        ): i32
        CreateCommandAllocator(
            type_: u32,
            riid: *const GUID,
            ppCommandAllocator: **void
        ): i32
        CreateGraphicsPipelineState(
            pDesc: *const D3D12_GRAPHICS_PIPELINE_STATE_DESC,
            riid: *const GUID,
            ppPipelineState: **void
        ): i32
        _slot11_CreateComputePipelineState()
        CreateCommandList(
            nodeMask: u32,
            type_: u32,
            pCommandAllocator: i64,
            pInitialState: i64,
            riid: *const GUID,
            ppCommandList: **void
        ): i32
        _slot13_CheckFeatureSupport()
        CreateDescriptorHeap(
            pDesc: *const D3D12_DESCRIPTOR_HEAP_DESC,
            riid: *const GUID,
            ppHeap: **void
        ): i32
        GetDescriptorHandleIncrementSize(DescriptorHeapType: u32): u32
        CreateRootSignature(
            nodeMask: u32,
            pBlobWithRootSignature: *const void,
            blobLengthInBytes: u64,
            riid: *const void,
            ppvRootSignature: **void
        ): i32
        _slot17_CreateConstantBufferView()
        _slot18_CreateShaderResourceView()
        _slot19_CreateUnorderedAccessView()
        // `DestDescriptor` is a `D3D12_CPU_DESCRIPTOR_HANDLE` (single
        // SIZE_T) passed by value on x64 — identical at the ABI to a
        // u64 in rcx. Take the raw `.ptr` to sidestep ilang's
        // CRepr-struct value-vs-ptr mismatch on the call site.
        CreateRenderTargetView(
            pResource: i64,
            pDesc: *const void,
            DestDescriptorPtr: u64
        )
        _slot21_CreateDepthStencilView()
        _slot22_CreateSampler()
        _slot23_CopyDescriptors()
        _slot24_CopyDescriptorsSimple()
        _slot25_GetResourceAllocationInfo()
        _slot26_GetCustomHeapProperties()
        CreateCommittedResource(
            pHeapProperties: *const D3D12_HEAP_PROPERTIES,
            HeapFlags: u32,
            pDesc: *const D3D12_RESOURCE_DESC,
            InitialResourceState: u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            riidResource: *const GUID,
            ppvResource: **void
        ): i32
        _slot28_CreateHeap()
        _slot29_CreatePlacedResource()
        _slot30_CreateReservedResource()
        _slot31_CreateSharedHandle()
        _slot32_OpenSharedHandle()
        _slot33_OpenSharedHandleByName()
        _slot34_MakeResident()
        _slot35_Evict()
        CreateFence(
            InitialValue: u64,
            Flags: u32,
            riid: *const GUID,
            ppFence: **void
        ): i32
    }

    /// ID3D12GraphicsCommandList — record graphics / compute work
    /// into a command allocator's memory. Slot positions mirror
    /// the SDK vtable; unused entries stay as `_slotN_Name`.
    @com pub interface ID3D12GraphicsCommandList : ID3D12CommandList {
        Close(): i32
        Reset(pAllocator: i64, pInitialState: i64): i32
        _slot11_ClearState()
        DrawInstanced(
            VertexCountPerInstance: u32,
            InstanceCount: u32,
            StartVertexLocation: u32,
            StartInstanceLocation: u32
        )
        _slot13_DrawIndexedInstanced()
        _slot14_Dispatch()
        _slot15_CopyBufferRegion()
        _slot16_CopyTextureRegion()
        _slot17_CopyResource()
        _slot18_CopyTiles()
        _slot19_ResolveSubresource()
        IASetPrimitiveTopology(PrimitiveTopology: u32)
        RSSetViewports(NumViewports: u32, pViewports: *const D3D12_VIEWPORT)
        RSSetScissorRects(NumRects: u32, pRects: *const D3D12_RECT)
        _slot23_OMSetBlendFactor()
        _slot24_OMSetStencilRef()
        SetPipelineState(pPipelineState: i64)
        ResourceBarrier(NumBarriers: u32, pBarriers: *const D3D12_RESOURCE_BARRIER)
        _slot27_ExecuteBundle()
        _slot28_SetDescriptorHeaps()
        _slot29_SetComputeRootSignature()
        SetGraphicsRootSignature(pRootSignature: i64)
        _slot31_SetComputeRootDescriptorTable()
        _slot32_SetGraphicsRootDescriptorTable()
        _slot33_SetComputeRoot32BitConstant()
        _slot34_SetGraphicsRoot32BitConstant()
        _slot35_SetComputeRoot32BitConstants()
        _slot36_SetGraphicsRoot32BitConstants()
        _slot37_SetComputeRootConstantBufferView()
        _slot38_SetGraphicsRootConstantBufferView()
        _slot39_SetComputeRootShaderResourceView()
        _slot40_SetGraphicsRootShaderResourceView()
        _slot41_SetComputeRootUnorderedAccessView()
        _slot42_SetGraphicsRootUnorderedAccessView()
        _slot43_IASetIndexBuffer()
        IASetVertexBuffers(
            StartSlot: u32,
            NumViews: u32,
            pViews: *const D3D12_VERTEX_BUFFER_VIEW
        )
        _slot45_SOSetTargets()
        OMSetRenderTargets(
            NumRenderTargetDescriptors: u32,
            pRenderTargetDescriptors: *const D3D12_CPU_DESCRIPTOR_HANDLE,
            RTsSingleHandleToDescriptorRange: i32,
            pDepthStencilDescriptor: *const D3D12_CPU_DESCRIPTOR_HANDLE
        )
        _slot47_ClearDepthStencilView()
        // `RenderTargetView` is a CPU descriptor handle passed by
        // value (single SIZE_T = u64).
        ClearRenderTargetView(
            RenderTargetViewPtr: u64,
            ColorRGBA: *const f32,
            NumRects: u32,
            pRects: *const D3D12_RECT
        )
    }

    /// ID3DBlob — opaque memory blob (compiled shader bytecode,
    /// serialised root signature, error messages, ...). Lives in
    /// d3dcommon, but the IID is well-known and ilang doesn't
    /// need a separate translation unit for it.
    @com pub interface ID3DBlob : IUnknown {
        GetBufferPointer(): *void
        GetBufferSize(): u64
    }

    /// ID3D12Debug — entry point for the D3D12 debug layer. Call
    /// `EnableDebugLayer()` BEFORE creating the device to get
    /// detailed validation messages on subsequent API calls.
    @com pub interface ID3D12Debug : IUnknown {
        EnableDebugLayer()
    }

    /// ID3D12InfoQueue — programmatic access to debug-layer
    /// validation messages. Obtain via `device.QueryInterface`
    /// after `D3D12CreateDevice` (debug layer must be enabled
    /// up front). Slots between the ones we use stay as
    /// `_slotN_*` placeholders.
    @com pub interface ID3D12InfoQueue : IUnknown {
        _slot3_SetMessageCountLimit()
        _slot4_ClearStoredMessages()
        /// `GetMessage(MessageIndex, pMessage, pMessageByteLength)`.
        /// Pass `pMessage = NULL` + `*pMessageByteLength = 0` first to
        /// learn the required size; then alloc + call again.
        GetMessage(MessageIndex: u64, pMessage: *void, pMessageByteLength: *u64): i32
        _slot6_GetNumMessagesAllowedByStorageFilter()
        _slot7_GetNumMessagesDeniedByStorageFilter()
        GetNumStoredMessages(): u64
    }
}
