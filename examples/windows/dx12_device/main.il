// 🎮 DirectX 12 device + COM-pipeline demo.
//
// Run:
//   ilang run examples/windows/dx12_device/main.il
//
// Builds the bottom half of a real D3D12 render setup with flat
// entry points plus typed COM-interface dispatch:
//
//   1. D3D12CreateDevice                          flat
//   2. ID3D12Device::CreateCommandQueue           @com interface
//   3. ID3D12CommandQueue::Release                @com interface
//   4. ID3D12Device::Release                      @com interface
//
// IIDs come from `bindings/directx12/d3d12_iids.il` as `pub const`
// values, so user code never has to retype the {189819f1-…} octets.
// COM dispatch comes from `bindings/directx12/d3d12_com.il`'s
// `@com pub interface IUnknown / ID3D12Device` — slot numbers
// follow the C++ COM ABI through `: Parent` chains, and method
// calls compile to `(*recv)[slot](recv, args...)`.

use windows {
    NULL,
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd, WindowMessage,
    RegisterClassExA, CreateWindowExA, DefWindowProcA,
    ShowWindow, UpdateWindow,
    GetMessageA, TranslateMessage, DispatchMessageA,
    PostQuitMessage,
    SetWindowTextA,
}

use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    IID_ID3D12Device, IID_ID3D12CommandQueue,
    IUnknown, ID3D12Device,
}

@extern(C) {
    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == (WindowMessage.destroy as u32) {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as *HWND, msg, wParam, lParam)
    }

    pub fn run() {
        // ── Window setup ─────────────────────────────────────────
        let hInstance = GetModuleHandleA(NULL as *const char)
        let className = cstrFromString("IlangDx12Pipeline")

        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
            lpfnWndProc:   wndProc,
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hInstance as *HINSTANCE,
            hIcon:         NULL as *HICON,
            hCursor:       NULL as *HCURSOR,
            hbrBackground: 6 as *HBRUSH,
            lpszMenuName:  NULL as *const char,
            lpszClassName: className,
            hIconSm:       NULL as *HICON,
        }
        RegisterClassExA(wc)

        let initialTitle = cstrFromString("ilang DX12: bringing up the pipeline...")
        let hwnd = CreateWindowExA(
            0,
            className,
            initialTitle,
            WindowStyle.overlappedWindow as u32,
            100, 100,
            780, 200,
            NULL as *HWND,
            NULL as *HMENU,
            hInstance as *HINSTANCE,
            NULL
        )
        freeCstr(initialTitle)
        ShowWindow(hwnd, ShowCmd.show as i32)
        UpdateWindow(hwnd)

        // ── (1) D3D12CreateDevice (flat) ─────────────────────────
        let rawDevice: *void = NULL
        let rc = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0 as u32,
            IID_ID3D12Device,
            &rawDevice
        )

        let title = if rc == 0 {
            // ── (2) ID3D12Device::CreateCommandQueue ─────────────
            let device = rawDevice as ID3D12Device
            let queueDesc = D3D12_COMMAND_QUEUE_DESC {
                Type:     D3D12CommandListType.direct as i32,
                Priority: D3D12CommandQueuePriority.normal as i32,
                Flags:    D3D12CommandQueueFlags.none as u32,
                NodeMask: 0,
            }
            let rawQueue: *void = NULL
            let qhr = device.CreateCommandQueue(queueDesc, IID_ID3D12CommandQueue, &rawQueue)

            if qhr == 0 {
                // ── (3) Release queue (IUnknown::Release) ────────
                let queue = rawQueue as IUnknown
                let _ = queue.Release()

                // ── (4) Release device (IUnknown::Release) ───────
                let _ = device.Release()

                cstrFromString("ilang DX12: device + queue via @com interface - OK")
            } else {
                cstrFromString("ilang DX12: device OK, CreateCommandQueue failed")
            }
        } else {
            cstrFromString("ilang DX12: D3D12CreateDevice failed - no GPU?")
        }
        SetWindowTextA(hwnd, title)
        freeCstr(title)

        // ── Standard message pump ────────────────────────────────
        let msg = MSG {
            hwnd:     NULL as *HWND,
            message:  0,
            wParam:   0,
            lParam:   0,
            time:     0,
            pt:       POINT { x: 0, y: 0 },
            lPrivate: 0,
        }
        loop {
            let r = GetMessageA(msg, NULL as *HWND, 0, 0)
            if r <= 0 { break }
            TranslateMessage(msg)
            let _ = DispatchMessageA(msg)
        }
    }
}

run()
