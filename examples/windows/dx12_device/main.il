// 🎮 DirectX 12 device + COM-pipeline demo.
//
// Run:
//   ilang run examples/windows/dx12_device/main.il
//
// Builds the bottom half of a real D3D12 render setup via flat
// entry points + COM vtable dispatch:
//
//   1. D3D12CreateDevice                          flat
//   2. ID3D12Device::CreateCommandQueue           vtable +64
//   3. ID3D12CommandQueue::Release                vtable +16
//   4. ID3D12Device::Release                      vtable +16
//
// IIDs come from `bindings/directx12/d3d12_iids.il` as `pub const`
// values (`IID_ID3D12Device`, `IID_ID3D12CommandQueue`) so user code
// never has to retype
// {189819f1-…} octets.  Same shape as the Windows SDK convention of
// shipping pre-defined `IID_*` symbols in `dxguid.lib`.

use windows {
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
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    IID_ID3D12Device, IID_ID3D12CommandQueue,
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
        let hInstance = GetModuleHandleA(0 as *const char)
        let className = cstrFromString("IlangDx12Pipeline")

        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
            lpfnWndProc:   wndProc,
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hInstance as *HINSTANCE,
            hIcon:         0 as *HICON,
            hCursor:       0 as *HCURSOR,
            hbrBackground: 6 as *HBRUSH,
            lpszMenuName:  0 as *const char,
            lpszClassName: className,
            hIconSm:       0 as *HICON,
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
            0 as *HWND,
            0 as *HMENU,
            hInstance as *HINSTANCE,
            0 as *void
        )
        freeCstr(initialTitle)
        ShowWindow(hwnd, ShowCmd.show as i32)
        UpdateWindow(hwnd)

        // ── (1) D3D12CreateDevice (flat) ─────────────────────────
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            IID_ID3D12Device,
            &device
        )

        let title = if rc == 0 {
            // ── (2) ID3D12Device::CreateCommandQueue (vtable +64) ─
            let devVtbl = readU64(device as *const void, 0 as i64)
            let createQueuePtr = readU64(devVtbl as i64 as *const void, 64 as i64)
            let createQueue = createQueuePtr as i64 as *void
                as fn(i64, *const D3D12_COMMAND_QUEUE_DESC, *const GUID, **void): i32

            let queueDesc = D3D12_COMMAND_QUEUE_DESC {
                Type:     D3D12CommandListType.direct as i32,
                Priority: D3D12CommandQueuePriority.normal as i32,
                Flags:    D3D12CommandQueueFlags.none as u32,
                NodeMask: 0,
            }
            let queue: *void = 0 as *void
            let qhr = createQueue(device as i64, queueDesc, IID_ID3D12CommandQueue, &queue)

            if qhr == 0 {
                // ── (3) Release queue (vtable +16) ───────────────
                let qVtbl = readU64(queue as *const void, 0 as i64)
                let qReleasePtr = readU64(qVtbl as i64 as *const void, 16 as i64)
                let qRelease = qReleasePtr as i64 as *void as fn(i64): u32
                let _ = qRelease(queue as i64)

                // ── (4) Release device (vtable +16) ──────────────
                let dReleasePtr = readU64(devVtbl as i64 as *const void, 16 as i64)
                let dRelease = dReleasePtr as i64 as *void as fn(i64): u32
                let _ = dRelease(device as i64)

                cstrFromString("ilang DX12: device + queue via COM vtable - OK")
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
            hwnd:     0 as *HWND,
            message:  0,
            wParam:   0,
            lParam:   0,
            time:     0,
            pt:       POINT { x: 0, y: 0 },
            lPrivate: 0,
        }
        loop {
            let r = GetMessageA(msg, 0 as *HWND, 0, 0)
            if r <= 0 { break }
            TranslateMessage(msg)
            let _ = DispatchMessageA(msg)
        }
    }
}

run()

// ─── First-pass writing rough edges (revisited) ─────────────────
//
// 1. ✓ IID literal verbosity — SOLVED by `pub const IID_ID3D12Device:
//    GUID = …` constants in `bindings/directx12/d3d12_iids.il`.
//    User code references the constant by name; the GUID octets live
//    exactly once in the bindings, mirroring the C SDK convention
//    (`#include <d3d12.h>` + `dxguid.lib`).
//
// 2. Vtable offsets as bare integers.
//    `+16`, `+64`, … still carry no semantic info. An auto-generated
//    enum (`Vtbl.ID3D12Device.CreateCommandQueue`) or IDL-driven
//    code-gen step would make the source self-describing.
//
// 3. fn-typed local can't be passed through helper fns by value.
//    "no raw pointers in ilang-side wrapper signatures" rule blocks
//    `fn(...)` parameters. Today's workaround: inline every
//    cast-and-call. A `fn(...)` first-class param type (or a
//    relaxation of the rule inside @extern(C)) would let common
//    patterns like `comRelease(obj)` be a helper.
//
// 4. CRepr struct as a user-fn parameter type can't be re-marshalled
//    back to `*const T` for a cast-derived fn pointer at runtime
//    (STATUS_ACCESS_VIOLATION). Inline always works; user-fn helper
//    abstraction blocks on this.
//
// 5. `if`-expression result type inference: `else { -1 }` defaults
//    to i64; needs an explicit `as i32` to match an i32-returning
//    branch.
//
// 6. UTF-16 literals + raw `*u8`/`*u16` indexing `p[i] = x` are
//    still missing — not exercised here.
//
// 7. Larger programs combining window setup + many user-fn helpers +
//    vtable casts hit a startup-time AV that the COM-only tests
//    don't. The simpler version here works; helper-rich draft did
//    not.
