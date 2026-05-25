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
//   3. message pump (window stays open until the user closes it)
//   4. ID3D12CommandQueue::Release / Device::Release
//   5. free the class-name C string
//
// IIDs come from `bindings/directx12/d3d12_iids.il` as `pub const`
// values, so user code never has to retype the {189819f1-…} octets.
// COM dispatch comes from `bindings/directx12/d3d12_com.il`'s
// `@com pub interface IUnknown / ID3D12Device` — slot numbers
// follow the C++ COM ABI through `: Parent` chains, and method
// calls compile to `(*recv)[slot](recv, args...)`.

use std.ffi { cstrFromString, freeCstr }
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
    IID_ID3D12Device, IID_ID3D12CommandQueue, ID3D12Device
}
use ole32 { IUnknown }

@extern(C) {
    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == WindowMessage.destroy {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as HWND, msg, wParam, lParam)
    }

    pub fn run() {
        // ── Window setup ─────────────────────────────────────────
        let hInstance = GetModuleHandleA(NULL)
        let className = cstrFromString("IlangDx12Pipeline")

        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         ClassStyle.hredraw | ClassStyle.vredraw,
            lpfnWndProc:   wndProc,
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hInstance as HINSTANCE,
            hIcon:         NULL,
            hCursor:       NULL,
            hbrBackground: 6 as HBRUSH,
            lpszMenuName:  NULL,
            lpszClassName: className,
            hIconSm:       NULL,
        }
        RegisterClassExA(&wc)

        let initialTitle = cstrFromString("ilang DX12: bringing up the pipeline...")
        let hwnd = CreateWindowExA(
            0,
            className,
            initialTitle,
            WindowStyle.overlappedWindow,
            100, 100,
            780, 200,
            NULL,
            NULL,
            hInstance as HINSTANCE,
            NULL
        )
        freeCstr(initialTitle)
        ShowWindow(hwnd, ShowCmd.show)
        UpdateWindow(hwnd)

        // ── (1) D3D12CreateDevice (flat) ─────────────────────────
        let rawDevice: *void = NULL
        let rc = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0,
            &IID_ID3D12Device,
            &rawDevice
        )

        // ── (2) ID3D12Device::CreateCommandQueue ─────────────────
        // Build the command queue up front so the window title can
        // report a single end-to-end status; Release runs in the
        // cleanup block below, after the user closes the window.
        let rawQueue: *void = NULL
        let title = if rc == 0 {
            let device = rawDevice as ID3D12Device
            let queueDesc = D3D12_COMMAND_QUEUE_DESC {
                Type:     D3D12CommandListType.direct,
                Priority: D3D12CommandQueuePriority.normal,
                Flags:    D3D12CommandQueueFlags.none,
                NodeMask: 0,
            }
            let qhr = device.CreateCommandQueue(&queueDesc, &IID_ID3D12CommandQueue, &rawQueue)
            if qhr == 0 {
                cstrFromString("ilang DX12: device + queue ready - close window to release")
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
            hwnd:     NULL,
            message:  0,
            wParam:   0,
            lParam:   0,
            time:     0,
            pt:       POINT { x: 0, y: 0 },
            lPrivate: 0,
        }
        loop {
            let r = GetMessageA(&msg, NULL, 0, 0)
            if r <= 0 { break }
            TranslateMessage(&msg)
            let _ = DispatchMessageA(&msg)
        }

        // ── (3/4) Post-shutdown cleanup ──────────────────────────
        // The user has dismissed the window (WM_DESTROY →
        // PostQuitMessage breaks the pump). Release every COM
        // handle we acquired, then free the registered class name.
        // Each `Release` is null-guarded so a partial-init run (no
        // GPU, CreateCommandQueue failed) still cleans up what it
        // did create.
        if (rawQueue as i64) != 0 {
            let queue = rawQueue as IUnknown
            let _ = queue.Release()
        }
        if (rawDevice as i64) != 0 {
            let device = rawDevice as IUnknown
            let _ = device.Release()
        }
        freeCstr(className)
    }
}

run()
