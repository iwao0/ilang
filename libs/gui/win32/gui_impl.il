/// Windows implementation of the `gui-core` bridge API.
///
/// Mirrors `libs/gui/cocoa/gui_impl.il` — gui-core picks whichever
/// is reachable through `[deps] @target("…")` at build time. The
/// `handle` returned is the parallel-array index, not the raw
/// HWND, so per-window callback lookup goes through the indices
/// stashed in `GWLP_USERDATA`.
///
/// Untested on a real Windows host — the macOS build of the
/// compiler stops at parse/type-check for `@target("windows")`
/// items, so anything here that compiles cleanly on macOS may
/// still misbehave at runtime. Treat the dispatch table /
/// keyCode mapping as a starting point.

use windows {
    GetModuleHandleA,
    HWND, HINSTANCE, HICON, HCURSOR, HBRUSH, HMENU,
    WNDCLASSEXA, MSG, POINT,
    ClassStyle, WindowStyle, ShowCmd, WindowMessage, VirtualKey, WindowLongIndex,
    // Wide variants — drive every string-bearing entry point in
    // UTF-16 so non-ASCII titles / labels round-trip correctly.
    // The A variants reinterpret the UTF-8 bytes from
    // `cstrFromString` as the system ANSI code page, garbling
    // anything outside ASCII (Japanese / Chinese / accented Latin).
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    SetWindowTextW, GetMessageW, DispatchMessageW,
    ShowWindow, UpdateWindow, EnableWindow, MoveWindow,
    TranslateMessage, PostQuitMessage,
    GetWindowLongPtrA, SetWindowLongPtrA, GetKeyState,
    LoadCursorA, SetCursor, SystemCursor,
    // Side-by-side activation: load comctl32 v6 (themed buttons /
    // edit / list / tab / progress) without an embedded .manifest.
    ACTCTXW, CreateActCtxW, ActivateActCtx, GetTempPathA,
    CreateFileA, WriteFile, CloseHandle, HANDLE,
    FileAccess, FileCreation, FileAttribute,
    HeapAlloc, HeapFree, GetProcessHeap,
    MultiByteToWideChar,
    // SetWindowTheme — best-effort modernization for buttons /
    // listviews. No-op when a v6 activation context isn't live.
    SetWindowTheme,
    // InitCommonControlsEx — required so the v6 control classes
    // are actually registered before the first CreateWindow.
    INITCOMMONCONTROLSEX, InitCommonControlsEx, Icc,
}

// ─── per-window callback table ────────────────────────────────

let HWNDS:          HWND[]                                                  = []
let ON_CLOSE:       fn()[]                                                  = []
let ON_RESIZE:      fn(f64, f64)[]                                          = []
let ON_KEY_DOWN:    fn(string, bool, bool, bool, bool)[]                    = []
let ON_KEY_UP:      fn(string, bool, bool, bool, bool)[]                    = []
let ON_MOUSE_MOVE:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_DOWN:  fn(f64, f64, i32, bool, bool, bool, bool)[]             = []
let ON_MOUSE_UP:    fn(f64, f64, i32, bool, bool, bool, bool)[]             = []

let BUTTON_HWNDS:   HWND[]                                                  = []
let ON_CLICK:       fn(f64, f64, i32)[]                                     = []

// True once enableComctl32V6() has run — we only need to install
// the side-by-side activation context for comctl32 6.0 once per
// process, regardless of how many windows / buttons get created.
let SXS_INITIALIZED: bool[] = [false]

// Wide "Explorer" string used by SetWindowTheme on each button.
// Stored as `i64` (pointer value) because ilang arrays don't hold
// raw-pointer element types outside `@extern(C)` blocks. The slot
// is 0 until `enableComctl32V6` has populated it.
let EXPLORER_THEME: i64[] = [0 as i64]
fn noOpClick(x: f64, y: f64, b: i32) {
    let _ = x
    let _ = y
    let _ = b
}

fn noOp1() {}
fn noOp2(w: f64, h: f64) {
    let _ = w
    let _ = h
}
fn noOpKey(c: string, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = c
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}
fn noOpMouse(x: f64, y: f64, b: i32, k1: bool, k2: bool, k3: bool, k4: bool) {
    let _ = x
    let _ = y
    let _ = b
    let _ = k1
    let _ = k2
    let _ = k3
    let _ = k4
}

// ─── modifier helpers (GetKeyState high-bit set ⇒ pressed) ─────

fn keyDown(vk: i32): bool {
    (GetKeyState(vk) as i32 & 0x8000) != 0
}
fn modShift(): bool { keyDown(VirtualKey.shift as i32) }
fn modCtrl():  bool { keyDown(VirtualKey.control as i32) }
fn modAlt():   bool { keyDown(VirtualKey.alt as i32) }
fn modMeta():  bool { keyDown(VirtualKey.lWin as i32) || keyDown(VirtualKey.rWin as i32) }

// ─── VirtualKey → W3C KeyboardEvent.code ──────────────────────

fn vkToString(vk: u32): string {
    match vk {
        0x41 { "KeyA" }
        0x42 { "KeyB" }
        0x43 { "KeyC" }
        0x44 { "KeyD" }
        0x45 { "KeyE" }
        0x46 { "KeyF" }
        0x47 { "KeyG" }
        0x48 { "KeyH" }
        0x49 { "KeyI" }
        0x4A { "KeyJ" }
        0x4B { "KeyK" }
        0x4C { "KeyL" }
        0x4D { "KeyM" }
        0x4E { "KeyN" }
        0x4F { "KeyO" }
        0x50 { "KeyP" }
        0x51 { "KeyQ" }
        0x52 { "KeyR" }
        0x53 { "KeyS" }
        0x54 { "KeyT" }
        0x55 { "KeyU" }
        0x56 { "KeyV" }
        0x57 { "KeyW" }
        0x58 { "KeyX" }
        0x59 { "KeyY" }
        0x5A { "KeyZ" }
        0x30 { "Digit0" }
        0x31 { "Digit1" }
        0x32 { "Digit2" }
        0x33 { "Digit3" }
        0x34 { "Digit4" }
        0x35 { "Digit5" }
        0x36 { "Digit6" }
        0x37 { "Digit7" }
        0x38 { "Digit8" }
        0x39 { "Digit9" }
        0x0D { "Enter" }
        0x1B { "Escape" }
        0x08 { "Backspace" }
        0x09 { "Tab" }
        0x20 { "Space" }
        0x25 { "ArrowLeft" }
        0x26 { "ArrowUp" }
        0x27 { "ArrowRight" }
        0x28 { "ArrowDown" }
        _    { "" }
    }
}

@extern(C) {
    /// Window procedure. Every window registered through
    /// `createNativeWindow` shares this class proc; per-instance
    /// callbacks are looked up by the index stored in
    /// `GWLP_USERDATA` (= `WindowLongIndex.userdata`) at create
    /// time.
    pub fn guiWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        let ix = GetWindowLongPtrA(hwnd as HWND, WindowLongIndex.userdata as i32)
        if ix >= 0 && ix < (ON_CLOSE.length as i64) {
            // lParam packs (x, y) for mouse messages and (width,
            // height) for WM_SIZE — same low/high u16 split, pulled
            // here once so the per-arm code only emits the cb call.
            let lo = (lParam & 0xFFFF) as f64
            let hi = ((lParam >> 16) & 0xFFFF) as f64
            match msg as WindowMessage {
                WindowMessage.close {
                    let cb = ON_CLOSE[ix]
                    cb()
                    PostQuitMessage(0)
                    return 0
                }
                WindowMessage.setCursor {
                    // lParam packs (hit-test-code, mouse-message);
                    // low word = hit-test. HTCLIENT == 1 means the
                    // pointer is over the client area; force the
                    // standard arrow there so the resize-border
                    // cursor doesn't leak back in. Anything else
                    // (HTLEFT etc.) falls through to DefWindowProc
                    // so the system resize cursors still work.
                    let ht = (lParam & 0xFFFF) as i32
                    if ht == 1 {
                        let arrow = LoadCursorA(
                            0 as HINSTANCE,
                            SystemCursor.arrow as i64 as *const char
                        )
                        let _ = SetCursor(arrow)
                        return 1
                    }
                }
                WindowMessage.size {
                    let cb = ON_RESIZE[ix]
                    cb(lo, hi)
                    return 0
                }
                WindowMessage.keydown {
                    let cb = ON_KEY_DOWN[ix]
                    cb(vkToString(wParam as u32),
                       modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.keyup {
                    let cb = ON_KEY_UP[ix]
                    cb(vkToString(wParam as u32),
                       modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mouseMove {
                    let cb = ON_MOUSE_MOVE[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.lbuttondown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.lbuttonup {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 0, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mButtonDown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.mButtonUp {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 1, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.rButtonDown {
                    let cb = ON_MOUSE_DOWN[ix]
                    cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.rButtonUp {
                    let cb = ON_MOUSE_UP[ix]
                    cb(lo, hi, 2, modCtrl(), modShift(), modAlt(), modMeta())
                    return 0
                }
                WindowMessage.command {
                    // LOWORD(wParam) is the control identifier we
                    // baked in as the button's HMENU value
                    // (= the BUTTON_HWNDS index). BN_CLICKED carries
                    // no cursor; pass (0, 0, 0) and let gui-core
                    // fill in via the cached button position.
                    let id = (wParam & 0xFFFF) as i64
                    if id >= 0 && id < (ON_CLICK.length as i64) {
                        let cb = ON_CLICK[id]
                        cb(0.0, 0.0, 0)
                    }
                    return 0
                }
                _ {}
            }
        }
        DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
    }
}

// ─── public API ────────────────────────────────────────────────
//
// Wrapped in `@extern(C) { ... }` so the raw-pointer literals
// (`0 as *const char` for NULL, `0 as *void`, `&wc` / `&msg`)
// and `cstrFromString` are nameable. ilang only allows raw C
// pointer types to appear inside an `@extern(C)` block.

@extern(C) {

/// Convert an ilang UTF-8 `string` to a freshly-`HeapAlloc`'d
/// NUL-terminated UTF-16 buffer suitable for the wide Win32 API.
/// Returned as `i64` because ilang fns can't return raw pointers;
/// cast back with `as *u16` at the call site. The buffer is owned
/// by the caller; pass to a Win32 fn (which won't keep it) and
/// `HeapFree` afterwards — or, when the lifetime has to outlast a
/// single call (class name, theme name), leak it.
pub fn toWideUtf8(s: string): i64 {
    let cstr = cstrFromString(s)
    // 4× the code-point count is a safe upper bound for UTF-16:
    // CP_UTF8 ⇒ UTF-16 expands by at most 2× per code point, but
    // ilang's `.length` is code points; multiply by 2 and add room
    // for the NUL.
    let cap = (s.length as i32 + 1) * 2
    let buf = HeapAlloc(
        GetProcessHeap(),
        8 as u32,
        (cap as u64) * 2 as u64
    )
    let _ = MultiByteToWideChar(
        65001 as u32,           // CP_UTF8
        0 as u32,
        cstr,
        -1 as i32,              // NUL-terminated input
        buf as *u16,
        cap
    )
    buf as i64
}

/// Install a side-by-side activation context that pulls in
/// `Microsoft.Windows.Common-Controls` v6 — the "modern" themed
/// button / edit / listview / tab / progress classes. Without
/// this (or an embedded .manifest resource on the EXE), CreateWindow
/// hands back the legacy Win95-style controls.
///
/// Writes the manifest to `%TEMP%\ilang_gui_v6.manifest` on first
/// call, then calls `CreateActCtxW + ActivateActCtx`. The context
/// stays activated for the lifetime of the process (we leak the
/// cookie on purpose).
pub fn enableComctl32V6() {
    if SXS_INITIALIZED[0] { return }
    SXS_INITIALIZED[0] = true

    let heap = GetProcessHeap()

    // ── Compose the manifest path under %TEMP%. ─────────────────
    let tempBuf = HeapAlloc(heap, 8 as u32, 1024 as u64)
    let _ = GetTempPathA(1024 as u32, tempBuf as *char)
    let tempStr = stringFromCstr(tempBuf as *const char)
    let manifestPath = tempStr + "ilang_gui_v6.manifest"
    let pathC = cstrFromString(manifestPath)

    // ── Manifest body. Asks the loader for comctl32 6.0 — the
    //    same content used by themed apps' embedded .manifest. ─
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\"><dependency><dependentAssembly><assemblyIdentity type=\"win32\" name=\"Microsoft.Windows.Common-Controls\" version=\"6.0.0.0\" processorArchitecture=\"*\" publicKeyToken=\"6595b64144ccf1df\" language=\"*\"/></dependentAssembly></dependency></assembly>"
    let manifestC = cstrFromString(manifest)
    let manifestLen = manifest.length as u32

    let hFile = CreateFileA(
        pathC,
        FileAccess.write as u32,
        0 as u32,
        0 as *void,
        FileCreation.createAlways as u32,
        FileAttribute.normal as u32,
        0 as HANDLE
    )
    if (hFile as i64) == -1 as i64 {
        HeapFree(heap, 0 as u32, tempBuf)
        return
    }
    let written: u32 = 0
    let _ = WriteFile(
        hFile,
        manifestC as *const void,
        manifestLen,
        &written,
        0 as *void
    )
    CloseHandle(hFile)

    // ── ANSI path → wide for CreateActCtxW. ─────────────────────
    let wideBuf = HeapAlloc(heap, 8 as u32, 2048 as u64) as *u16
    let _ = MultiByteToWideChar(
        0 as u32,                   // CP_ACP
        0 as u32,
        pathC,
        -1 as i32,                  // NUL-terminated
        wideBuf,
        1024 as i32
    )

    // ── CreateActCtxW + ActivateActCtx. Cookie leaks on purpose
    //    so v6 stays active for every later window / button. ──
    let actCtx = ACTCTXW {
        cbSize:                 56 as u32,
        dwFlags:                0 as u32,
        lpSource:               wideBuf as *const u16,
        wProcessorArchitecture: 0 as u16,
        wLangId:                0 as u16,
        _pad:                   0 as u32,
        lpAssemblyDirectory:    0 as *const u16,
        lpResourceName:         0 as *const u16,
        lpApplicationName:      0 as *const u16,
        hModule:                0 as i64,
    }
    let h = CreateActCtxW(&actCtx)
    if h != -1 as i64 {
        let cookie: u64 = 0
        let _ = ActivateActCtx(h, &cookie)
    }
    HeapFree(heap, 0 as u32, tempBuf)
    // wideBuf intentionally not freed — the activation context
    // keeps a pointer to it.

    // ── Register v6 common control classes. ────────────────────
    let icc = INITCOMMONCONTROLSEX {
        dwSize: 8 as u32,
        dwICC:  Icc.standardClasses as u32 | Icc.barClasses as u32,
    }
    let _ = InitCommonControlsEx(&icc)

    // ── Build a cached wide "Explorer" string for SetWindowTheme. ─
    let themeBuf = HeapAlloc(heap, 8 as u32, 32 as u64) as *u16
    let themeName = cstrFromString("Explorer")
    let _ = MultiByteToWideChar(
        0 as u32, 0 as u32, themeName, -1 as i32,
        themeBuf, 16 as i32
    )
    EXPLORER_THEME[0] = themeBuf as i64
}

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    // Switch the process over to comctl32 v6 *before* the first
    // CreateWindowExA so registered classes pick up the themed
    // shapes.
    enableComctl32V6()

    let hInstance = GetModuleHandleA(0 as *const char)
    // Class name + title go through UTF-16 so non-ASCII characters
    // (Japanese / Chinese / etc.) survive intact. The class name
    // buffer must outlive `RegisterClassExW`; we intentionally
    // leak it (a single 32-byte allocation per process).
    let classNameW = toWideUtf8("IlangGuiWindow") as *u16
    let titleW     = toWideUtf8(title) as *u16

    // Load IDC_ARROW so Windows restores the standard pointer
    // when the cursor re-enters the client area after the resize
    // border swapped it to a size cursor. We also explicitly
    // handle WM_SETCURSOR in `guiWndProc` so this is belt-and-
    // suspenders.
    let arrowCursor = LoadCursorA(0 as HINSTANCE,
                                  SystemCursor.arrow as i64 as *const char)

    // WNDCLASSEXA and WNDCLASSEXW have the same byte layout
    // (80 bytes on x64); the difference is which char width Win32
    // uses to read `lpszClassName` / `lpszMenuName`. We cast the
    // wide pointer through `*const char` so the existing struct
    // declaration keeps working, then call `RegisterClassExW`.
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   guiWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       arrowCursor,
        // COLOR_BTNFACE (15) + 1 = 16 — the default dialog /
        // control panel grey, what callers usually mean by
        // "normal Windows background". COLOR_WINDOW + 1 (= 6)
        // is pure white, which makes single-button utility
        // windows look like an embedded text editor.
        hbrBackground: 16 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: classNameW as *const char,
        hIconSm:       0 as HICON,
    }
    // RegisterClassExW returns 0 only when the class isn't yet
    // registered; calling twice silently no-ops, which is what we
    // want when `createNativeWindow` is called more than once.
    let _ = RegisterClassExW(&wc)

    let hwnd = CreateWindowExW(
        0,
        classNameW,
        titleW,
        WindowStyle.overlappedWindow as u32,
        100, 100,
        width as i32, height as i32,
        0 as HWND,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, titleW as *void)
    let handle: i64 = HWNDS.length as i64
    HWNDS.push(hwnd)
    ON_CLOSE.push(noOp1)
    ON_RESIZE.push(noOp2)
    ON_KEY_DOWN.push(noOpKey)
    ON_KEY_UP.push(noOpKey)
    ON_MOUSE_MOVE.push(noOpMouse)
    ON_MOUSE_DOWN.push(noOpMouse)
    ON_MOUSE_UP.push(noOpMouse)
    // Stash our parallel-array index so WndProc can route back.
    let _ = SetWindowLongPtrA(hwnd, WindowLongIndex.userdata as i32, handle)

    ShowWindow(hwnd, ShowCmd.show as i32)
    UpdateWindow(hwnd)
    handle
}

pub fn setOnClose(handle: i64, cb: fn()) {
    if handle >= 0 && handle < (ON_CLOSE.length as i64) { ON_CLOSE[handle] = cb }
}
pub fn setOnResize(handle: i64, cb: fn(f64, f64)) {
    if handle >= 0 && handle < (ON_RESIZE.length as i64) { ON_RESIZE[handle] = cb }
}
pub fn setOnKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_KEY_DOWN.length as i64) { ON_KEY_DOWN[handle] = cb }
}
pub fn setOnKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_KEY_UP.length as i64) { ON_KEY_UP[handle] = cb }
}
pub fn setOnMouseMove(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_MOVE.length as i64) { ON_MOUSE_MOVE[handle] = cb }
}
pub fn setOnMouseDown(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_DOWN.length as i64) { ON_MOUSE_DOWN[handle] = cb }
}
pub fn setOnMouseUp(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if handle >= 0 && handle < (ON_MOUSE_UP.length as i64) { ON_MOUSE_UP[handle] = cb }
}

// ─── buttons ──────────────────────────────────────────────────

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = BUTTON_HWNDS.length as i64
    let classW = toWideUtf8("BUTTON") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        textW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        id as HMENU,           // HMENU slot doubles as the control ID
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, textW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)

    // Apply the Explorer theme so the button picks up the modern
    // rounded / hover-shaded look. With the v6 activation context
    // installed by `enableComctl32V6` this swaps the Win95 grey
    // 3-D border for the current theme's button.
    if EXPLORER_THEME[0] != 0 as i64 {
        let _ = SetWindowTheme(
            hwnd as i64,
            EXPLORER_THEME[0] as *const u16,
            0 as *const u16
        )
    }

    BUTTON_HWNDS.push(hwnd)
    ON_CLICK.push(noOpClick)
    id
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if handle >= 0 && handle < (ON_CLICK.length as i64) { ON_CLICK[handle] = cb }
}
pub fn setButtonText(handle: i64, text: string) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(BUTTON_HWNDS[handle], w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        MoveWindow(BUTTON_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(BUTTON_HWNDS[handle], flag)
    }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (BUTTON_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(BUTTON_HWNDS[handle], cmd)
    }
}

pub fn runEventLoop() {
    let msg = MSG {
        hwnd:     0 as HWND,
        message:  0,
        wParam:   0,
        lParam:   0,
        time:     0,
        pt:       POINT { x: 0, y: 0 },
        lPrivate: 0,
    }
    loop {
        // GetMessageW / DispatchMessageW so the synthesised WM_CHAR
        // messages carry UTF-16 code units (matters for any future
        // text input — TranslateMessage queues a wide WM_CHAR for
        // wide message loops, an ANSI one otherwise).
        let r = GetMessageW(&msg, 0 as HWND, 0, 0)
        if r <= 0 { break }
        TranslateMessage(&msg)
        let _ = DispatchMessageW(&msg)
    }
}

} // @extern(C)
