// user32.dll — window classes, window creation, message pump.
//
// ANSI (`A`) variants only for now: arguments stay as ilang
// `string` / `*const char`, no UTF-16 conversion. Sufficient for
// a minimal "show a window" demo. Switch to `*W` once a UTF-16
// path lands.
//
// Handle types (HWND / HINSTANCE / HICON / HCURSOR / HBRUSH /
// HMENU) are all `*void` — opaque pointer-sized values on x64.
// WPARAM = `u64` (UINT_PTR), LPARAM = `i64` (LONG_PTR),
// LRESULT = `i64` (LONG_PTR), ATOM = `u16`.

// ─── Window class styles (CS_*) ───────────────────────────────
@flags
pub enum ClassStyle: u32 {
    vredraw     = 0x0001
    hredraw     = 0x0002
    dblclks     = 0x0008
    ownDC       = 0x0020
    classDC     = 0x0040
    parentDC    = 0x0080
    noClose     = 0x0200
    saveBits    = 0x0800
}

// ─── Window styles (WS_*) ─────────────────────────────────────
@flags
pub enum WindowStyle: u32 {
    overlapped   = 0x00000000
    popup        = 0x80000000
    child        = 0x40000000
    visible      = 0x10000000
    disabled     = 0x08000000
    border       = 0x00800000
    caption      = 0x00C00000
    sysmenu      = 0x00080000
    thickframe   = 0x00040000
    minimizeBox  = 0x00020000
    maximizeBox  = 0x00010000
    /// `WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME
    /// | WS_MINIMIZEBOX | WS_MAXIMIZEBOX` — the usual top-level
    /// resizable window.
    overlappedWindow = 0x00CF0000
}

// ─── ShowWindow nCmdShow ──────────────────────────────────────
pub enum ShowCmd: i32 {
    hide            = 0
    showNormal      = 1
    showMinimized   = 2
    showMaximized   = 3
    showNoActivate  = 4
    show            = 5
    minimize        = 6
    showMinNoActive = 7
    showNA          = 8
    restore         = 9
    showDefault     = 10
}

// ─── Window messages (WM_*) ───────────────────────────────────
pub enum WindowMessage: u32 {
    null     = 0x0000
    create   = 0x0001
    destroy  = 0x0002
    close    = 0x0010
    quit     = 0x0012
    paint    = 0x000F
    size     = 0x0005
    keydown  = 0x0100
    keyup    = 0x0101
    lbuttondown = 0x0201
    lbuttonup   = 0x0202
}

@extern(C, "user32") {
    /// Opaque window handle. Pointers (`*HWND`) flow through the
    /// raw `@lib` API; the wrapper-level `wndProc` callback uses
    /// `i64` instead because ilang forbids raw-pointer types in
    /// `pub fn` signatures (the Win64 ABI passes both shapes in
    /// the same register, so a single `as *HWND` / `as i64` cast
    /// at the boundary covers it).
    pub struct HWND {}

    /// `POINT { LONG x; LONG y; }` — 8 bytes.
    pub struct POINT {
        x: i32
        y: i32
    }

    /// `MSG` returned by `GetMessageA` / posted by Windows.
    /// 48 bytes on x64 with the post-Vista `lPrivate` field.
    pub struct MSG {
        hwnd:     *HWND
        message:  u32
        // 4 bytes of padding here so wParam aligns to 8.
        wParam:   u64
        lParam:   i64
        time:     u32
        pt:       POINT
        lPrivate: u32
    }

    /// `WNDCLASSEXA` — 80 bytes on x64. `cbSize` must be set
    /// to `80` before calling `RegisterClassExA`.
    pub struct WNDCLASSEXA {
        cbSize:        u32
        style:         u32
        // First param is HWND. Declared `i64` (not `*void`) so the
        // user-side `wndProc` can be a plain `pub fn` without
        // exposing raw pointers; on x64 Windows the calling
        // convention treats `*void` and `i64` identically (RCX).
        lpfnWndProc:   fn(i64, u32, u64, i64): i64
        cbClsExtra:    i32
        cbWndExtra:    i32
        // 4 bytes pad → hInstance aligns to 8.
        hInstance:     *void
        hIcon:         *void
        hCursor:       *void
        hbrBackground: *void
        lpszMenuName:  *const char
        lpszClassName: *const char
        hIconSm:       *void
    }

    /// `ATOM RegisterClassExA(const WNDCLASSEXA *lpwcx)`.
    /// Returns 0 on failure; use `GetLastError()` for details.
    @lib pub fn RegisterClassExA(lpwcx: *const WNDCLASSEXA): u16

    /// `HWND CreateWindowExA(...)` — full 12-arg form.
    /// Returns the new window handle, or NULL (`0 as *HWND`) on
    /// failure.
    @lib pub fn CreateWindowExA(
        dwExStyle:    u32,
        lpClassName:  *const char,
        lpWindowName: *const char,
        dwStyle:      u32,
        X:            i32,
        Y:            i32,
        nWidth:       i32,
        nHeight:      i32,
        hWndParent:   *HWND,
        hMenu:        *void,
        hInstance:    *void,
        lpParam:      *void
    ): *HWND

    /// `LRESULT DefWindowProcA(HWND, UINT, WPARAM, LPARAM)`.
    /// Call this from the wndproc for every message you don't
    /// handle yourself.
    @lib pub fn DefWindowProcA(
        hwnd: *HWND, msg: u32, wParam: u64, lParam: i64
    ): i64

    /// `BOOL ShowWindow(HWND, int nCmdShow)`. Returns the
    /// window's previous visibility (0 = was hidden, non-zero
    /// = was visible).
    @lib pub fn ShowWindow(hwnd: *HWND, nCmdShow: i32): i32

    /// `BOOL UpdateWindow(HWND)` — flushes a synchronous
    /// `WM_PAINT` if the update region is non-empty.
    @lib pub fn UpdateWindow(hwnd: *HWND): i32

    /// `BOOL GetMessageA(LPMSG, HWND, UINT, UINT)`. Returns:
    ///   `>0` — got a message (call Translate / Dispatch),
    ///   `0`  — got `WM_QUIT` (exit the loop),
    ///   `-1` — error.
    @lib pub fn GetMessageA(
        lpMsg: *MSG, hwnd: *HWND, wMsgFilterMin: u32, wMsgFilterMax: u32
    ): i32

    /// `BOOL TranslateMessage(const MSG *)` — synthesises
    /// `WM_CHAR` from key events.
    @lib pub fn TranslateMessage(lpMsg: *const MSG): i32

    /// `LRESULT DispatchMessageA(const MSG *)` — invokes the
    /// target window's wndproc.
    @lib pub fn DispatchMessageA(lpMsg: *const MSG): i64

    /// `void PostQuitMessage(int nExitCode)` — posts `WM_QUIT`
    /// to the current thread's queue.
    @lib pub fn PostQuitMessage(nExitCode: i32)
}
