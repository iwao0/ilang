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

use windows { HANDLE }
use d3d12 {
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12_DESCRIPTOR_HEAP_DESC,
    D3D12_CPU_DESCRIPTOR_HANDLE,
    D3D12_GPU_DESCRIPTOR_HANDLE,
    D3D12_RESOURCE_BARRIER,
    D3D12_RESOURCE_DESC,
    D3D12_VERTEX_BUFFER_VIEW,
    D3D12_INDEX_BUFFER_VIEW,
    D3D12_VIEWPORT,
    D3D12_RANGE,
    D3D12_RECT,
    D3D12_BOX,
    D3D12_HEAP_PROPERTIES,
    D3D12_HEAP_DESC,
    D3D12_CLEAR_VALUE,
    D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    D3D12_COMPUTE_PIPELINE_STATE_DESC,
    D3D12_CONSTANT_BUFFER_VIEW_DESC,
    D3D12_SHADER_RESOURCE_VIEW_DESC,
    D3D12_UNORDERED_ACCESS_VIEW_DESC,
    D3D12_RENDER_TARGET_VIEW_DESC,
    D3D12_DEPTH_STENCIL_VIEW_DESC,
    D3D12_SAMPLER_DESC,
    D3D12_RESOURCE_ALLOCATION_INFO,
    D3D12_QUERY_HEAP_DESC,
    D3D12_TEXTURE_COPY_LOCATION,
    D3D12_DISCARD_REGION,
    D3D12_TILED_RESOURCE_COORDINATE,
    D3D12_PROTECTED_RESOURCE_SESSION_DESC,
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

    /// ID3D12Heap — a chunk of GPU memory you can place resources into.
    @com pub interface ID3D12Heap : ID3D12Pageable {
        _slot8_GetDesc()
    }

    /// ID3D12QueryHeap — pool of GPU-side timestamp / occlusion /
    /// pipeline-statistics slots.
    @com pub interface ID3D12QueryHeap : ID3D12Pageable {
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
        WriteToSubresource(
            DstSubresource: u32,
            pDstBox:        *const D3D12_BOX,
            pSrcData:       *const void,
            SrcRowPitch:    u32,
            SrcDepthPitch:  u32
        ): i32
        ReadFromSubresource(
            pDstData:       *void,
            DstRowPitch:    u32,
            DstDepthPitch:  u32,
            SrcSubresource: u32,
            pSrcBox:        *const D3D12_BOX
        ): i32
        GetHeapProperties(
            pHeapProperties: *D3D12_HEAP_PROPERTIES,
            pHeapFlags:      *u32
        ): i32
    }

    /// ID3D12Resource1 — adds protected-session accessor (DRM).
    @com pub interface ID3D12Resource1 : ID3D12Resource {
        GetProtectedResourceSession(riid: *const GUID, ppProtectedSession: **void): i32
    }

    /// ID3D12Resource2 — adds GetDesc1 (extended desc with sampler
    /// feedback support). Hands back the desc by retval-pointer.
    @com pub interface ID3D12Resource2 : ID3D12Resource1 {
        GetDesc1(retval: *D3D12_RESOURCE_DESC): *D3D12_RESOURCE_DESC
    }

    /// ID3D12CommandAllocator — backing memory for command lists.
    @com pub interface ID3D12CommandAllocator : ID3D12Pageable {
        Reset(): i32
    }

    /// ID3D12Fence — CPU/GPU sync primitive.
    @com pub interface ID3D12Fence : ID3D12Pageable {
        GetCompletedValue(): u64
        SetEventOnCompletion(Value: u64, hEvent: HANDLE): i32
        Signal(Value: u64): i32
    }

    /// ID3D12Fence1 — adds creation-flags accessor.
    @com pub interface ID3D12Fence1 : ID3D12Fence {
        GetCreationFlags(): u32
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
        ExecuteCommandLists(NumCommandLists: u32, ppCommandLists: *const *ID3D12CommandList)
        _slot11_SetMarker()
        _slot12_BeginEvent()
        _slot13_EndEvent()
        Signal(pFence: *ID3D12Fence, Value: u64): i32
        /// Block the queue (GPU-side) until `pFence` reaches `Value`.
        Wait(pFence: *ID3D12Fence, Value: u64): i32
        /// Frequency of timestamps in ticks per second.
        GetTimestampFrequency(pFrequency: *u64): i32
        /// Sample GPU + CPU timestamps simultaneously for cross-clock
        /// calibration.
        GetClockCalibration(pGpuTimestamp: *u64, pCpuTimestamp: *u64): i32
        GetDesc(retval: *D3D12_COMMAND_QUEUE_DESC): *D3D12_COMMAND_QUEUE_DESC
    }

    /// ID3D12Device — main GPU device. Slots match the SDK vtable
    /// 1:1 so call sites that re-use the C SDK's slot number
    /// (typical Microsoft tutorials) line up exactly. Methods this
    /// repo doesn't call yet keep their position as `_slotN_Name`
    /// placeholders. `*Ptr` u64 parameters carry CPU/GPU descriptor
    /// handles by value (one SIZE_T at the ABI).
    @com pub interface ID3D12Device : ID3D12Object {
        GetNodeCount(): u32
        CreateCommandQueue(
            pDesc: *const D3D12_COMMAND_QUEUE_DESC,
            riid: *const GUID,
            ppCommandQueue: **void
        ): i32
        CreateCommandAllocator(
            type_: i32,
            riid: *const GUID,
            ppCommandAllocator: **void
        ): i32
        CreateGraphicsPipelineState(
            pDesc: *const D3D12_GRAPHICS_PIPELINE_STATE_DESC,
            riid: *const GUID,
            ppPipelineState: **void
        ): i32
        CreateComputePipelineState(
            pDesc: *const D3D12_COMPUTE_PIPELINE_STATE_DESC,
            riid: *const GUID,
            ppPipelineState: **void
        ): i32
        CreateCommandList(
            nodeMask: u32,
            type_: i32,
            pCommandAllocator: *ID3D12CommandAllocator,
            pInitialState: *ID3D12PipelineState,
            riid: *const GUID,
            ppCommandList: **void
        ): i32
        /// `Feature` selects which feature-data struct `pFeatureSupportData`
        /// is interpreted as (see `D3D12Feature.*`); `FeatureSupportDataSize`
        /// is `sizeof` that struct.
        CheckFeatureSupport(
            Feature: u32,
            pFeatureSupportData: *void,
            FeatureSupportDataSize: u32
        ): i32
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
            riid: *const GUID,
            ppvRootSignature: **void
        ): i32
        CreateConstantBufferView(
            pDesc: *const D3D12_CONSTANT_BUFFER_VIEW_DESC,
            DestDescriptorPtr: u64
        )
        CreateShaderResourceView(
            pResource: *ID3D12Resource,
            pDesc: *const D3D12_SHADER_RESOURCE_VIEW_DESC,
            DestDescriptorPtr: u64
        )
        CreateUnorderedAccessView(
            pResource: *ID3D12Resource,
            pCounterResource: *ID3D12Resource,
            pDesc: *const D3D12_UNORDERED_ACCESS_VIEW_DESC,
            DestDescriptorPtr: u64
        )
        // `DestDescriptor` is a `D3D12_CPU_DESCRIPTOR_HANDLE` (single
        // SIZE_T) passed by value on x64 — identical at the ABI to a
        // u64 in rcx. Take the raw `.ptr` to sidestep ilang's
        // CRepr-struct value-vs-ptr mismatch on the call site.
        CreateRenderTargetView(
            pResource: *ID3D12Resource,
            pDesc: *const D3D12_RENDER_TARGET_VIEW_DESC,
            DestDescriptorPtr: u64
        )
        CreateDepthStencilView(
            pResource: *ID3D12Resource,
            pDesc: *const D3D12_DEPTH_STENCIL_VIEW_DESC,
            DestDescriptorPtr: u64
        )
        CreateSampler(
            pDesc: *const D3D12_SAMPLER_DESC,
            DestDescriptorPtr: u64
        )
        CopyDescriptors(
            NumDestDescriptorRanges:   u32,
            pDestDescriptorRangeStarts: *const D3D12_CPU_DESCRIPTOR_HANDLE,
            pDestDescriptorRangeSizes:  *const u32,
            NumSrcDescriptorRanges:    u32,
            pSrcDescriptorRangeStarts: *const D3D12_CPU_DESCRIPTOR_HANDLE,
            pSrcDescriptorRangeSizes:  *const u32,
            DescriptorHeapsType:       u32
        )
        CopyDescriptorsSimple(
            NumDescriptors:           u32,
            DestDescriptorRangeStartPtr: u64,
            SrcDescriptorRangeStartPtr:  u64,
            DescriptorHeapsType:      u32
        )
        /// `pResourceDescs` length = `numResourceDescs`.
        /// `retval` is filled with the alignment+size and returned.
        GetResourceAllocationInfo(
            retval: *D3D12_RESOURCE_ALLOCATION_INFO,
            visibleMask:        u32,
            numResourceDescs:   u32,
            pResourceDescs:     *const D3D12_RESOURCE_DESC
        ): *D3D12_RESOURCE_ALLOCATION_INFO
        GetCustomHeapProperties(
            retval: *D3D12_HEAP_PROPERTIES,
            nodeMask: u32,
            heapType: u32
        ): *D3D12_HEAP_PROPERTIES
        CreateCommittedResource(
            pHeapProperties: *const D3D12_HEAP_PROPERTIES,
            HeapFlags: u32,
            pDesc: *const D3D12_RESOURCE_DESC,
            InitialResourceState: u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            riidResource: *const GUID,
            ppvResource: **void
        ): i32
        CreateHeap(
            pDesc: *const D3D12_HEAP_DESC,
            riid: *const GUID,
            ppvHeap: **void
        ): i32
        CreatePlacedResource(
            pHeap:                *ID3D12Heap,
            HeapOffset:           u64,
            pDesc:                *const D3D12_RESOURCE_DESC,
            InitialState:         u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            riid:                 *const GUID,
            ppvResource:          **void
        ): i32
        CreateReservedResource(
            pDesc:                *const D3D12_RESOURCE_DESC,
            InitialState:         u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            riid:                 *const GUID,
            ppvResource:          **void
        ): i32
        _slot31_CreateSharedHandle()
        _slot32_OpenSharedHandle()
        _slot33_OpenSharedHandleByName()
        MakeResident(NumObjects: u32, ppObjects: *const *void): i32
        Evict(NumObjects: u32, ppObjects: *const *void): i32
        CreateFence(
            InitialValue: u64,
            Flags: u32,
            riid: *const GUID,
            ppFence: **void
        ): i32
        GetDeviceRemovedReason(): i32
        _slot38_GetCopyableFootprints()
        CreateQueryHeap(
            pDesc: *const D3D12_QUERY_HEAP_DESC,
            riid: *const GUID,
            ppvHeap: **void
        ): i32
        _slot40_SetStablePowerState()
        _slot41_CreateCommandSignature()
        _slot42_GetResourceTiling()
        _slot43_GetAdapterLuid()
    }

    // ── ID3D12Device1 … 5 ─────────────────────────────────────────
    //
    // Each generation adds a handful of methods. Only the ones the
    // repo actively needs get typed signatures; the rest are
    // `_slotN_Name()` placeholders that anchor the SDK numbering.

    /// ID3D12Device1 — adds PSO library + pipeline residency
    /// priority + event-on-multi-fence.
    @com pub interface ID3D12Device1 : ID3D12Device {
        CreatePipelineLibrary(
            pLibraryBlob:      *const void,
            BlobLength:        u64,
            riid:              *const GUID,
            ppPipelineLibrary: **void
        ): i32
        _slot45_SetEventOnMultipleFenceCompletion()
        _slot46_SetResidencyPriority()
    }

    /// ID3D12Device2 — adds a unified `CreatePipelineState` that
    /// takes a `D3D12_PIPELINE_STATE_STREAM_DESC` (so the desc
    /// stream can include mesh shaders, depth bounds, etc).
    @com pub interface ID3D12Device2 : ID3D12Device1 {
        CreatePipelineState(
            pDesc: *const void,                     // D3D12_PIPELINE_STATE_STREAM_DESC
            riid:  *const GUID,
            ppPipelineState: **void
        ): i32
    }

    /// ID3D12Device3 — adds external-heap import, residency, fences
    /// across processes.
    @com pub interface ID3D12Device3 : ID3D12Device2 {
        _slot48_OpenExistingHeapFromAddress()
        _slot49_OpenExistingHeapFromFileMapping()
        _slot50_EnqueueMakeResident()
    }

    /// ID3D12Device4 — adds CreateCommandList1 (no allocator
    /// up-front), heap1, protected sessions, committed1.
    @com pub interface ID3D12Device4 : ID3D12Device3 {
        CreateCommandList1(
            nodeMask: u32,
            type_:    i32,
            flags:    u32,
            riid:     *const GUID,
            ppCommandList: **void
        ): i32
        _slot52_CreateProtectedResourceSession()
        CreateCommittedResource1(
            pHeapProperties:           *const D3D12_HEAP_PROPERTIES,
            HeapFlags:                 u32,
            pDesc:                     *const D3D12_RESOURCE_DESC,
            InitialResourceState:      u32,
            pOptimizedClearValue:      *const D3D12_CLEAR_VALUE,
            pProtectedSession:         *void,
            riidResource:              *const GUID,
            ppvResource:               **void
        ): i32
        CreateHeap1(
            pDesc:             *const D3D12_HEAP_DESC,
            pProtectedSession: *void,
            riid:              *const GUID,
            ppvHeap:           **void
        ): i32
        _slot55_CreateReservedResource1()
        GetResourceAllocationInfo1(
            retval:               *D3D12_RESOURCE_ALLOCATION_INFO,
            visibleMask:          u32,
            numResourceDescs:     u32,
            pResourceDescs:       *const D3D12_RESOURCE_DESC,
            pResourceAllocationInfo1: *void
        ): *D3D12_RESOURCE_ALLOCATION_INFO
    }

    /// ID3D12Device5 — adds raytracing state-object creation +
    /// removed-reason event registration.
    @com pub interface ID3D12Device5 : ID3D12Device4 {
        _slot57_CreateLifetimeTracker()
        _slot58_RemoveDevice()
        _slot59_EnumerateMetaCommands()
        _slot60_EnumerateMetaCommandParameters()
        _slot61_CreateMetaCommand()
        CreateStateObject(
            pDesc:        *const void,              // D3D12_STATE_OBJECT_DESC
            riid:         *const GUID,
            ppStateObject: **void
        ): i32
        GetRaytracingAccelerationStructurePrebuildInfo(
            pDesc: *const void,                     // D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS
            pInfo: *void                            // D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO
        )
        _slot64_CheckDriverMatchingIdentifier()
    }

    /// ID3D12Device6 — adds SetBackgroundProcessingMode.
    @com pub interface ID3D12Device6 : ID3D12Device5 {
        _slot65_SetBackgroundProcessingMode()
    }

    /// ID3D12Device7 — adds CreateProtectedResourceSession1 +
    /// AddToStateObject (incremental DXR state-object growth).
    @com pub interface ID3D12Device7 : ID3D12Device6 {
        AddToStateObject(
            pAddition:                *const void,
            pStateObjectToGrowFrom:   *ID3D12StateObject,
            riid:                     *const GUID,
            ppNewStateObject:         **void
        ): i32
        _slot67_CreateProtectedResourceSession1()
    }

    /// ID3D12Device8 — adds GetCopyableFootprints1 / CreateCommittedResource2 /
    /// CreatePlacedResource1 / CreateSamplerFeedbackUnorderedAccessView /
    /// GetResourceAllocationInfo2.
    @com pub interface ID3D12Device8 : ID3D12Device7 {
        _slot68_GetResourceAllocationInfo2()
        CreateCommittedResource2(
            pHeapProperties:           *const D3D12_HEAP_PROPERTIES,
            HeapFlags:                 u32,
            pDesc:                     *const D3D12_RESOURCE_DESC,
            InitialResourceState:      u32,
            pOptimizedClearValue:      *const D3D12_CLEAR_VALUE,
            pProtectedSession:         *void,
            riidResource:              *const GUID,
            ppvResource:               **void
        ): i32
        CreatePlacedResource1(
            pHeap:                *ID3D12Heap,
            HeapOffset:           u64,
            pDesc:                *const D3D12_RESOURCE_DESC,
            InitialState:         u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            riid:                 *const GUID,
            ppvResource:          **void
        ): i32
        _slot71_CreateSamplerFeedbackUnorderedAccessView()
        _slot72_GetCopyableFootprints1()
    }

    /// ID3D12Device9 — adds CreateShaderCacheSession / ShaderCacheControl /
    /// CreateCommandQueue1.
    @com pub interface ID3D12Device9 : ID3D12Device8 {
        _slot73_CreateShaderCacheSession()
        _slot74_ShaderCacheControl()
        CreateCommandQueue1(
            pDesc:                  *const D3D12_COMMAND_QUEUE_DESC,
            CreatorID:              *const GUID,
            riid:                   *const GUID,
            ppCommandQueue:         **void
        ): i32
    }

    /// ID3D12Device10 — adds CreateCommittedResource3 /
    /// CreatePlacedResource2 / CreateReservedResource1 / 2 (initial
    /// layout-aware variants).
    @com pub interface ID3D12Device10 : ID3D12Device9 {
        CreateCommittedResource3(
            pHeapProperties:           *const D3D12_HEAP_PROPERTIES,
            HeapFlags:                 u32,
            pDesc:                     *const D3D12_RESOURCE_DESC,
            InitialLayout:             u32,            // D3D12BarrierLayout
            pOptimizedClearValue:      *const D3D12_CLEAR_VALUE,
            pProtectedSession:         *void,
            NumCastableFormats:        u32,
            pCastableFormats:          *const u32,
            riidResource:              *const GUID,
            ppvResource:               **void
        ): i32
        CreatePlacedResource2(
            pHeap:                *ID3D12Heap,
            HeapOffset:           u64,
            pDesc:                *const D3D12_RESOURCE_DESC,
            InitialLayout:        u32,
            pOptimizedClearValue: *const D3D12_CLEAR_VALUE,
            NumCastableFormats:   u32,
            pCastableFormats:     *const u32,
            riid:                 *const GUID,
            ppvResource:          **void
        ): i32
        _slot78_CreateReservedResource1()
        _slot79_CreateReservedResource2()
    }

    /// ID3D12Device11 — adds CreateSampler2 (D3D12_SAMPLER_DESC2).
    @com pub interface ID3D12Device11 : ID3D12Device10 {
        CreateSampler2(
            pDesc:                *const void,         // D3D12_SAMPLER_DESC2
            DestDescriptorPtr:    u64
        )
    }

    // ── DRED ──────────────────────────────────────────────────────

    /// ID3D12DeviceRemovedExtendedDataSettings — opt-in to DRED
    /// before device creation: tell the runtime to capture
    /// auto-breadcrumbs / page-fault info on a device-removed event.
    @com pub interface ID3D12DeviceRemovedExtendedDataSettings : IUnknown {
        SetAutoBreadcrumbsEnablement(Enablement: u32)     // D3D12DredEnablement
        SetPageFaultEnablement(Enablement: u32)
        SetWatsonDumpEnablement(Enablement: u32)
    }

    /// ID3D12DeviceRemovedExtendedDataSettings1 — adds
    /// SetBreadcrumbContextEnablement.
    @com pub interface ID3D12DeviceRemovedExtendedDataSettings1 : ID3D12DeviceRemovedExtendedDataSettings {
        SetBreadcrumbContextEnablement(Enablement: u32)
    }

    /// ID3D12DeviceRemovedExtendedDataSettings2 — adds
    /// UseMarkersOnlyAutoBreadcrumbs.
    @com pub interface ID3D12DeviceRemovedExtendedDataSettings2 : ID3D12DeviceRemovedExtendedDataSettings1 {
        UseMarkersOnlyAutoBreadcrumbs(MarkersOnly: i32)
    }

    /// ID3D12DeviceRemovedExtendedData — post-removed accessor for
    /// the captured auto-breadcrumb / page-fault data.
    @com pub interface ID3D12DeviceRemovedExtendedData : IUnknown {
        GetAutoBreadcrumbsOutput(pOutput: *void): i32      // D3D12_DEVICE_REMOVED_EXTENDED_DATA
        GetPageFaultAllocationOutput(pOutput: *void): i32  // D3D12_DRED_PAGE_FAULT_OUTPUT
    }

    @com pub interface ID3D12DeviceRemovedExtendedData1 : ID3D12DeviceRemovedExtendedData {
        GetAutoBreadcrumbsOutput1(pOutput: *void): i32
        GetPageFaultAllocationOutput1(pOutput: *void): i32
    }

    @com pub interface ID3D12DeviceRemovedExtendedData2 : ID3D12DeviceRemovedExtendedData1 {
        GetPageFaultAllocationOutput2(pOutput: *void): i32
        GetDeviceState(): u32   // D3D12 device state enum
    }

    // ── Misc device-side helpers ─────────────────────────────────

    /// ID3D12CommandSignature — drives ExecuteIndirect.
    @com pub interface ID3D12CommandSignature : ID3D12Pageable {
    }

    /// ID3D12ProtectedSession — base for protected resource / queue
    /// sessions. No own methods at the @com level.
    @com pub interface ID3D12ProtectedSession : ID3D12DeviceChild {
        GetStatusFence(riid: *const GUID, ppFence: **void): i32
        GetSessionStatus(): u32   // D3D12ProtectedSessionStatus
    }

    /// ID3D12ProtectedResourceSession.
    @com pub interface ID3D12ProtectedResourceSession : ID3D12ProtectedSession {
        GetDesc(retval: *D3D12_PROTECTED_RESOURCE_SESSION_DESC): *D3D12_PROTECTED_RESOURCE_SESSION_DESC
    }

    /// ID3D12LifetimeTracker — opt-in lifetime / residency tracking.
    @com pub interface ID3D12LifetimeTracker : ID3D12DeviceChild {
        DestroyOwnedObject(pObject: *void): i32
    }

    /// ID3D12GraphicsCommandList — record graphics / compute work
    /// into a command allocator's memory. Slot positions mirror
    /// the SDK vtable; entries the repo hasn't surfaced as typed
    /// signatures yet stay as `_slotN_Name`. Descriptor handles
    /// passed by value (CPU/GPU) come in as `*Ptr: u64`.
    @com pub interface ID3D12GraphicsCommandList : ID3D12CommandList {
        Close(): i32
        Reset(pAllocator: *ID3D12CommandAllocator, pInitialState: *ID3D12PipelineState): i32
        ClearState(pPipelineState: *ID3D12PipelineState)
        DrawInstanced(
            VertexCountPerInstance: u32,
            InstanceCount: u32,
            StartVertexLocation: u32,
            StartInstanceLocation: u32
        )
        DrawIndexedInstanced(
            IndexCountPerInstance: u32,
            InstanceCount:         u32,
            StartIndexLocation:    u32,
            BaseVertexLocation:    i32,
            StartInstanceLocation: u32
        )
        Dispatch(
            ThreadGroupCountX: u32,
            ThreadGroupCountY: u32,
            ThreadGroupCountZ: u32
        )
        CopyBufferRegion(
            pDstBuffer: *ID3D12Resource,
            DstOffset:  u64,
            pSrcBuffer: *ID3D12Resource,
            SrcOffset:  u64,
            NumBytes:   u64
        )
        CopyTextureRegion(
            pDst:    *const D3D12_TEXTURE_COPY_LOCATION,
            DstX:    u32,
            DstY:    u32,
            DstZ:    u32,
            pSrc:    *const D3D12_TEXTURE_COPY_LOCATION,
            pSrcBox: *const D3D12_BOX
        )
        CopyResource(pDstResource: *ID3D12Resource, pSrcResource: *ID3D12Resource)
        _slot18_CopyTiles()
        ResolveSubresource(
            pDstResource: *ID3D12Resource,
            DstSubresource: u32,
            pSrcResource: *ID3D12Resource,
            SrcSubresource: u32,
            Format: u32
        )
        IASetPrimitiveTopology(PrimitiveTopology: u32)
        RSSetViewports(NumViewports: u32, pViewports: *const D3D12_VIEWPORT)
        RSSetScissorRects(NumRects: u32, pRects: *const D3D12_RECT)
        OMSetBlendFactor(BlendFactor: *const f32)
        OMSetStencilRef(StencilRef: u32)
        SetPipelineState(pPipelineState: *ID3D12PipelineState)
        ResourceBarrier(NumBarriers: u32, pBarriers: *const D3D12_RESOURCE_BARRIER)
        ExecuteBundle(pCommandList: *ID3D12GraphicsCommandList)
        SetDescriptorHeaps(
            NumDescriptorHeaps: u32,
            ppDescriptorHeaps:  *const *ID3D12DescriptorHeap
        )
        SetComputeRootSignature(pRootSignature: *ID3D12RootSignature)
        SetGraphicsRootSignature(pRootSignature: *ID3D12RootSignature)
        SetComputeRootDescriptorTable(
            RootParameterIndex: u32,
            BaseDescriptorPtr:  u64
        )
        SetGraphicsRootDescriptorTable(
            RootParameterIndex: u32,
            BaseDescriptorPtr:  u64
        )
        SetComputeRoot32BitConstant(
            RootParameterIndex:        u32,
            SrcData:                   u32,
            DestOffsetIn32BitValues:   u32
        )
        SetGraphicsRoot32BitConstant(
            RootParameterIndex:        u32,
            SrcData:                   u32,
            DestOffsetIn32BitValues:   u32
        )
        SetComputeRoot32BitConstants(
            RootParameterIndex:        u32,
            Num32BitValuesToSet:       u32,
            pSrcData:                  *const void,
            DestOffsetIn32BitValues:   u32
        )
        SetGraphicsRoot32BitConstants(
            RootParameterIndex:        u32,
            Num32BitValuesToSet:       u32,
            pSrcData:                  *const void,
            DestOffsetIn32BitValues:   u32
        )
        SetComputeRootConstantBufferView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        SetGraphicsRootConstantBufferView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        SetComputeRootShaderResourceView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        SetGraphicsRootShaderResourceView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        SetComputeRootUnorderedAccessView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        SetGraphicsRootUnorderedAccessView(
            RootParameterIndex: u32,
            BufferLocation:     u64
        )
        IASetIndexBuffer(pView: *const D3D12_INDEX_BUFFER_VIEW)
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
        /// `ClearFlags`: D3D12_CLEAR_FLAG_DEPTH = 1, _STENCIL = 2.
        ClearDepthStencilView(
            DepthStencilViewPtr: u64,
            ClearFlags:          u32,
            Depth:               f32,
            Stencil:             u8,
            NumRects:            u32,
            pRects:              *const D3D12_RECT
        )
        // `RenderTargetView` is a CPU descriptor handle passed by
        // value (single SIZE_T = u64).
        ClearRenderTargetView(
            RenderTargetViewPtr: u64,
            ColorRGBA: *const f32,
            NumRects: u32,
            pRects: *const D3D12_RECT
        )
        ClearUnorderedAccessViewUint(
            ViewGPUHandleInCurrentHeapPtr: u64,
            ViewCPUHandlePtr:              u64,
            pResource:                     *ID3D12Resource,
            Values:                        *const u32,
            NumRects:                      u32,
            pRects:                        *const D3D12_RECT
        )
        ClearUnorderedAccessViewFloat(
            ViewGPUHandleInCurrentHeapPtr: u64,
            ViewCPUHandlePtr:              u64,
            pResource:                     *ID3D12Resource,
            Values:                        *const f32,
            NumRects:                      u32,
            pRects:                        *const D3D12_RECT
        )
        DiscardResource(
            pResource: *ID3D12Resource,
            pRegion:   *const D3D12_DISCARD_REGION
        )
        BeginQuery(pQueryHeap: *ID3D12QueryHeap, Type: u32, Index: u32)
        EndQuery(pQueryHeap: *ID3D12QueryHeap, Type: u32, Index: u32)
        ResolveQueryData(
            pQueryHeap:        *ID3D12QueryHeap,
            Type:              u32,
            StartIndex:        u32,
            NumQueries:        u32,
            pDestinationBuffer: *ID3D12Resource,
            AlignedDestinationBufferOffset: u64
        )
        SetPredication(
            pBuffer:           *ID3D12Resource,
            AlignedBufferOffset: u64,
            Operation:         u32
        )
        _slot56_SetMarker()
        _slot57_BeginEvent()
        _slot58_EndEvent()
        ExecuteIndirect(
            pCommandSignature:        *void,
            MaxCommandCount:          u32,
            pArgumentBuffer:          *ID3D12Resource,
            ArgumentBufferOffset:     u64,
            pCountBuffer:             *ID3D12Resource,
            CountBufferOffset:        u64
        )
    }

    // ── ID3D12GraphicsCommandList1 … 7 ────────────────────────────
    //
    // Each generation adds a small batch of methods. The repo only
    // names the ones it has a need for; the rest stay as
    // `_slotN_Name()` placeholders that anchor the SDK slot
    // numbering. Casts let user code talk to whichever generation
    // the underlying object actually is.

    @com pub interface ID3D12GraphicsCommandList1 : ID3D12GraphicsCommandList {
        _slot60_AtomicCopyBufferUINT()
        _slot61_AtomicCopyBufferUINT64()
        OMSetDepthBounds(Min: f32, Max: f32)
        _slot63_SetSamplePositions()
        _slot64_ResolveSubresourceRegion()
        _slot65_SetViewInstanceMask()
    }

    @com pub interface ID3D12GraphicsCommandList2 : ID3D12GraphicsCommandList1 {
        _slot66_WriteBufferImmediate()
    }

    @com pub interface ID3D12GraphicsCommandList3 : ID3D12GraphicsCommandList2 {
        _slot67_SetProtectedResourceSession()
    }

    @com pub interface ID3D12GraphicsCommandList4 : ID3D12GraphicsCommandList3 {
        _slot68_BeginRenderPass()
        _slot69_EndRenderPass()
        _slot70_InitializeMetaCommand()
        _slot71_ExecuteMetaCommand()
        _slot72_BuildRaytracingAccelerationStructure()
        _slot73_EmitRaytracingAccelerationStructurePostbuildInfo()
        _slot74_CopyRaytracingAccelerationStructure()
        SetPipelineState1(pStateObject: *ID3D12StateObject)
        _slot76_DispatchRays()
    }

    @com pub interface ID3D12GraphicsCommandList5 : ID3D12GraphicsCommandList4 {
        _slot77_RSSetShadingRate()
        _slot78_RSSetShadingRateImage()
    }

    @com pub interface ID3D12GraphicsCommandList6 : ID3D12GraphicsCommandList5 {
        DispatchMesh(
            ThreadGroupCountX: u32,
            ThreadGroupCountY: u32,
            ThreadGroupCountZ: u32
        )
    }

    @com pub interface ID3D12GraphicsCommandList7 : ID3D12GraphicsCommandList6 {
        Barrier(NumBarrierGroups: u32, pBarrierGroups: *const void)
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

    /// ID3D12Debug1 — adds GPU-based validation + synchronized
    /// command-queue validation toggles. `SetEnableGPUBasedValidation`
    /// must run before device creation; the others can run on the
    /// debug interface obtained from an existing device.
    @com pub interface ID3D12Debug1 : IUnknown {
        EnableDebugLayer()
        SetEnableGPUBasedValidation(Enable: i32)
        SetEnableSynchronizedCommandQueueValidation(Enable: i32)
    }

    /// ID3D12Debug3 — adds GPU-validation flags configuration.
    @com pub interface ID3D12Debug3 : ID3D12Debug {
        SetEnableGPUBasedValidation(Enable: i32)
        SetEnableSynchronizedCommandQueueValidation(Enable: i32)
        SetGPUBasedValidationFlags(Flags: u32)
    }

    /// ID3D12Debug5 — adds auto-naming of D3D12 objects in the
    /// debug layer output.
    @com pub interface ID3D12Debug5 : ID3D12Debug3 {
        SetEnableAutoName(Enable: i32)
    }

    /// ID3D12DebugDevice — live-object reporting / feature toggles
    /// scoped to a single device instance.
    @com pub interface ID3D12DebugDevice : IUnknown {
        _slot3_SetFeatureMask()
        GetFeatureMask(): u32
        /// `Flags`: D3D12_RLDO_*; reports live D3D12 objects to the
        /// debug stream.
        ReportLiveDeviceObjects(Flags: u32): i32
    }

    /// ID3D12StateObject — compiled ray-tracing / DXR pipeline.
    /// No own methods at the @com level; identity tracked via
    /// IUnknown chain.
    @com pub interface ID3D12StateObject : ID3D12Pageable {
    }

    /// ID3D12StateObjectProperties — companion query interface for
    /// ID3D12StateObject; exposes shader identifiers and stack-size
    /// inspection.
    @com pub interface ID3D12StateObjectProperties : IUnknown {
        GetShaderIdentifier(pExportName: *const u16): *void
        GetShaderStackSize(pExportName: *const u16): u64
        GetPipelineStackSize(): u64
        SetPipelineStackSize(PipelineStackSizeInBytes: u64)
    }

    /// ID3D12PipelineLibrary — PSO disk-cache container. Lets you
    /// store and look up PSOs by name to avoid recompilation.
    @com pub interface ID3D12PipelineLibrary : ID3D12DeviceChild {
        StorePipeline(pName: *const u16, pPipeline: *ID3D12PipelineState): i32
        LoadGraphicsPipeline(
            pName: *const u16,
            pDesc: *const D3D12_GRAPHICS_PIPELINE_STATE_DESC,
            riid:  *const GUID,
            ppPipelineState: **void
        ): i32
        LoadComputePipeline(
            pName: *const u16,
            pDesc: *const D3D12_COMPUTE_PIPELINE_STATE_DESC,
            riid:  *const GUID,
            ppPipelineState: **void
        ): i32
        GetSerializedSize(): u64
        Serialize(pData: *void, DataSizeInBytes: u64): i32
    }

    /// ID3D12RootSignatureDeserializer — parses a serialised root
    /// signature blob back into the 1.0 desc form.
    @com pub interface ID3D12RootSignatureDeserializer : IUnknown {
        GetRootSignatureDesc(): *const void
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
