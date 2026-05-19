// 🎮 DirectX 12 device + shader compile demo.
//
// Run:
//   ilang run examples/windows/dx12_device/main.il
//
// What it does:
//   1. Opens a Win32 window.
//   2. Calls D3D12CreateDevice — flat entry point from d3d12.dll.
//   3. Calls D3D12GetDebugInterface — flat entry point.
//   4. Compiles a trivial HLSL vertex shader with D3DCompile.
//   5. Displays the status of each step in the window title.
//   6. Runs a standard message loop until the window is closed.
//
// What it does NOT do:
//   - Actual rendering. Every step past `D3D12CreateDevice` (e.g.
//     CreateCommandQueue, CreateDescriptorHeap, CreateGraphicsPipelineState)
//     requires COM vtable calls on the returned interface pointer.
//     ilang doesn't yet support calling through an arbitrary COM
//     vtable: there's no syntax for `obj->vtbl->Method(obj, args...)`
//     without manually laying out the vtable struct and (more
//     importantly) without a way to safely invoke a function pointer
//     loaded out of a heap-allocated struct field — see the
//     limitations note at the bottom of this file for details.

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
    HeapAlloc, HeapFree, GetProcessHeap,
}

use directx12 {
    D3D12CreateDevice, D3D12GetDebugInterface,
    D3DFeatureLevel,
    GUID,
    D3DCompile, ID3DBlob, D3D_SHADER_MACRO, D3DCompileFlags,
}

@extern(C) {
    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == (WindowMessage.destroy as u32) {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as *HWND, msg, wParam, lParam)
    }

    /// Tries to create a D3D12 device. Returns 1 on success, 0 on
    /// failure.  Leaks the device pointer — without COM vtable
    /// support we can't call Release().
    pub fn tryCreateDevice(): i32 {
        // IID_ID3D12Device = {189819f1-1db6-4b57-be54-1821339b85f7}
        // Data4 bytes (in order): be 54 18 21 33 9b 85 f7.
        // Stored as two little-endian u32:
        //   lo = 0xbe | 0x54<<8 | 0x18<<16 | 0x21<<24 = 0x211854be
        //   hi = 0x33 | 0x9b<<8 | 0x85<<16 | 0xf7<<24 = 0xf7859b33
        let iid = GUID {
            Data1:   0x189819f1,
            Data2:   0x1db6,
            Data3:   0x4b57,
            Data4lo: 0x211854be,
            Data4hi: 0xf7859b33,
        }
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            iid,
            &device
        )
        if rc == 0 && (device as i64) != 0 {
            return 1
        }
        0
    }

    /// Asks for the D3D12 debug interface. Requires the "Graphics
    /// Tools" optional feature; returns DXGI_ERROR_NOT_AVAILABLE
    /// otherwise. Either outcome is a successful call here.
    pub fn tryDebugInterface(): i32 {
        // IID_ID3D12Debug = {344488b7-6846-474b-b989-f027448245e0}
        let iid = GUID {
            Data1:   0x344488b7,
            Data2:   0x6846,
            Data3:   0x474b,
            Data4lo: 0x27f089b9,
            Data4hi: 0xe0458244,
        }
        let dbg: *void = 0 as *void
        let rc = D3D12GetDebugInterface(iid, &dbg)
        if rc == 0 && (dbg as i64) != 0 {
            return 1
        }
        0
    }

    /// Compiles a one-line HLSL vertex shader. Returns 1 on success.
    pub fn tryCompileShader(): i32 {
        let hlsl = cstrFromString(
            "float4 main(float3 p : POSITION) : SV_POSITION { return float4(p, 1.0); }"
        )
        let entry = cstrFromString("main")
        let target = cstrFromString("vs_5_0")
        let code: *ID3DBlob = 0 as *ID3DBlob
        let errors: *ID3DBlob = 0 as *ID3DBlob
        let rc = D3DCompile(
            hlsl as *const void,
            76 as u64,                       // length; small overshoot OK
            0 as *const char,
            0 as *const D3D_SHADER_MACRO,
            0 as *void,
            entry,
            target,
            D3DCompileFlags.optimizationLevel1 as u32,
            0 as u32,
            &code,
            &errors
        )
        freeCstr(hlsl)
        freeCstr(entry)
        freeCstr(target)
        if rc == 0 && (code as i64) != 0 {
            return 1
        }
        0
    }

    pub fn run() {
        let hInstance = GetModuleHandleA(0 as *const char)
        let className = cstrFromString("IlangDx12Window")

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

        let initialTitle = cstrFromString("ilang DX12: initialising...")
        let hwnd = CreateWindowExA(
            0,
            className,
            initialTitle,
            WindowStyle.overlappedWindow as u32,
            100, 100,
            720, 200,
            0 as *HWND,
            0 as *HMENU,
            hInstance as *HINSTANCE,
            0 as *void
        )
        freeCstr(initialTitle)
        ShowWindow(hwnd, ShowCmd.show as i32)
        UpdateWindow(hwnd)

        // Probe each capability.
        let devOK   = tryCreateDevice()
        let dbgOK   = tryDebugInterface()
        let shadeOK = tryCompileShader()

        // Update title with the combined status. We avoid string
        // formatting (none in ilang for ints yet) and just pick a
        // fixed message per outcome.
        let title = if devOK != 0 {
            if shadeOK != 0 {
                if dbgOK != 0 {
                    cstrFromString("ilang DX12: device OK | shader OK | debug-layer OK")
                } else {
                    cstrFromString("ilang DX12: device OK | shader OK | debug-layer N/A")
                }
            } else {
                cstrFromString("ilang DX12: device OK | shader FAIL")
            }
        } else {
            cstrFromString("ilang DX12: D3D12CreateDevice failed — no GPU?")
        }
        SetWindowTextA(hwnd, title)
        freeCstr(title)

        // Standard message pump.
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

// ─── Limitations encountered ─────────────────────────────────────
//
// 1. COM vtable invocation
//    DirectX 12 is COM-based: everything past `D3D12CreateDevice`
//    (queue / heap / list / pso creation, draw recording, present,
//    fence signal/wait) lives on the returned interface vtable.
//    A typical call looks like (in C):
//      device->lpVtbl->CreateCommandQueue(device, &desc, &iid, &queue);
//    which requires:
//      a. reading 8 bytes from the object pointer to get the vtable,
//      b. indexing into the vtable to get the method pointer,
//      c. calling that pointer with the object as the first argument.
//
//    ilang has function-pointer struct fields (used by the WndProc
//    callback in WNDCLASSEXA), but there is no `obj->method(...)`
//    syntax that loads a function pointer out of a heap struct and
//    invokes it as a C function in one step. The few examples that
//    use fn-pointer fields all ASSIGN them and let the OS call them,
//    not the other way round.
//
//    Without COM dispatch, we can:
//      - call any flat `D3D12*` / `D3DCompile*` / `CreateDXGIFactory*`
//        export,
//      - hold the returned interface pointers as opaque values,
//      - leak them on program exit.
//    We cannot:
//      - Release() the interfaces (no vtable call),
//      - create a command queue, allocator, list, or PSO,
//      - present anything to the swap chain,
//      - query feature support, query capabilities, set up barriers.
//
//    Adding COM-call support would benefit a lot of Win32 bindings
//    (DirectX, WIC, Media Foundation, Direct2D, DirectWrite, Windows
//    Imaging, all of which are COM-only).
//
// 2. Wide-string literals
//    Many DirectX entry points take UTF-16 (e.g. D3DCompileFromFile
//    `pFileName`). ilang's string literal is UTF-8, so we either
//    convert at runtime via MultiByteToWideChar (annoying for short
//    constant paths) or stick to the ANSI variants where available.
//
// 3. Inline byte / pointer writes
//    Indexing into raw `*u8` / `*u16` pointers from ilang code
//    (`p[i] = x`) is rejected with "expected any[], got *u16". This
//    makes it awkward to build short UTF-16 strings or to write into
//    HeapAlloc'd buffers without going through a Win32 string helper.
//
// 4. Function-pointer return without struct field
//    GetProcAddress returns `*void` which ilang can hold but can't
//    later cast back to a callable `fn(...)` type. Combined with (1)
//    this rules out the "lazy COM" pattern of resolving a single
//    method by name.
