// 🪟 Win32 minimal-window demo.
//
// Run on Windows:
//   ilang run examples/windows/min_window/main.il
//
// Bones of every Win32 GUI app:
//   1. `GetModuleHandleA(NULL)` → HINSTANCE for the EXE.
//   2. Fill in a `WNDCLASSEXA`, register it.
//   3. `CreateWindowExA` with the class name → HWND.
//   4. `ShowWindow` + `UpdateWindow`.
//   5. `GetMessage` / `TranslateMessage` / `DispatchMessage` loop
//      until `WM_QUIT`.
//
// Every line here lives inside an `@extern(C, "user32")` block —
// ilang requires raw pointers (`*void` / `*const char`) and the
// address-of operator (`&`) to be confined to extern blocks, and
// Win32 has those at every API boundary. `wndProc` is defined as
// a `pub fn` inside the same block so the WNDCLASSEXA literal
// can reference it by name.

use windows {
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd, WindowMessage,
    RegisterClassExA, CreateWindowExA, DefWindowProcA,
    ShowWindow, UpdateWindow,
    GetMessageA, TranslateMessage, DispatchMessageA,
    PostQuitMessage,
}

@extern(C) {
    /// Forwards every message except `WM_DESTROY`, which posts
    /// `WM_QUIT` so the message loop drops out. `hwnd` is `i64`
    /// (not `*void`) to satisfy the "no raw pointers in wrapper
    /// signatures" rule; the Win64 ABI treats the two the same.
    pub fn wndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == (WindowMessage.destroy as u32) {
            PostQuitMessage(0)
            return 0
        }
        DefWindowProcA(hwnd as *HWND, msg, wParam, lParam)
    }

    pub fn run() {
        let hInstance = GetModuleHandleA(0 as *const char)
        let className = cstrFromString("IlangMinWindow")

        // `(HBRUSH)(COLOR_WINDOW + 1)` — small integer cast to
        // HBRUSH so the system erases the client area to the
        // COLOR_WINDOW system colour.
        let wc = WNDCLASSEXA {
            cbSize:        80,
            style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
            lpfnWndProc:   wndProc,
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hInstance as *HINSTANCE,
            hIcon:         0 as *HICON,
            hCursor:       0 as *HCURSOR,
            hbrBackground: 6 as *HBRUSH,        // COLOR_WINDOW (5) + 1
            lpszMenuName:  0 as *const char,
            lpszClassName: className,
            hIconSm:       0 as *HICON,
        }
        RegisterClassExA(&wc)

        let hwnd = CreateWindowExA(
            0,
            className,
            cstrFromString("ilang Win32 minimal window"),
            WindowStyle.overlappedWindow as u32,
            100, 100,                           // X, Y
            640, 480,                           // W, H
            0 as *HWND,                         // hWndParent
            0 as *HMENU,
            hInstance as *HINSTANCE,
            0 as *void                          // lpParam
        )
        ShowWindow(hwnd, ShowCmd.show as i32)
        UpdateWindow(hwnd)

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
            let r = GetMessageA(&msg, 0 as *HWND, 0, 0)
            if r <= 0 { break }
            TranslateMessage(&msg)
            let _ = DispatchMessageA(&msg)
        }
    }
}

run()
