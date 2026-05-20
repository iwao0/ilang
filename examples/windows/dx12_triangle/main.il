// 🔺 DirectX 12 hello-triangle.
//
// Run:
//   ilang run examples/windows/dx12_triangle/main.il
//
// Builds the canonical D3D12 hello-triangle pipeline:
//
//   1. Win32 window + message pump
//   2. DXGI factory + swap chain (double-buffered)
//   3. RTV descriptor heap + render target views for each back buffer
//   4. Empty root signature
//   5. HLSL VS / PS compiled inline via D3DCompile
//   6. Graphics PSO
//   7. Vertex buffer in an upload heap (3 vertices, position + colour)
//   8. Command allocator + command list + fence for per-frame sync
//   9. Render loop:
//      a. Reset allocator / list
//      b. Transition back buffer PRESENT → RENDER_TARGET
//      c. ClearRenderTargetView + DrawInstanced(3, 1, 0, 0)
//      d. Transition RENDER_TARGET → PRESENT
//      e. ExecuteCommandLists / Present / fence wait
//
// Close the window to exit. All COM handles + the class name C
// string are released after the message pump returns.

use windows {
    NULL,
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU, HANDLE,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd, WindowMessage,
    RegisterClassExA, CreateWindowExA, DefWindowProcA,
    ShowWindow, UpdateWindow,
    GetMessageA, TranslateMessage, DispatchMessageA,
    PostQuitMessage,
    SetWindowTextA,
    CreateEventA, WaitForSingleObject, CloseHandle,
    CreateDXGIFactory2,
    DXGI_SAMPLE_DESC, DXGI_SWAP_CHAIN_DESC1,
    DxgiFormat, DxgiUsage, DxgiSwapEffect, DxgiAlphaMode, DxgiScaling,
    IDXGIFactory4, IDXGISwapChain3,
}

use directx12 {
    D3D12CreateDevice, D3D12SerializeRootSignature,
    D3DFeatureLevel,
    GUID,
    IUnknown, ID3D12Device, ID3D12CommandQueue, ID3D12CommandAllocator,
    ID3D12GraphicsCommandList, ID3D12Resource, ID3D12Fence,
    ID3D12DescriptorHeap, ID3D12RootSignature, ID3D12PipelineState,
    ID3DBlob,
    IID_ID3D12Device, IID_ID3D12CommandQueue, IID_ID3D12CommandAllocator,
    IID_ID3D12GraphicsCommandList, IID_ID3D12Resource, IID_ID3D12Fence,
    IID_ID3D12DescriptorHeap, IID_ID3D12RootSignature, IID_ID3D12PipelineState,
    IID_IDXGIFactory4, IID_IDXGISwapChain3,
    D3D12_COMMAND_QUEUE_DESC, D3D12_DESCRIPTOR_HEAP_DESC, D3D12_VIEWPORT,
    D3D12_RECT, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_DESC,
    D3D12_HEAP_PROPERTIES, D3D12_CPU_DESCRIPTOR_HANDLE,
    D3D12_VERTEX_BUFFER_VIEW, D3D12_RANGE,
    D3D12_SHADER_BYTECODE, D3D12_BLEND_DESC, D3D12_RENDER_TARGET_BLEND_DESC,
    D3D12_RASTERIZER_DESC, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_STENCILOP_DESC,
    D3D12_INPUT_ELEMENT_DESC, D3D12_INPUT_LAYOUT_DESC, D3D12_STREAM_OUTPUT_DESC,
    D3D12_CACHED_PIPELINE_STATE, D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    D3D12_ROOT_SIGNATURE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    D3D12DescriptorHeapType, D3D12DescriptorHeapFlags,
    D3D12HeapType, D3D12HeapFlags, D3D12HeapMiscFlags,
    D3D12CpuPageProperty, D3D12MemoryPool,
    D3D12ResourceDimension, D3D12TextureLayout, D3D12ResourceFlags, D3D12ResourceStates,
    D3D12ResourceBarrierType, D3D12ResourceBarrierFlags,
    D3D12FillMode, D3D12CullMode, D3D12ComparisonFunc, D3D12Blend, D3D12BlendOp,
    D3D12LogicOp, D3D12StencilOp, D3D12DepthWriteMask, D3D12ColorWriteEnable,
    D3D12InputClassification, D3D12IndexBufferStripCutValue, D3D12PipelineStateFlags,
    D3D12RootSignatureFlags, D3D12RootSignatureVersion, D3D12PrimitiveTopologyType,
    D3DPrimitiveTopologyValue,
    D3DCompile,
}

@extern(C) {
    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == WindowMessage.destroy {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as *HWND, msg, wParam, lParam)
    }

    pub fn run() {
        // ── Constants ──────────────────────────────────────────────
        let width:  u32 = 800
        let height: u32 = 600

        // ── Window setup ───────────────────────────────────────────
        let hInstance  = GetModuleHandleA(NULL)
        let className  = cstrFromString("IlangDx12Triangle")
        let title      = cstrFromString("ilang DX12: hello triangle")

        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         ClassStyle.hredraw | ClassStyle.vredraw,
            lpfnWndProc:   wndProc,
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hInstance as *HINSTANCE,
            hIcon:         NULL,
            hCursor:       NULL,
            hbrBackground: NULL,
            lpszMenuName:  NULL,
            lpszClassName: className,
            hIconSm:       NULL,
        }
        RegisterClassExA(wc)

        let hwnd = CreateWindowExA(
            0,
            className,
            title,
            WindowStyle.overlappedWindow,
            120, 120,
            width as i32 + 16, height as i32 + 39,
            NULL, NULL,
            hInstance as *HINSTANCE,
            NULL
        )
        ShowWindow(hwnd, ShowCmd.show)
        UpdateWindow(hwnd)

        // ── DXGI factory ───────────────────────────────────────────
        // Copy each IID const into a local so `&` can take its
        // address — top-level `pub const` values get folded inline
        // and have no addressable storage of their own.
        let iidFactory4    = IID_IDXGIFactory4
        let iidSwapChain3  = IID_IDXGISwapChain3
        let iidResource    = IID_ID3D12Resource

        let rawFactory: *void = NULL
        let rcFactory = CreateDXGIFactory2(0, &iidFactory4 as *const GUID as *const void, &rawFactory)
        if rcFactory != 0 {
            SetWindowTextA(hwnd, cstrFromString("ilang DX12: CreateDXGIFactory2 failed"))
            return
        }
        let factory = rawFactory as IDXGIFactory4

        // ── D3D12 device ───────────────────────────────────────────
        let rawDevice: *void = NULL
        let rcDevice = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0,
            IID_ID3D12Device,
            &rawDevice
        )
        if rcDevice != 0 {
            SetWindowTextA(hwnd, cstrFromString("ilang DX12: D3D12CreateDevice failed - no GPU?"))
            return
        }
        let device = rawDevice as ID3D12Device

        // ── Command queue ──────────────────────────────────────────
        let queueDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct,
            Priority: D3D12CommandQueuePriority.normal,
            Flags:    D3D12CommandQueueFlags.none,
            NodeMask: 0,
        }
        let rawQueue: *void = NULL
        device.CreateCommandQueue(queueDesc, IID_ID3D12CommandQueue, &rawQueue)
        let cmdQueue = rawQueue as ID3D12CommandQueue

        // ── Swap chain ─────────────────────────────────────────────
        let scDesc = DXGI_SWAP_CHAIN_DESC1 {
            Width:       width,
            Height:      height,
            Format:      DxgiFormat.r8g8b8a8_unorm as u32,
            Stereo:      0,
            SampleDesc:  DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DxgiUsage.renderTargetOutput as u32,
            BufferCount: 2,
            Scaling:     DxgiScaling.stretch as u32,
            SwapEffect:  DxgiSwapEffect.flipDiscard as u32,
            AlphaMode:   DxgiAlphaMode.unspecified as u32,
            Flags:       0,
        }
        let rawSwap1: *void = NULL
        factory.CreateSwapChainForHwnd(
            cmdQueue as i64,
            hwnd as i64,
            &scDesc,
            NULL,
            0 as i64,
            &rawSwap1
        )
        // Promote IDXGISwapChain1 → IDXGISwapChain3 via QueryInterface.
        let swapChainBase = rawSwap1 as IUnknown
        let rawSwap3: *void = NULL
        swapChainBase.QueryInterface(&iidSwapChain3, &rawSwap3)
        let swapChain = rawSwap3 as IDXGISwapChain3
        let _ = swapChainBase.Release()

        // ── RTV descriptor heap ────────────────────────────────────
        let rtvHeapDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.rtv as u32,
            NumDescriptors: 2,
            Flags:          D3D12DescriptorHeapFlags.none as u32,
            NodeMask:       0,
        }
        let rawRtvHeap: *void = NULL
        device.CreateDescriptorHeap(&rtvHeapDesc, IID_ID3D12DescriptorHeap, &rawRtvHeap)
        let rtvHeap = rawRtvHeap as ID3D12DescriptorHeap
        let rtvDescriptorSize = device.GetDescriptorHandleIncrementSize(
            D3D12DescriptorHeapType.rtv as u32
        )

        let rtvBase = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: 0 }
        rtvHeap.GetCPUDescriptorHandleForHeapStart(&rtvBase)

        // Back buffers + RTVs
        let rawBackBuf0: *void = NULL
        swapChain.GetBuffer(0, &iidResource, &rawBackBuf0)
        let backBuf0 = rawBackBuf0 as ID3D12Resource
        device.CreateRenderTargetView(
            backBuf0 as i64,
            NULL,
            D3D12_CPU_DESCRIPTOR_HANDLE { ptr: rtvBase.ptr }
        )
        let rawBackBuf1: *void = NULL
        swapChain.GetBuffer(1, &iidResource, &rawBackBuf1)
        let backBuf1 = rawBackBuf1 as ID3D12Resource
        device.CreateRenderTargetView(
            backBuf1 as i64,
            NULL,
            D3D12_CPU_DESCRIPTOR_HANDLE { ptr: rtvBase.ptr + rtvDescriptorSize as u64 }
        )

        // ── Command allocator ──────────────────────────────────────
        let rawAlloc: *void = NULL
        device.CreateCommandAllocator(
            D3D12CommandListType.direct as u32,
            IID_ID3D12CommandAllocator,
            &rawAlloc
        )
        let allocator = rawAlloc as ID3D12CommandAllocator

        // ── Empty root signature ───────────────────────────────────
        let rsDesc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters:     0,
            _pad0:             0,
            pParameters:       NULL,
            NumStaticSamplers: 0,
            _pad1:             0,
            pStaticSamplers:   NULL,
            Flags:             D3D12RootSignatureFlags.allowInputAssemblerInputLayout as u32,
            _pad2:             0,
        }
        let rawSigBlob:   *void = NULL
        let rawSigErrBlob: *void = NULL
        D3D12SerializeRootSignature(
            &rsDesc as *const void,
            D3D12RootSignatureVersion.version_1_0 as u32,
            &rawSigBlob,
            &rawSigErrBlob
        )
        let sigBlob = rawSigBlob as ID3DBlob
        let rawRootSig: *void = NULL
        device.CreateRootSignature(
            0,
            sigBlob.GetBufferPointer() as *const void,
            sigBlob.GetBufferSize(),
            IID_ID3D12RootSignature,
            &rawRootSig
        )
        let rootSig = rawRootSig as ID3D12RootSignature
        let _ = sigBlob.Release()

        // ── HLSL inline source ─────────────────────────────────────
        let hlsl = "struct PSInput { float4 position : SV_POSITION; float4 color : COLOR; }; PSInput VSMain(float3 position : POSITION, float4 color : COLOR) { PSInput r; r.position = float4(position, 1.0); r.color = color; return r; } float4 PSMain(PSInput input) : SV_TARGET { return input.color; }"
        let hlslLen   = hlsl.length
        let hlslCstr  = cstrFromString(hlsl)
        let srcName   = cstrFromString("triangle.hlsl")
        let vsEntry   = cstrFromString("VSMain")
        let psEntry   = cstrFromString("PSMain")
        let vsProfile = cstrFromString("vs_5_0")
        let psProfile = cstrFromString("ps_5_0")

        let rawVs:    *void = NULL
        let rawVsErr: *void = NULL
        D3DCompile(
            hlslCstr as *const void, hlslLen as u64,
            srcName,
            NULL as *const void as *const void,  // placeholder: zero pDefines
            NULL,
            vsEntry, vsProfile,
            0, 0,
            &rawVs as **void as **ID3DBlob,
            &rawVsErr as **void as **ID3DBlob
        )
        let vsBlob = rawVs as ID3DBlob

        let rawPs:    *void = NULL
        let rawPsErr: *void = NULL
        D3DCompile(
            hlslCstr as *const void, hlslLen as u64,
            srcName,
            NULL as *const void as *const void,
            NULL,
            psEntry, psProfile,
            0, 0,
            &rawPs as **void as **ID3DBlob,
            &rawPsErr as **void as **ID3DBlob
        )
        let psBlob = rawPs as ID3DBlob

        // ── Input layout ──────────────────────────────────────────
        let posSem = cstrFromString("POSITION")
        let colSem = cstrFromString("COLOR")
        let inputElems: D3D12_INPUT_ELEMENT_DESC[2] = [
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName:         posSem,
                SemanticIndex:        0,
                Format:               DxgiFormat.r32g32b32_float as u32,
                InputSlot:            0,
                AlignedByteOffset:    0,
                InputSlotClass:       D3D12InputClassification.perVertexData as u32,
                InstanceDataStepRate: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName:         colSem,
                SemanticIndex:        0,
                Format:               DxgiFormat.r32g32b32a32_float as u32,
                InputSlot:            0,
                AlignedByteOffset:    12,
                InputSlotClass:       D3D12InputClassification.perVertexData as u32,
                InstanceDataStepRate: 0,
            },
        ]

        // ── Graphics PSO ──────────────────────────────────────────
        let psoDesc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: rootSig as i64,
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vsBlob.GetBufferPointer() as *const void,
                BytecodeLength:  vsBlob.GetBufferSize(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: psBlob.GetBufferPointer() as *const void,
                BytecodeLength:  psBlob.GetBufferSize(),
            },
            DS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            HS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            GS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            StreamOutput: D3D12_STREAM_OUTPUT_DESC {
                pSODeclaration: NULL, NumEntries: 0,
                pBufferStrides: NULL, NumStrides: 0,
                RasterizedStream: 0,
            },
            BlendState: defaultBlendDesc(),
            SampleMask: 0xFFFFFFFF as u32,
            RasterizerState: defaultRasterizerDesc(),
            DepthStencilState: defaultDepthStencilDesc(),
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: inputElems,
                NumElements:        2,
                _pad0:              0,
            },
            IBStripCutValue:       D3D12IndexBufferStripCutValue.disabled as u32,
            PrimitiveTopologyType: D3D12PrimitiveTopologyType.triangle as u32,
            RTVFormat0: DxgiFormat.r8g8b8a8_unorm as u32,
            RTVFormat1: 0, RTVFormat2: 0, RTVFormat3: 0,
            RTVFormat4: 0, RTVFormat5: 0, RTVFormat6: 0, RTVFormat7: 0,
            NumRenderTargets: 1,
            DSVFormat:        0,
            SampleDesc:       DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            NodeMask:         0,
            CachedPSO:        D3D12_CACHED_PIPELINE_STATE { pCachedBlob: NULL, CachedBlobSizeInBytes: 0 },
            Flags:            D3D12PipelineStateFlags.none as u32,
            _pad0:            0,
        }
        let rawPso: *void = NULL
        device.CreateGraphicsPipelineState(
            &psoDesc,
            IID_ID3D12PipelineState,
            &rawPso
        )
        let pso = rawPso as ID3D12PipelineState

        // ── Command list (immediately closed; reset each frame) ───
        let rawList: *void = NULL
        device.CreateCommandList(
            0,
            D3D12CommandListType.direct as u32,
            allocator as i64,
            pso as i64,
            IID_ID3D12GraphicsCommandList,
            &rawList
        )
        let cmdList = rawList as ID3D12GraphicsCommandList
        let _ = cmdList.Close()

        // ── Vertex buffer (upload heap, 3 vertices) ───────────────
        let vbSize: u64 = 84   // 3 * (12 + 16)
        let uploadHeap = D3D12_HEAP_PROPERTIES {
            Type:                 D3D12HeapType.upload as u32,
            CPUPageProperty:      D3D12CpuPageProperty.unknown as u32,
            MemoryPoolPreference: D3D12MemoryPool.unknown as u32,
            CreationNodeMask:     1,
            VisibleNodeMask:      1,
        }
        let vbDesc = D3D12_RESOURCE_DESC {
            Dimension:        D3D12ResourceDimension.buffer as u32,
            Alignment:        0,
            Width:            vbSize,
            Height:           1,
            DepthOrArraySize: 1,
            MipLevels:        1,
            Format:           0,  // DxgiFormat.unknown — buffers carry no typed format
            SampleDesc:       DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout:           D3D12TextureLayout.rowMajor as u32,
            Flags:            D3D12ResourceFlags.none as u32,
        }
        let rawVb: *void = NULL
        device.CreateCommittedResource(
            &uploadHeap,
            D3D12HeapMiscFlags.none as u32,
            &vbDesc,
            D3D12ResourceStates.genericRead as u32,
            NULL,
            IID_ID3D12Resource,
            &rawVb
        )
        let vb = rawVb as ID3D12Resource

        // Write the triangle (3 vertices, position + color).
        // Numeric suffix `_f32` is required — bare `0.0` is f64 and
        // the writeF32 host helper rejects the wider operand.
        let mapped: *void = NULL
        let readNone = D3D12_RANGE { Begin: 0, End: 0 }
        vb.Map(0, &readNone, &mapped)
        // v0 — top, red
        writeF32(mapped,  0,  0.0_f32)
        writeF32(mapped,  4,  0.25_f32)
        writeF32(mapped,  8,  0.0_f32)
        writeF32(mapped, 12,  1.0_f32)
        writeF32(mapped, 16,  0.0_f32)
        writeF32(mapped, 20,  0.0_f32)
        writeF32(mapped, 24,  1.0_f32)
        // v1 — bottom right, green
        writeF32(mapped, 28,  0.25_f32)
        writeF32(mapped, 32, -0.25_f32)
        writeF32(mapped, 36,  0.0_f32)
        writeF32(mapped, 40,  0.0_f32)
        writeF32(mapped, 44,  1.0_f32)
        writeF32(mapped, 48,  0.0_f32)
        writeF32(mapped, 52,  1.0_f32)
        // v2 — bottom left, blue
        writeF32(mapped, 56, -0.25_f32)
        writeF32(mapped, 60, -0.25_f32)
        writeF32(mapped, 64,  0.0_f32)
        writeF32(mapped, 68,  0.0_f32)
        writeF32(mapped, 72,  0.0_f32)
        writeF32(mapped, 76,  1.0_f32)
        writeF32(mapped, 80,  1.0_f32)
        vb.Unmap(0, NULL)

        let vbView = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: vb.GetGPUVirtualAddress(),
            SizeInBytes:    vbSize as u32,
            StrideInBytes:  28,
        }

        // ── Fence ─────────────────────────────────────────────────
        let rawFence: *void = NULL
        device.CreateFence(0, 0, IID_ID3D12Fence, &rawFence)
        let fence = rawFence as ID3D12Fence
        let fenceEvent = CreateEventA(NULL, 0, 0, NULL)
        let fenceValue: u64 = 1

        // ── Viewport + scissor (fixed) ────────────────────────────
        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0, TopLeftY: 0.0,
            Width:    width  as f32,
            Height:   height as f32,
            MinDepth: 0.0, MaxDepth: 1.0,
        }
        let scissor = D3D12_RECT {
            left: 0, top: 0,
            right:  width  as i32,
            bottom: height as i32,
        }

        // ── Render loop ───────────────────────────────────────────
        let msg = MSG {
            hwnd:     NULL,
            message:  0,
            wParam:   0,
            lParam:   0,
            time:     0,
            pt:       POINT { x: 0, y: 0 },
            lPrivate: 0,
        }
        let running: i32 = 1
        loop {
            // Drain the queue first; the message pump exits cleanly
            // on WM_QUIT (GetMessage returns 0).
            let r = GetMessageA(msg, NULL, 0, 0)
            if r <= 0 { break }
            TranslateMessage(msg)
            let _ = DispatchMessageA(msg)

            // Record one frame's commands per message tick. Real
            // games drive this from idle / WM_PAINT; this is the
            // simplest "draw something" loop.
            let _ = allocator.Reset()
            let _ = cmdList.Reset(allocator as i64, pso as i64)
            cmdList.SetGraphicsRootSignature(rootSig as i64)
            cmdList.RSSetViewports(1, &viewport)
            cmdList.RSSetScissorRects(1, &scissor)

            let frameIdx = swapChain.GetCurrentBackBufferIndex()
            let backBuf = if frameIdx == 0 as u32 { backBuf0 } else { backBuf1 }
            let rtvHandle = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: rtvBase.ptr + (frameIdx as u64) * (rtvDescriptorSize as u64),
            }

            let toRT = D3D12_RESOURCE_BARRIER {
                Type:        D3D12ResourceBarrierType.transition as u32,
                Flags:       D3D12ResourceBarrierFlags.none as u32,
                pResource:   backBuf as i64 as *ID3D12Resource,
                Subresource: 0xFFFFFFFF as u32,
                StateBefore: D3D12ResourceStates.present as u32,
                StateAfter:  D3D12ResourceStates.renderTarget as u32,
                _pad:        0,
            }
            cmdList.ResourceBarrier(1, &toRT)

            cmdList.OMSetRenderTargets(1, &rtvHandle, 0, NULL)
            let clearColor: f32[] = [0.0, 0.2, 0.4, 1.0]
            cmdList.ClearRenderTargetView(rtvHandle, clearColor, 0, NULL)
            cmdList.IASetPrimitiveTopology(D3DPrimitiveTopologyValue.triangleList as u32)
            cmdList.IASetVertexBuffers(0, 1, &vbView)
            cmdList.DrawInstanced(3, 1, 0, 0)

            let toPresent = D3D12_RESOURCE_BARRIER {
                Type:        D3D12ResourceBarrierType.transition as u32,
                Flags:       D3D12ResourceBarrierFlags.none as u32,
                pResource:   backBuf as i64 as *ID3D12Resource,
                Subresource: 0xFFFFFFFF as u32,
                StateBefore: D3D12ResourceStates.renderTarget as u32,
                StateAfter:  D3D12ResourceStates.present as u32,
                _pad:        0,
            }
            cmdList.ResourceBarrier(1, &toPresent)
            let _ = cmdList.Close()

            // Execute + present + wait. `&listPtr as *const *void`
            // forms an array-of-one of `ID3D12CommandList*`.
            let listPtr: i64 = cmdList as i64
            cmdQueue.ExecuteCommandLists(1, &listPtr as *const *void)
            let _ = swapChain.Present(1, 0)

            let _ = cmdQueue.Signal(fence as i64, fenceValue)
            if fence.GetCompletedValue() < fenceValue {
                let _ = fence.SetEventOnCompletion(fenceValue, fenceEvent as i64)
                let _ = WaitForSingleObject(fenceEvent, 0xFFFFFFFF as u32)
            }
            fenceValue = fenceValue + 1 as u64
        }

        // ── Cleanup ───────────────────────────────────────────────
        let _ = vb.Release()
        let _ = pso.Release()
        let _ = rootSig.Release()
        let _ = vsBlob.Release()
        let _ = psBlob.Release()
        let _ = cmdList.Release()
        let _ = allocator.Release()
        let _ = backBuf1.Release()
        let _ = backBuf0.Release()
        let _ = rtvHeap.Release()
        let _ = swapChain.Release()
        let _ = cmdQueue.Release()
        let _ = device.Release()
        let _ = factory.Release()
        let _ = fence.Release()
        let _ = CloseHandle(fenceEvent)

        freeCstr(posSem)
        freeCstr(colSem)
        freeCstr(vsEntry)
        freeCstr(psEntry)
        freeCstr(vsProfile)
        freeCstr(psProfile)
        freeCstr(srcName)
        freeCstr(hlslCstr)
        freeCstr(title)
        freeCstr(className)
    }

    // ── Default state helpers ─────────────────────────────────────

    /// Single render target, no blending — straight write of the
    /// pixel shader output.
    pub fn defaultRTBlendDesc(): D3D12_RENDER_TARGET_BLEND_DESC {
        D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable:           0,
            LogicOpEnable:         0,
            SrcBlend:              D3D12Blend.one as u32,
            DestBlend:             D3D12Blend.zero as u32,
            BlendOp:               D3D12BlendOp.add as u32,
            SrcBlendAlpha:         D3D12Blend.one as u32,
            DestBlendAlpha:        D3D12Blend.zero as u32,
            BlendOpAlpha:          D3D12BlendOp.add as u32,
            LogicOp:               D3D12LogicOp.noop as u32,
            RenderTargetWriteMask: D3D12ColorWriteEnable.all as u8,
            _pad0: 0, _pad1: 0,
        }
    }

    pub fn defaultBlendDesc(): D3D12_BLEND_DESC {
        let rt = defaultRTBlendDesc()
        D3D12_BLEND_DESC {
            AlphaToCoverageEnable:  0,
            IndependentBlendEnable: 0,
            RenderTarget0: rt,
            RenderTarget1: rt,
            RenderTarget2: rt,
            RenderTarget3: rt,
            RenderTarget4: rt,
            RenderTarget5: rt,
            RenderTarget6: rt,
            RenderTarget7: rt,
        }
    }

    pub fn defaultRasterizerDesc(): D3D12_RASTERIZER_DESC {
        D3D12_RASTERIZER_DESC {
            FillMode:              D3D12FillMode.solid as u32,
            CullMode:              D3D12CullMode.back as u32,
            FrontCounterClockwise: 0,
            DepthBias:             0,
            DepthBiasClamp:        0.0,
            SlopeScaledDepthBias:  0.0,
            DepthClipEnable:       1,
            MultisampleEnable:     0,
            AntialiasedLineEnable: 0,
            ForcedSampleCount:     0,
            ConservativeRaster:    0,
        }
    }

    pub fn defaultDepthStencilDesc(): D3D12_DEPTH_STENCIL_DESC {
        D3D12_DEPTH_STENCIL_DESC {
            DepthEnable:      0,
            DepthWriteMask:   D3D12DepthWriteMask.all as u32,
            DepthFunc:        D3D12ComparisonFunc.less as u32,
            StencilEnable:    0,
            StencilReadMask:  0xFF as u8,
            StencilWriteMask: 0xFF as u8,
            _pad0:            0,
            FrontFace: D3D12_DEPTH_STENCILOP_DESC {
                StencilFailOp:      D3D12StencilOp.keep as u32,
                StencilDepthFailOp: D3D12StencilOp.keep as u32,
                StencilPassOp:      D3D12StencilOp.keep as u32,
                StencilFunc:        D3D12ComparisonFunc.always as u32,
            },
            BackFace: D3D12_DEPTH_STENCILOP_DESC {
                StencilFailOp:      D3D12StencilOp.keep as u32,
                StencilDepthFailOp: D3D12StencilOp.keep as u32,
                StencilPassOp:      D3D12StencilOp.keep as u32,
                StencilFunc:        D3D12ComparisonFunc.always as u32,
            },
        }
    }
}

run()
