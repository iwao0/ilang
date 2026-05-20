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
    RtlMoveMemory,
    CreateDXGIFactory2,
    DXGI_SAMPLE_DESC, DXGI_SWAP_CHAIN_DESC1,
    DxgiFormat, DxgiUsage, DxgiSwapEffect, DxgiAlphaMode, DxgiScaling,
    IDXGIFactory4, IDXGISwapChain1,
}

use directx12 {
    D3D12CreateDevice, D3D12SerializeRootSignature, D3D12GetDebugInterface,
    ID3D12Debug, IID_ID3D12Debug,
    ID3D12InfoQueue, IID_ID3D12InfoQueue,
    D3DFeatureLevel,
    GUID,
    IUnknown, ID3D12Device, ID3D12CommandQueue, ID3D12CommandAllocator,
    ID3D12CommandList, ID3D12GraphicsCommandList, ID3D12Resource, ID3D12Fence,
    ID3D12DescriptorHeap, ID3D12RootSignature, ID3D12PipelineState,
    ID3DBlob,
    IID_ID3D12Device, IID_ID3D12CommandQueue, IID_ID3D12CommandAllocator,
    IID_ID3D12GraphicsCommandList, IID_ID3D12Resource, IID_ID3D12Fence,
    IID_ID3D12DescriptorHeap, IID_ID3D12RootSignature, IID_ID3D12PipelineState,
    IID_IDXGIFactory4,
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
    // Triangle vertex layout — position (xyz) + color (rgba),
    // 28 bytes total, matches the input-element desc below.
    struct Vertex {
        pos:   f32[3]
        color: f32[4]
    }

    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == WindowMessage.destroy {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as HWND, msg, wParam, lParam)
    }

    pub fn run() {
        console.log("[dx12] start")
        // ── Constants ──────────────────────────────────────────────
        let width:  i32 = 800
        let height: i32 = 600

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
            hInstance:     hInstance as HINSTANCE,
            hIcon:         NULL,
            hCursor:       NULL,
            hbrBackground: NULL,
            lpszMenuName:  NULL,
            lpszClassName: className,
            hIconSm:       NULL,
        }
        RegisterClassExA(&wc)

        let hwnd = CreateWindowExA(
            0,
            className,
            title,
            WindowStyle.overlappedWindow,
            120, 120,
            width + 16, height + 39,
            NULL, NULL,
            hInstance as HINSTANCE,
            NULL
        )
        console.log("[dx12] hwnd =", hwnd as i64)
        ShowWindow(hwnd, ShowCmd.show)
        UpdateWindow(hwnd)
        console.log("[dx12] window shown")

        // ── D3D12 debug layer ───────────────────────────────────────
        // Must be enabled BEFORE D3D12CreateDevice for validation
        // messages to attach. Output lands on the debugger / event
        // viewer; run under `windbg` or with DebugView to read it.
        let rawDebug: *void = NULL
        let rcDbg = D3D12GetDebugInterface(&IID_ID3D12Debug, &rawDebug)
        console.log("[dx12] D3D12GetDebugInterface rc =", rcDbg)
        if rcDbg == 0 {
            let dbg = rawDebug as ID3D12Debug
            dbg.EnableDebugLayer()
            console.log("[dx12] debug layer enabled")
        }

        // ── DXGI factory ───────────────────────────────────────────
        let rawFactory: *void = NULL
        let rcFactory = CreateDXGIFactory2(0, &IID_IDXGIFactory4, &rawFactory)
        console.log("[dx12] CreateDXGIFactory2 rc =", rcFactory)
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
            &IID_ID3D12Device,
            &rawDevice
        )
        console.log("[dx12] D3D12CreateDevice rc =", rcDevice)
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
        let rcQueue = device.CreateCommandQueue(&queueDesc, &IID_ID3D12CommandQueue, &rawQueue)
        console.log("[dx12] CreateCommandQueue rc =", rcQueue)
        let cmdQueue = rawQueue as ID3D12CommandQueue

        // ── Swap chain ─────────────────────────────────────────────
        let scDesc = DXGI_SWAP_CHAIN_DESC1 {
            Width:       width as u32,
            Height:      height as u32,
            Format:      DxgiFormat.r8g8b8a8_unorm,
            Stereo:      0,
            SampleDesc:  DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DxgiUsage.renderTargetOutput,
            BufferCount: 2,
            Scaling:     DxgiScaling.stretch,
            SwapEffect:  DxgiSwapEffect.flipDiscard,
            AlphaMode:   DxgiAlphaMode.unspecified,
            Flags:       0,
        }
        let rawSwap1: *void = NULL
        let rcSwap = factory.CreateSwapChainForHwnd(
            &cmdQueue,
            hwnd,
            &scDesc,
            NULL,
            NULL,
            &rawSwap1
        )
        console.log("[dx12] CreateSwapChainForHwnd rc =", rcSwap)
        // Promote IDXGISwapChain1 → IDXGISwapChain3 via QueryInterface.
        // Skip the IDXGISwapChain3 QueryInterface — the back buffer
        // index can be tracked locally with a `% BufferCount` counter,
        // so the base IDXGISwapChain1 interface is enough.
        let swapChain = rawSwap1 as IDXGISwapChain1

        // ── RTV descriptor heap ────────────────────────────────────
        let rtvHeapDesc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type:           D3D12DescriptorHeapType.rtv,
            NumDescriptors: 2,
            Flags:          D3D12DescriptorHeapFlags.none,
            NodeMask:       0,
        }
        let rawRtvHeap: *void = NULL
        let rcHeap = device.CreateDescriptorHeap(&rtvHeapDesc, &IID_ID3D12DescriptorHeap, &rawRtvHeap)
        console.log("[dx12] CreateDescriptorHeap rc =", rcHeap)
        let rtvHeap = rawRtvHeap as ID3D12DescriptorHeap
        let rtvDescriptorSize = device.GetDescriptorHandleIncrementSize(
            D3D12DescriptorHeapType.rtv
        )

        let rtvBase = D3D12_CPU_DESCRIPTOR_HANDLE { ptr: 0 }
        rtvHeap.GetCPUDescriptorHandleForHeapStart(&rtvBase)

        // Back buffers + RTVs
        let rawBackBuf0: *void = NULL
        let rcBuf0 = swapChain.GetBuffer(0, &IID_ID3D12Resource, &rawBackBuf0)
        console.log("[dx12] GetBuffer(0) rc =", rcBuf0)
        let backBuf0 = rawBackBuf0 as ID3D12Resource
        device.CreateRenderTargetView(
            &backBuf0,
            NULL,
            rtvBase.ptr
        )
        let rawBackBuf1: *void = NULL
        swapChain.GetBuffer(1, &IID_ID3D12Resource, &rawBackBuf1)
        let backBuf1 = rawBackBuf1 as ID3D12Resource
        device.CreateRenderTargetView(
            &backBuf1,
            NULL,
            rtvBase.ptr + rtvDescriptorSize as u64
        )

        // ── Command allocator ──────────────────────────────────────
        let rawAlloc: *void = NULL
        device.CreateCommandAllocator(
            D3D12CommandListType.direct,
            &IID_ID3D12CommandAllocator,
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
            // 0x1 = ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT; required so
            // the IA stage can consume vertex/index buffers.
            Flags:             1,
            _pad2:             0,
        }
        let rawSigBlob:   *void = NULL
        let rawSigErrBlob: *void = NULL
        let rcSerial = D3D12SerializeRootSignature(
            &rsDesc,
            D3D12RootSignatureVersion.version_1_0,
            &rawSigBlob,
            &rawSigErrBlob
        )
        console.log("[dx12] D3D12SerializeRootSignature rc =", rcSerial)
        let sigBlob = rawSigBlob as ID3DBlob
        let blobPtr = sigBlob.GetBufferPointer()
        let blobSize = sigBlob.GetBufferSize()
        let rawRootSig: *void = NULL
        let rcRoot = device.CreateRootSignature(
            0,
            blobPtr,
            blobSize,
            &IID_ID3D12RootSignature,
            &rawRootSig
        )
        console.log("[dx12] CreateRootSignature rc =", rcRoot)
        if rcRoot != 0 {
            // Failure path: drain D3D12's info queue so we see exactly
            // what the debug layer rejected. Empty in the happy path
            // so we don't QueryInterface every frame.
            let rawInfoQueue: *void = NULL
            let rcQi = device.QueryInterface(&IID_ID3D12InfoQueue, &rawInfoQueue)
            if rcQi == 0 {
                let infoQueue = rawInfoQueue as ID3D12InfoQueue
                let numMsgs = infoQueue.GetNumStoredMessages()
                console.log("[dx12] info queue messages =", numMsgs)
                // Scratch buffer for GetMessage. ilang has no fixed-size
                // array repeat-init, so build a 4 KiB u8[] by push.
                let scratch: u8[] = []
                let fill: u64 = 0
                loop {
                    if fill >= 4096 { break }
                    scratch.push(0)
                    fill = fill + 1
                }
                let i: u64 = 0
                loop {
                    if i >= numMsgs { break }
                    let lenSlot: u64 = 4096
                    let rcMsg = infoQueue.GetMessage(i, scratch, &lenSlot)
                    if rcMsg != 0 {
                        console.log("[dx12]  msg", i, "rc =", rcMsg)
                    } else {
                        // D3D12_MESSAGE { Category u32, Severity u32,
                        //                 ID u32, pad u32,
                        //                 pDescription ptr, Length u64 }
                        let pDesc = readU64(scratch, 16)
                        let descLen = readU64(scratch, 24)
                        let descStr = stringFromCstr(pDesc as *const char)
                        console.log("[dx12]  msg", i, "len=", descLen, "text=", descStr)
                    }
                    i = i + 1
                }
                let _ = infoQueue.Release()
            }
        }
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
        let rcVs = D3DCompile(
            hlslCstr, hlslLen as u64,
            srcName,
            NULL,  // placeholder: zero pDefines
            NULL,
            vsEntry, vsProfile,
            0, 0,
            &rawVs as **ID3DBlob,
            &rawVsErr as **ID3DBlob
        )
        console.log("[dx12] D3DCompile VS rc =", rcVs)
        let vsBlob = rawVs as ID3DBlob

        let rawPs:    *void = NULL
        let rawPsErr: *void = NULL
        let rcPs = D3DCompile(
            hlslCstr, hlslLen as u64,
            srcName,
            NULL,
            NULL,
            psEntry, psProfile,
            0, 0,
            &rawPs as **ID3DBlob,
            &rawPsErr as **ID3DBlob
        )
        console.log("[dx12] D3DCompile PS rc =", rcPs)
        let psBlob = rawPs as ID3DBlob

        // ── Input layout ──────────────────────────────────────────
        let posSem = cstrFromString("POSITION")
        let colSem = cstrFromString("COLOR")
        let inputElems: D3D12_INPUT_ELEMENT_DESC[2] = [
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName:         posSem,
                SemanticIndex:        0,
                Format:               DxgiFormat.r32g32b32_float,
                InputSlot:            0,
                AlignedByteOffset:    0,
                InputSlotClass:       D3D12InputClassification.perVertexData,
                InstanceDataStepRate: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName:         colSem,
                SemanticIndex:        0,
                Format:               DxgiFormat.r32g32b32a32_float,
                InputSlot:            0,
                AlignedByteOffset:    12,
                InputSlotClass:       D3D12InputClassification.perVertexData,
                InstanceDataStepRate: 0,
            },
        ]
        // ── Graphics PSO ──────────────────────────────────────────
        let psoDesc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: rootSig as i64,
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vsBlob.GetBufferPointer(),
                BytecodeLength:  vsBlob.GetBufferSize(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: psBlob.GetBufferPointer(),
                BytecodeLength:  psBlob.GetBufferSize(),
            },
            DS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            HS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            GS: D3D12_SHADER_BYTECODE { pShaderBytecode: NULL, BytecodeLength: 0 },
            StreamOutput: D3D12_STREAM_OUTPUT_DESC {
                pSODeclaration: NULL, NumEntries: 0,
                _pad0: 0,
                pBufferStrides: NULL, NumStrides: 0,
                RasterizedStream: 0,
            },
            BlendState: defaultBlendDesc(),
            SampleMask: 0xFFFFFFFF,
            RasterizerState: defaultRasterizerDesc(),
            DepthStencilState: defaultDepthStencilDesc(),
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: inputElems,
                NumElements:        2,
                _pad0:              0,
            },
            IBStripCutValue:       D3D12IndexBufferStripCutValue.disabled,
            PrimitiveTopologyType: D3D12PrimitiveTopologyType.triangle,
            NumRenderTargets: 1,
            RTVFormat0: DxgiFormat.r8g8b8a8_unorm,
            RTVFormat1: 0, RTVFormat2: 0, RTVFormat3: 0,
            RTVFormat4: 0, RTVFormat5: 0, RTVFormat6: 0, RTVFormat7: 0,
            DSVFormat:        0,
            SampleDesc:       DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            NodeMask:         0,
            CachedPSO:        D3D12_CACHED_PIPELINE_STATE { pCachedBlob: NULL, CachedBlobSizeInBytes: 0 },
            Flags:            D3D12PipelineStateFlags.none,
            _pad0:            0,
        }
        let rawPso: *void = NULL
        let rcPso = device.CreateGraphicsPipelineState(
            &psoDesc,
            &IID_ID3D12PipelineState,
            &rawPso
        )
        console.log("[dx12] CreateGraphicsPipelineState rc =", rcPso)
        if rcPso != 0 {
            // Same info-queue drain pattern as CreateRootSignature
            // — pulls D3D12's validation messages so we see exactly
            // which PSO desc field it rejected.
            let rawIq: *void = NULL
            let rcQi = device.QueryInterface(&IID_ID3D12InfoQueue, &rawIq)
            if rcQi == 0 {
                let iq = rawIq as ID3D12InfoQueue
                let n = iq.GetNumStoredMessages()
                console.log("[dx12] PSO info queue messages =", n)
                let buf: u8[] = []
                let f: u64 = 0
                loop {
                    if f >= 8192 { break }
                    buf.push(0)
                    f = f + 1
                }
                let i: u64 = 0
                loop {
                    if i >= n { break }
                    let lenSlot: u64 = 8192
                    let rcM = iq.GetMessage(i, buf, &lenSlot)
                    if rcM == 0 {
                        let pd  = readU64(buf, 16)
                        let dl  = readU64(buf, 24)
                        let txt = stringFromCstr(pd as *const char)
                        console.log("[dx12]  pso msg", i, "len=", dl, "text=", txt)
                    }
                    i = i + 1
                }
                let _ = iq.Release()
            }
        }
        let pso = rawPso as ID3D12PipelineState

        // ── Command list (immediately closed; reset each frame) ───
        let rawList: *void = NULL
        let rcList = device.CreateCommandList(
            0,
            D3D12CommandListType.direct,
            &allocator,
            &pso,
            &IID_ID3D12GraphicsCommandList,
            &rawList
        )
        console.log("[dx12] CreateCommandList rc =", rcList)
        let cmdList = rawList as ID3D12GraphicsCommandList
        let _ = cmdList.Close()

        // ── Vertex buffer (upload heap, 3 vertices) ───────────────
        let vbSize: u64 = 84   // 3 * (12 + 16)
        let uploadHeap = D3D12_HEAP_PROPERTIES {
            Type:                 D3D12HeapType.upload,
            CPUPageProperty:      D3D12CpuPageProperty.unknown,
            MemoryPoolPreference: D3D12MemoryPool.unknown,
            CreationNodeMask:     1,
            VisibleNodeMask:      1,
        }
        let vbDesc = D3D12_RESOURCE_DESC {
            Dimension:        D3D12ResourceDimension.buffer,
            Alignment:        0,
            Width:            vbSize,
            Height:           1,
            DepthOrArraySize: 1,
            MipLevels:        1,
            Format:           0,  // DxgiFormat.unknown — buffers carry no typed format
            SampleDesc:       DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout:           D3D12TextureLayout.rowMajor,
            Flags:            D3D12ResourceFlags.none,
        }
        let rawVb: *void = NULL
        let rcVb = device.CreateCommittedResource(
            &uploadHeap,
            D3D12HeapMiscFlags.none,
            &vbDesc,
            D3D12ResourceStates.genericRead,
            NULL,
            &IID_ID3D12Resource,
            &rawVb
        )
        console.log("[dx12] CreateCommittedResource rc =", rcVb)
        let vb = rawVb as ID3D12Resource

        // Write the triangle (3 vertices, position + color).
        let vertices: Vertex[3] = [
            Vertex { pos: [ 0.0,   0.25, 0.0], color: [1.0, 0.0, 0.0, 1.0] },
            Vertex { pos: [ 0.25, -0.25, 0.0], color: [0.0, 1.0, 0.0, 1.0] },
            Vertex { pos: [-0.25, -0.25, 0.0], color: [0.0, 0.0, 1.0, 1.0] },
        ]
        let mapped: *void = NULL
        let readNone = D3D12_RANGE { Begin: 0, End: 0 }
        vb.Map(0, &readNone, &mapped)
        RtlMoveMemory(mapped, vertices, vbSize)
        vb.Unmap(0, NULL)

        let vbView = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: vb.GetGPUVirtualAddress(),
            SizeInBytes:    vbSize as u32,
            StrideInBytes:  28,
        }

        // ── Fence ─────────────────────────────────────────────────
        let rawFence: *void = NULL
        let rcFence = device.CreateFence(0, 0, &IID_ID3D12Fence, &rawFence)
        console.log("[dx12] CreateFence rc =", rcFence)
        console.log("[dx12] entering message pump")
        let fence = rawFence as ID3D12Fence
        let fenceEvent = CreateEventA(NULL, 0, 0, NULL)
        let fenceValue: u64 = 1
        let frameCounter: u32 = 0

        // ── Viewport + scissor (fixed) ────────────────────────────
        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0, TopLeftY: 0.0,
            Width:    width  as f32,
            Height:   height as f32,
            MinDepth: 0.0, MaxDepth: 1.0,
        }
        let scissor = D3D12_RECT {
            left: 0, top: 0,
            right:  width,
            bottom: height,
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
            let r = GetMessageA(&msg, NULL, 0, 0)
            if r <= 0 { break }
            TranslateMessage(&msg)
            let _ = DispatchMessageA(&msg)

            // Record one frame's commands per message tick. Real
            // games drive this from idle / WM_PAINT; this is the
            // simplest "draw something" loop.
            let _ = allocator.Reset()
            let _ = cmdList.Reset(&allocator, &pso)
            cmdList.SetGraphicsRootSignature(&rootSig)
            cmdList.RSSetViewports(1, &viewport)
            cmdList.RSSetScissorRects(1, &scissor)

            let frameIdx = frameCounter % 2
            let backBuf = if frameIdx == 0 { backBuf0 } else { backBuf1 }
            let rtvHandle = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: rtvBase.ptr + (frameIdx as u64) * (rtvDescriptorSize as u64),
            }

            let toRT = D3D12_RESOURCE_BARRIER {
                Type:        D3D12ResourceBarrierType.transition,
                Flags:       D3D12ResourceBarrierFlags.none,
                pResource:   &backBuf,
                Subresource: 0xFFFFFFFF,
                StateBefore: D3D12ResourceStates.present,
                StateAfter:  D3D12ResourceStates.renderTarget,
                _pad:        0,
            }
            cmdList.ResourceBarrier(1, &toRT)

            cmdList.OMSetRenderTargets(1, &rtvHandle, 0, NULL)
            let clearColor: f32[] = [0.0, 0.2, 0.4, 1.0]
            cmdList.ClearRenderTargetView(rtvHandle.ptr, clearColor, 0, NULL)
            cmdList.IASetPrimitiveTopology(D3DPrimitiveTopologyValue.triangleList)
            cmdList.IASetVertexBuffers(0, 1, &vbView)
            cmdList.DrawInstanced(3, 1, 0, 0)

            let toPresent = D3D12_RESOURCE_BARRIER {
                Type:        D3D12ResourceBarrierType.transition,
                Flags:       D3D12ResourceBarrierFlags.none,
                pResource:   &backBuf,
                Subresource: 0xFFFFFFFF,
                StateBefore: D3D12ResourceStates.renderTarget,
                StateAfter:  D3D12ResourceStates.present,
                _pad:        0,
            }
            cmdList.ResourceBarrier(1, &toPresent)
            let _ = cmdList.Close()

            // Execute + present + wait. Array-of-one of
            // `ID3D12CommandList*`.
            let listPtr: *ID3D12CommandList = &cmdList as *ID3D12CommandList
            cmdQueue.ExecuteCommandLists(1, &listPtr)
            let _ = swapChain.Present(1, 0)

            let _ = cmdQueue.Signal(&fence, fenceValue)
            if fence.GetCompletedValue() < fenceValue {
                let _ = fence.SetEventOnCompletion(fenceValue, fenceEvent)
                let _ = WaitForSingleObject(fenceEvent, 0xFFFFFFFF)
            }
            fenceValue = fenceValue + 1
            frameCounter = frameCounter + 1
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
            SrcBlend:              D3D12Blend.one,
            DestBlend:             D3D12Blend.zero,
            BlendOp:               D3D12BlendOp.add,
            SrcBlendAlpha:         D3D12Blend.one,
            DestBlendAlpha:        D3D12Blend.zero,
            BlendOpAlpha:          D3D12BlendOp.add,
            LogicOp:               D3D12LogicOp.noop,
            RenderTargetWriteMask: D3D12ColorWriteEnable.all,
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
            FillMode:              D3D12FillMode.solid,
            CullMode:              D3D12CullMode.back,
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
            DepthWriteMask:   D3D12DepthWriteMask.all,
            DepthFunc:        D3D12ComparisonFunc.less,
            StencilEnable:    0,
            StencilReadMask:  0xFF,
            StencilWriteMask: 0xFF,
            _pad0:            0,
            FrontFace: D3D12_DEPTH_STENCILOP_DESC {
                StencilFailOp:      D3D12StencilOp.keep,
                StencilDepthFailOp: D3D12StencilOp.keep,
                StencilPassOp:      D3D12StencilOp.keep,
                StencilFunc:        D3D12ComparisonFunc.always,
            },
            BackFace: D3D12_DEPTH_STENCILOP_DESC {
                StencilFailOp:      D3D12StencilOp.keep,
                StencilDepthFailOp: D3D12StencilOp.keep,
                StencilPassOp:      D3D12StencilOp.keep,
                StencilFunc:        D3D12ComparisonFunc.always,
            },
        }
    }
}

run()
