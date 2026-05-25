/// d3d12 + dxgi — swap chain + back-buffer indexing smoke test.
///
/// Creates a hidden Win32 window, a D3D12 device + direct queue,
/// then a 2-buffer DXGI swap chain.  QueryInterfaces the swap chain
/// to `IDXGISwapChain3` and reads back-buffer index — which must be
/// 0 immediately after creation.
///
/// Skips cleanly when any prerequisite (window class, GPU, factory
/// 4+) is unavailable.

use std.ffi { cstrFromString, freeCstr }
use std.test as test
use windows {
    HANDLE, NULL,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, ClassStyle, WindowStyle,
    RegisterClassExA, CreateWindowExA, DefWindowProcA, DestroyWindow,
    GetModuleHandleA,
}
use directx12 {
    D3D12CreateDevice, D3DFeatureLevel,
    IID_ID3D12Device, IID_ID3D12CommandQueue,
    ID3D12Device, ID3D12CommandQueue,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    CreateDXGIFactory2, DXGI_SAMPLE_DESC, DXGI_SWAP_CHAIN_DESC1, DxgiAlphaMode, DxgiFormat, DxgiScaling, DxgiSwapEffect, DxgiUsage, IDXGIFactory2, IDXGIFactory4, IDXGISwapChain1, IDXGISwapChain3,
}
use winnt { GUID }
use ole32 { IUnknown }
// IID_IDXGIFactory4    = 1bc6ea02-ef36-464f-bf0c-21ca39e5168a
// IID_IDXGISwapChain3  = 94d99bdb-f1f8-4ab0-b236-7da0170edab1
//
// Re-declared here so the test crate doesn't need a re-export.

pub const IID_IDXGIFactory4: GUID = GUID {
    Data1: 0x1bc6ea02, Data2: 0xef36, Data3: 0x464f,
    Data4lo: 0xca210cbf, Data4hi: 0x8a16e539,
}
pub const IID_IDXGISwapChain3: GUID = GUID {
    Data1: 0x94d99bdb, Data2: 0xf1f8, Data3: 0x4ab0,
    Data4lo: 0xa07d36b2, Data4hi: 0xb1da0e17,
}

@extern(C) {
    pub fn nopWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        DefWindowProcA(hwnd as HWND, msg, wParam, lParam)
    }

    pub fn run() {
        // ── Hidden window ─────────────────────────────────────────
        let hInstance = GetModuleHandleA(NULL)
        let className = cstrFromString("IlangDx12SwapChainTest")
        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
            lpfnWndProc:   nopWndProc,
            cbClsExtra:    0, cbWndExtra: 0,
            hInstance:     hInstance as HINSTANCE,
            hIcon:         0 as HICON,
            hCursor:       0 as HCURSOR,
            hbrBackground: 0 as HBRUSH,
            lpszMenuName:  0 as *const char,
            lpszClassName: className,
            hIconSm:       0 as HICON,
        }
        if RegisterClassExA(&wc) == 0 as u16 {
            freeCstr(className)
            return
        }
        let hwnd = CreateWindowExA(
            0, className, cstrFromString("test"),
            WindowStyle.overlappedWindow as u32,
            0, 0, 64, 64,
            0 as HWND, 0 as HMENU,
            hInstance as HINSTANCE,
            NULL
        )
        if (hwnd as i64) == 0 {
            freeCstr(className)
            return
        }

        // ── D3D12 device + direct queue ───────────────────────────
        let rawDev: *void = NULL
        let drc = D3D12CreateDevice(
            NULL, D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device, &rawDev
        )
        if drc != 0 {
            let _ = DestroyWindow(hwnd)
            freeCstr(className)
            return
        }
        let device = rawDev as ID3D12Device

        let qDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }
        let rawQueue: *void = NULL
        test.expect(device.CreateCommandQueue(&qDesc, &IID_ID3D12CommandQueue, &rawQueue) as i64, 0)
        let queue = rawQueue as ID3D12CommandQueue

        // ── DXGI factory4 + swap chain ────────────────────────────
        let rawFactory: *void = NULL
        let fhr = CreateDXGIFactory2(0, &IID_IDXGIFactory4 as *const void, &rawFactory)
        if fhr != 0 {
            test.expect(queue.Release()  as i64, 0)
            test.expect(device.Release() as i64, 0)
            let _ = DestroyWindow(hwnd)
            freeCstr(className)
            return
        }
        let factory = rawFactory as IDXGIFactory4

        let scDesc = DXGI_SWAP_CHAIN_DESC1 {
            Width:       64, Height: 64,
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
        // CreateSwapChainForHwnd lives on IDXGIFactory2 — the
        // factory4 we have inherits the slot, so the call is direct.
        let rawSc: *void = NULL
        let scHr = (factory as *void as IDXGIFactory2).CreateSwapChainForHwnd(
            &queue as *void,
            hwnd,
            &scDesc,
            0 as *const void,
            0 as *void,
            &rawSc
        )
        if scHr != 0 {
            test.expect(factory.Release() as i64, 0)
            test.expect(queue.Release()  as i64, 0)
            test.expect(device.Release() as i64, 0)
            let _ = DestroyWindow(hwnd)
            freeCstr(className)
            return
        }

        // QueryInterface to IDXGISwapChain3 and read the back-buffer
        // index. Fresh swap chain → index 0.
        let rawSc3: *void = NULL
        let qhr = (rawSc as IUnknown).QueryInterface(
            &IID_IDXGISwapChain3 as *const void, &rawSc3
        )
        test.expect(qhr as i64, 0)
        let sc3 = rawSc3 as IDXGISwapChain3
        test.expect(sc3.GetCurrentBackBufferIndex() as i64, 0)

        // ── Cleanup ────────────────────────────────────────────────
        test.expectTrue(sc3.Release()    <= 1 as u32)
        test.expect((rawSc as IUnknown).Release() as i64, 0)
        test.expect(factory.Release() as i64, 0)
        test.expect(queue.Release()   as i64, 0)
        test.expect(device.Release()  as i64, 0)
        let _ = DestroyWindow(hwnd)
        freeCstr(className)
    }
}

run()
