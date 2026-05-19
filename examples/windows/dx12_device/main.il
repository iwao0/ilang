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
// First-pass writing rough edges are listed at the bottom of the
// file — refactor for ergonomics later.

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
        // IID_ID3D12Device = 189819f1-1db6-4b57-be54-1821339b85f7
        let deviceIid = GUID {
            Data1: 0x189819f1, Data2: 0x1db6, Data3: 0x4b57,
            Data4lo: 0x211854be, Data4hi: 0xf7859b33,
        }
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            deviceIid,
            &device
        )

        let title = if rc == 0 {
            // ── (2) ID3D12Device::CreateCommandQueue (vtable +64) ─
            let devVtbl = readU64(device as *const void, 0 as i64)
            let createQueuePtr = readU64(devVtbl as i64 as *const void, 64 as i64)
            let createQueue = createQueuePtr as i64 as *void
                as fn(i64, *const D3D12_COMMAND_QUEUE_DESC, *const GUID, **void): i32

            // IID_ID3D12CommandQueue = 0ec870a6-5d7e-4c22-8cfc-5baae07616ed
            let queueIid = GUID {
                Data1: 0x0ec870a6, Data2: 0x5d7e, Data3: 0x4c22,
                Data4lo: 0xaa5bfc8c, Data4hi: 0xed1676e0,
            }
            let queueDesc = D3D12_COMMAND_QUEUE_DESC {
                Type:     D3D12CommandListType.direct as i32,
                Priority: D3D12CommandQueuePriority.normal as i32,
                Flags:    D3D12CommandQueueFlags.none as u32,
                NodeMask: 0,
            }
            let queue: *void = 0 as *void
            let qhr = createQueue(device as i64, queueDesc, queueIid, &queue)

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

                cstrFromString("ilang DX12: device + queue via COM vtable — OK")
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

// ─── First-pass writing rough edges ─────────────────────────────
//
// 1. IID literal verbosity.
//    Every COM call site needs an IID populated with `Data1` (u32),
//    `Data2`/`Data3` (u16), and two u32 halves of the 8-byte `Data4`.
//    Computing the two u32 halves from the canonical
//    `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` text is hand-work
//    (little-endian packing of 4 bytes at a time). A compile-time
//    `guid!("...")` macro would eliminate every comment block.
//
// 2. Vtable offsets as bare integers.
//    `+16`, `+64`, … carry no semantic info — readers have to consult
//    the SDK headers to know that `+64` is
//    `ID3D12Device::CreateCommandQueue`. An auto-generated enum
//    (`Vtbl.ID3D12Device.CreateCommandQueue`) or an IDL-driven
//    code-gen step would make the source self-describing.
//
// 3. fn-typed local can't be passed through helper fns by value.
//    A helper that does `let f = ptr as *void as fn(...): T; f(args)`
//    is straightforward inline but can't be factored out: the
//    "no raw pointers in ilang-side wrapper signatures" rule blocks
//    `fn(...)` parameters too. Today's workaround is to inline every
//    cast-and-call.
//
// 4. CRepr struct as a user-fn parameter type can't be re-marshalled
//    back to `*const T` for a cast-derived fn pointer at runtime
//    (STATUS_ACCESS_VIOLATION). The ilang user-fn ABI for CRepr
//    structs needs to align with what the cast site expects before
//    `pub fn createCommandQueue(device: i64, desc: D3D12_COMMAND_QUEUE_DESC,
//    ...)` -style helpers can be written. Inline always works.
//
// 5. `if`-expression result type inference.
//    `if cond { fn(): i32 } else { -1 }` — the `-1` defaults to i64;
//    needs an explicit `as i32` to match the function-call branch.
//
// 6. UTF-16 literals and raw `*u8`/`*u16` indexing `p[i] = x` are
//    still missing — not exercised here since every D3D12 entry
//    point used takes ANSI strings or `*void`.
//
// 7. Larger programs that combine the window setup + COM dispatch
//    via vtable casts hit a startup-time AV that smaller fixtures
//    don't — the COM bits work cleanly in isolation under
//    `bindings/directx12/test/` but combining the same shape with
//    the Win32 window setup currently breaks. Worth a focused
//    repro and an ilang-side bisect.
