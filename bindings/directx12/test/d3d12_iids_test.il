/// d3d12 — verify a representative slice of binding-side IIDs
/// match the SDK-published GUID layout.
///
/// `Data1` / `Data2` / `Data3` are taken straight from the canonical
/// dash-separated form. `Data4lo` / `Data4hi` reverse-pack the
/// trailing 8 bytes as little-endian u32 — see the comment at the
/// top of `d3d12_iids.il` for the conversion. This fixture catches
/// silent drift between the source-of-truth header and the
/// hand-written `IID_*` constants.

use std.test
use windows { HANDLE }
use directx12 {
    IID_ID3D12Device,
    IID_ID3D12CommandQueue,
    IID_ID3D12CommandAllocator,
    IID_ID3D12GraphicsCommandList,
    IID_ID3D12Resource,
    IID_ID3D12DescriptorHeap,
    IID_ID3D12RootSignature,
    IID_ID3D12PipelineState,
    IID_ID3D12Fence,
    IID_ID3D12Debug,
    IID_ID3D12InfoQueue,
    IID_IDXGIFactory4,
    IID_IDXGISwapChain3,
    // New (second-pass) IIDs.
    IID_ID3D12Heap,
    IID_ID3D12QueryHeap,
    IID_ID3D12Fence1,
    IID_ID3D12Resource1,
    IID_ID3D12Resource2,
    IID_ID3D12Device1,
    IID_ID3D12Device5,
    IID_ID3D12GraphicsCommandList4,
    IID_ID3D12GraphicsCommandList6,
    IID_ID3D12Debug1,
    IID_ID3D12StateObject,
    IID_ID3D12StateObjectProperties,
    IID_ID3D12PipelineLibrary,
    IID_IDXGIFactory6,
    IID_IDXGIFactory7,
    IID_IDXGISwapChain4,
    IID_IDXGIAdapter1,
    IID_IDXGIAdapter4,
    IID_IDXGIOutput,
    IID_IDXGIOutput6,
    IID_IDXGIDevice,
    // Third-pass IIDs.
    IID_ID3D12Device8,
    IID_ID3D12Device10,
    IID_ID3D12DeviceRemovedExtendedDataSettings,
    IID_ID3D12DeviceRemovedExtendedData,
    IID_ID3D12CommandSignature,
    IID_ID3D12ProtectedResourceSession,
    IID_IDXGIResource,
    IID_IDXGIResource1,
    IID_IDXGIKeyedMutex,
    IID_IDXGISurface,
    IID_IDXGISurface1,
    IID_IDXGIOutputDuplication,
    IID_IDXGIDebug,
    IID_IDXGIDebug1,
    IID_IDXGIInfoQueue,
    IID_DXGI_DEBUG_ALL,
    IID_DXGI_DEBUG_DX,
    IID_DXGI_DEBUG_DXGI,
    IID_DXGI_DEBUG_APP,
}

@extern(C) {
    pub fn run() {
        // ── First-pass: a few previously-defined IIDs as a sanity
        //    floor (in case a refactor were to touch the header
        //    triple of the bundled GUIDs).
        // IID_ID3D12Device = 189819f1-1db6-4b57-be54-1821339b85f7
        test.expect(IID_ID3D12Device.Data1   as i64, 0x189819f1)
        test.expect(IID_ID3D12Device.Data2   as i64, 0x1db6)
        test.expect(IID_ID3D12Device.Data3   as i64, 0x4b57)
        test.expect(IID_ID3D12Device.Data4lo as i64, 0x211854be)
        test.expect(IID_ID3D12Device.Data4hi as i64, 0xf7859b33)
        // IID_ID3D12CommandQueue = 0ec870a6-5d7e-4c22-8cfc-5baae07616ed
        test.expect(IID_ID3D12CommandQueue.Data1 as i64, 0x0ec870a6)
        test.expect(IID_ID3D12CommandQueue.Data2 as i64, 0x5d7e)
        test.expect(IID_ID3D12CommandQueue.Data3 as i64, 0x4c22)
        // IID_ID3D12GraphicsCommandList = 5b160d0f-ac1b-4185-8ba8-b3ae42a5a455
        test.expect(IID_ID3D12GraphicsCommandList.Data1 as i64, 0x5b160d0f)
        // IID_ID3D12Resource = 696442be-a72e-4059-bc79-5b5c98040fad
        test.expect(IID_ID3D12Resource.Data1 as i64, 0x696442be)
        // IID_ID3D12Fence = 0a753dcf-c4d8-4b91-adf6-be5a60d95a76
        test.expect(IID_ID3D12Fence.Data1 as i64, 0x0a753dcf)
        // IID_ID3D12Debug = 344488b7-6846-474b-b989-f027448245e0
        test.expect(IID_ID3D12Debug.Data1 as i64, 0x344488b7)
        // IID_IDXGIFactory4 = 1bc6ea02-ef36-464f-bf0c-21ca39e5168a
        test.expect(IID_IDXGIFactory4.Data1 as i64, 0x1bc6ea02)
        // IID_IDXGISwapChain3 = 94d99bdb-f1f8-4ab0-b236-7da0170edab1
        test.expect(IID_IDXGISwapChain3.Data1 as i64, 0x94d99bdb)
        // Smoke: other first-pass entries' header triples.
        test.expect(IID_ID3D12CommandAllocator.Data1 as i64, 0x6102dee4)
        test.expect(IID_ID3D12DescriptorHeap.Data1   as i64, 0x8efb471d)
        test.expect(IID_ID3D12RootSignature.Data1    as i64, 0xc54a6b66)
        test.expect(IID_ID3D12PipelineState.Data1    as i64, 0x765a30f3)
        test.expect(IID_ID3D12InfoQueue.Data1        as i64, 0x0742a90b)

        // ── Second-pass: new D3D12 IIDs ────────────────────────────
        // IID_ID3D12Heap = 6b3b2502-6e51-45b3-90ee-9884265e8df3
        test.expect(IID_ID3D12Heap.Data1 as i64, 0x6b3b2502)
        test.expect(IID_ID3D12Heap.Data2 as i64, 0x6e51)
        test.expect(IID_ID3D12Heap.Data3 as i64, 0x45b3)
        // IID_ID3D12QueryHeap = 0d9658ae-ed45-469e-a61d-970ec583cab4
        test.expect(IID_ID3D12QueryHeap.Data1 as i64, 0x0d9658ae)
        // IID_ID3D12Fence1 = 433685fe-e22b-4ca0-a8db-b5b4f4dd0e4a
        test.expect(IID_ID3D12Fence1.Data1 as i64, 0x433685fe)
        // IID_ID3D12Resource1 = 9d5e227a-4430-4161-88b3-3eca6bb16e19
        test.expect(IID_ID3D12Resource1.Data1 as i64, 0x9d5e227a)
        test.expect(IID_ID3D12Resource1.Data2 as i64, 0x4430)
        test.expect(IID_ID3D12Resource1.Data3 as i64, 0x4161)
        // IID_ID3D12Resource2 = be36ec3b-ea85-4aeb-a45a-e9d76404a495
        test.expect(IID_ID3D12Resource2.Data1 as i64, 0xbe36ec3b)
        // IID_ID3D12Device1 = 77acce80-638e-4e65-8895-c1f23386863e
        test.expect(IID_ID3D12Device1.Data1 as i64, 0x77acce80)
        // IID_ID3D12Device5 = 8b4f173b-2fea-4b80-8f58-4307191ab95d
        test.expect(IID_ID3D12Device5.Data1 as i64, 0x8b4f173b)
        // IID_ID3D12GraphicsCommandList4 = 8754318e-d3a9-4541-98cf-645b50dc4874
        test.expect(IID_ID3D12GraphicsCommandList4.Data1 as i64, 0x8754318e)
        test.expect(IID_ID3D12GraphicsCommandList4.Data2 as i64, 0xd3a9)
        // IID_ID3D12GraphicsCommandList6 = c3827890-e548-4cfa-96cf-5689a9370f80
        test.expect(IID_ID3D12GraphicsCommandList6.Data1 as i64, 0xc3827890)
        // IID_ID3D12Debug1 = affaa4ca-63fe-4d8e-b8ad-159000af4304
        test.expect(IID_ID3D12Debug1.Data1 as i64, 0xaffaa4ca)
        // IID_ID3D12StateObject = 47016943-fca8-4594-93ea-af258b55346d
        test.expect(IID_ID3D12StateObject.Data1 as i64, 0x47016943)
        // IID_ID3D12StateObjectProperties = de5fa827-9bf9-4f26-89ff-d7f56fde3860
        test.expect(IID_ID3D12StateObjectProperties.Data1 as i64, 0xde5fa827)
        // IID_ID3D12PipelineLibrary = c64226a8-9201-46af-b4cc-53fb9ff7414f
        test.expect(IID_ID3D12PipelineLibrary.Data1 as i64, 0xc64226a8)

        // ── Second-pass: new DXGI IIDs ─────────────────────────────
        // IID_IDXGIFactory6 = c1b6694f-ff09-44a9-b03c-77900a0a1d17
        test.expect(IID_IDXGIFactory6.Data1 as i64, 0xc1b6694f)
        // IID_IDXGIFactory7 = a4966eed-76db-44da-84c1-ee9a7afb20a8
        test.expect(IID_IDXGIFactory7.Data1 as i64, 0xa4966eed)
        // IID_IDXGISwapChain4 = 3d585d5a-bd4a-489e-b1f4-3dbcb6452ffb
        test.expect(IID_IDXGISwapChain4.Data1 as i64, 0x3d585d5a)
        // IID_IDXGIAdapter1 = 29038f61-3839-4626-91fd-086879011a05
        test.expect(IID_IDXGIAdapter1.Data1 as i64, 0x29038f61)
        // IID_IDXGIAdapter4 = 3c8d99d1-4fbf-4181-a82c-af66bf7bd24e
        test.expect(IID_IDXGIAdapter4.Data1 as i64, 0x3c8d99d1)
        // IID_IDXGIOutput = ae02eedb-c735-4690-8d52-5a8dc20213aa
        test.expect(IID_IDXGIOutput.Data1 as i64, 0xae02eedb)
        // IID_IDXGIOutput6 = 068346e8-aaec-4b84-add7-137f513f77a1
        test.expect(IID_IDXGIOutput6.Data1 as i64, 0x068346e8)
        // IID_IDXGIDevice = 54ec77fa-1377-44e6-8c32-88fd5f44c84c
        test.expect(IID_IDXGIDevice.Data1 as i64, 0x54ec77fa)

        // ── Third-pass: Device6+, DRED, dxgi debug/info ────────────
        // IID_ID3D12Device8 = 9218e6bb-f944-4f7e-a75c-b1b2c7b701f3
        test.expect(IID_ID3D12Device8.Data1 as i64, 0x9218e6bb)
        // IID_ID3D12Device10 = 517f8718-aa66-49f9-b02b-a7ab89c06031
        test.expect(IID_ID3D12Device10.Data1 as i64, 0x517f8718)
        // IID_ID3D12DeviceRemovedExtendedDataSettings = 82bc481c-6b9b-4030-aedb-7ee3d1df1e63
        test.expect(IID_ID3D12DeviceRemovedExtendedDataSettings.Data1 as i64, 0x82bc481c)
        // IID_ID3D12DeviceRemovedExtendedData = 98931d33-5ae8-4791-aa3c-1a73a2934e71
        test.expect(IID_ID3D12DeviceRemovedExtendedData.Data1 as i64, 0x98931d33)
        // IID_ID3D12CommandSignature = c36a797c-ec80-4f0a-8985-a7b2475082d1
        test.expect(IID_ID3D12CommandSignature.Data1 as i64, 0xc36a797c)
        // IID_ID3D12ProtectedResourceSession = 6cd696f4-f289-40cc-8091-5a6c0a099c3d
        test.expect(IID_ID3D12ProtectedResourceSession.Data1 as i64, 0x6cd696f4)

        // IID_IDXGIResource = 035f3ab4-482e-4e50-b41f-8a7f8bd8960b
        test.expect(IID_IDXGIResource.Data1 as i64, 0x035f3ab4)
        // IID_IDXGIResource1 = 30961379-4609-4a41-998e-54fe567ee0c1
        test.expect(IID_IDXGIResource1.Data1 as i64, 0x30961379)
        // IID_IDXGIKeyedMutex = 9d8e1289-d7b3-465f-8126-250e349af85d
        test.expect(IID_IDXGIKeyedMutex.Data1 as i64, 0x9d8e1289)
        // IID_IDXGISurface = cafcb56c-6ac3-4889-bf47-9e23bbd260ec
        test.expect(IID_IDXGISurface.Data1 as i64, 0xcafcb56c)
        // IID_IDXGISurface1 = 4ae63092-6327-4c1b-80ae-bfe12ea32b86
        test.expect(IID_IDXGISurface1.Data1 as i64, 0x4ae63092)
        // IID_IDXGIOutputDuplication = 191cfac3-a341-470d-b26e-a864f428319c
        test.expect(IID_IDXGIOutputDuplication.Data1 as i64, 0x191cfac3)
        // IID_IDXGIDebug = 119e7452-de9e-40fe-8806-88f90c12b441
        test.expect(IID_IDXGIDebug.Data1 as i64, 0x119e7452)
        // IID_IDXGIDebug1 = c5a05f0c-16f2-4adf-9f4d-a8c4d58ac550
        test.expect(IID_IDXGIDebug1.Data1 as i64, 0xc5a05f0c)
        // IID_IDXGIInfoQueue = d67441c7-672a-476f-9e82-cd55b44949ce
        test.expect(IID_IDXGIInfoQueue.Data1 as i64, 0xd67441c7)

        // ── Debug producer GUIDs (used with IDXGIInfoQueue) ────────
        // DXGI_DEBUG_ALL = e48ae283-da80-490b-87e6-43e9a9cfda08
        test.expect(IID_DXGI_DEBUG_ALL.Data1 as i64, 0xe48ae283)
        // DXGI_DEBUG_DX  = 35cdd7fc-13b2-421d-a5d7-7e4451287d64
        test.expect(IID_DXGI_DEBUG_DX.Data1 as i64, 0x35cdd7fc)
        // DXGI_DEBUG_DXGI = 25cddaa4-b1c6-47e1-ac3e-98875b5a2e2a
        test.expect(IID_DXGI_DEBUG_DXGI.Data1 as i64, 0x25cddaa4)
        // DXGI_DEBUG_APP = 06cd6e01-4219-4ebd-8709-27ed23360c62
        test.expect(IID_DXGI_DEBUG_APP.Data1 as i64, 0x06cd6e01)
    }
}

run()
