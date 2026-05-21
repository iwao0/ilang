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
    TranslateMessage, PostQuitMessage, DestroyWindow,
    GetWindowLongPtrA, SetWindowLongPtrA, GetKeyState,
    LoadCursorA, SetCursor, SystemCursor,
    // WM_ERASEBKGND path — handle it explicitly so resize /
    // maximise transitions actually paint the new region.
    HDC, RECT, GetClientRect, FillRect, GetSysColorBrush,
    // Side-by-side activation: load comctl32 v6 (themed buttons /
    // edit / list / tab / progress) without an embedded .manifest.
    ACTCTXW, CreateActCtxW, ActivateActCtx, GetTempPathA,
    CreateFileA, WriteFile, CloseHandle, HANDLE,
    FileAccess, FileCreation, FileAttribute,
    HeapAlloc, HeapFree, GetProcessHeap,
    MultiByteToWideChar, WideCharToMultiByte,
    // Edit / static read-back — fetch the current text out of an
    // EDIT control for the EN_CHANGE callback.
    GetWindowTextW, GetWindowTextLengthW,
    // Control I/O for trackbar / progress / combo / button-state.
    SendMessageA, SendMessageW,
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

// Labels (STATIC class) — display-only text, no event surface.
let LABEL_HWNDS:    HWND[]                                                  = []

// Text fields (EDIT class) — single-line input. EN_CHANGE
// notifications fire each time the contents change; the cb gets
// the freshly-read UTF-8 contents.
let EDIT_HWNDS:     HWND[]                                                  = []
let ON_TEXT_CHANGE: fn(string)[]                                            = []

// Checkboxes (BUTTON class with BS_AUTOCHECKBOX). BN_CLICKED
// fires after the state has already flipped, so the callback
// reads the post-flip BM_GETCHECK result.
let CHECKBOX_HWNDS: HWND[]                                                  = []
let ON_TOGGLE:      fn(bool)[]                                              = []

// Sliders (msctls_trackbar32). The control reports moves via
// WM_HSCROLL to its parent — we map the integer position back to
// the user-facing f64 in [min, max] using the per-slider ranges
// cached below.
let SLIDER_HWNDS:   HWND[]                                                  = []
let ON_SLIDER:      fn(f64)[]                                               = []
let SLIDER_MIN:     f64[]                                                   = []
let SLIDER_MAX:     f64[]                                                   = []

// Progress bars (msctls_progress32). Read-only; no callbacks.
let PROGRESS_HWNDS: HWND[]                                                  = []
let PROGRESS_MIN:   f64[]                                                   = []
let PROGRESS_MAX:   f64[]                                                   = []

// Combo boxes (COMBOBOX with CBS_DROPDOWNLIST). CBN_SELCHANGE
// fires when the user picks a new item.
let COMBO_HWNDS:    HWND[]                                                  = []
let ON_COMBO:       fn(i32)[]                                               = []

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
fn noOpText(s: string) { let _ = s }
fn noOpBool(b: bool) { let _ = b }
fn noOpF64(v: f64) { let _ = v }
fn noOpI32(i: i32) { let _ = i }

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
                    // Standard idiom: fire the user's onClose, then
                    // ask the system to destroy the HWND. WM_DESTROY
                    // (below) calls PostQuitMessage so the message
                    // loop exits *after* the window is gone — the
                    // previous "PostQuitMessage from WM_CLOSE without
                    // destroying" path left a live HWND that the OS
                    // tore down forcibly during process exit, hitting
                    // our globals after they'd been freed (=>
                    // ACCESS_VIOLATION on close).
                    let cb = ON_CLOSE[ix]
                    cb()
                    let _ = DestroyWindow(hwnd as HWND)
                    return 0
                }
                WindowMessage.destroy {
                    PostQuitMessage(0)
                    return 0
                }
                WindowMessage.eraseBkgnd {
                    // Paint the entire client area with the system
                    // dialog grey ourselves. Relying on DefWindowProc
                    // + WNDCLASSEX.hbrBackground works for the
                    // initial WM_PAINT but DWM sometimes skips the
                    // background fill during maximise / per-monitor
                    // DPI transitions, leaving the newly-uncovered
                    // region as the compositor's uninitialised
                    // surface (= black). Doing the fill manually
                    // here side-steps that path entirely.
                    let hdc = wParam as i64 as HDC
                    let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
                    let _ = GetClientRect(hwnd as HWND, &rc)
                    let brush = GetSysColorBrush(15 as i32)   // COLOR_BTNFACE
                    let _ = FillRect(hdc, &rc, brush)
                    return 1
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
                    // For WM_COMMAND from a control: lParam is the
                    // control HWND, HIWORD(wParam) the notify code,
                    // LOWORD(wParam) the control id (= HMENU at
                    // create time). We route by notify code, then
                    // resolve the control by HWND equality so the
                    // BUTTON / EDIT / COMBO id spaces never collide.
                    let ctrlHwnd = lParam as i64
                    let notify = ((wParam >> 16) & 0xFFFF) as i32
                    if notify == 0 {
                        // BN_CLICKED — try buttons first, then
                        // checkboxes (which are also BUTTON class).
                        let n = BUTTON_HWNDS.length as i64
                        let i: i64 = 0
                        while i < n {
                            if BUTTON_HWNDS[i] as i64 == ctrlHwnd {
                                let cb = ON_CLICK[i]
                                cb(0.0, 0.0, 0)
                                return 0
                            }
                            i = i + 1
                        }
                        let m = CHECKBOX_HWNDS.length as i64
                        let j: i64 = 0
                        while j < m {
                            if CHECKBOX_HWNDS[j] as i64 == ctrlHwnd {
                                let cb = ON_TOGGLE[j]
                                cb(readCheckboxChecked(j))
                                return 0
                            }
                            j = j + 1
                        }
                    } elif notify == 0x0300 {
                        // EN_CHANGE — text edited by the user. Read
                        // the fresh contents back and surface them.
                        let n = EDIT_HWNDS.length as i64
                        let i: i64 = 0
                        while i < n {
                            if EDIT_HWNDS[i] as i64 == ctrlHwnd {
                                let cb = ON_TEXT_CHANGE[i]
                                cb(readEditText(i))
                                return 0
                            }
                            i = i + 1
                        }
                    } elif notify == 1 {
                        // CBN_SELCHANGE — user picked a combo item.
                        let n = COMBO_HWNDS.length as i64
                        let i: i64 = 0
                        while i < n {
                            if COMBO_HWNDS[i] as i64 == ctrlHwnd {
                                let cb = ON_COMBO[i]
                                cb(readComboSelectedIndex(i))
                                return 0
                            }
                            i = i + 1
                        }
                    }
                    return 0
                }
                WindowMessage.hScroll {
                    // Trackbars (msctls_trackbar32) post WM_HSCROLL
                    // to their parent for every thumb move. lParam
                    // is the slider HWND; we map back to the
                    // f64 [min, max] range cached at create time.
                    let ctrlHwnd = lParam as i64
                    let n = SLIDER_HWNDS.length as i64
                    let i: i64 = 0
                    while i < n {
                        if SLIDER_HWNDS[i] as i64 == ctrlHwnd {
                            let cb = ON_SLIDER[i]
                            cb(readSliderValue(i))
                            return 0
                        }
                        i = i + 1
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
        dwICC:  Icc.standardClasses as u32
              | Icc.barClasses as u32      // trackbar / toolbar
              | Icc.progressClass as u32,  // progress bar
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
        hCursor:       0 as HCURSOR,
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

// ─── labels (STATIC class) ────────────────────────────────────

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let classW = toWideUtf8("STATIC") as *u16
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
        0 as HMENU,            // labels emit no commands
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, textW as *void)
    let id: i64 = LABEL_HWNDS.length as i64
    LABEL_HWNDS.push(hwnd)
    id
}

pub fn setLabelText(handle: i64, text: string) {
    if handle >= 0 && handle < (LABEL_HWNDS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(LABEL_HWNDS[handle], w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (LABEL_HWNDS.length as i64) {
        MoveWindow(LABEL_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setLabelVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (LABEL_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(LABEL_HWNDS[handle], cmd)
    }
}

// ─── text fields (EDIT class) ────────────────────────────────

pub fn createNativeEdit(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = EDIT_HWNDS.length as i64
    let classW = toWideUtf8("EDIT") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // WS_BORDER so the field reads as an editable input rather
    // than a flat label. ES_AUTOHSCROLL (0x80) makes the field
    // scroll instead of clip when typing past the right edge.
    let style: u32 = (WindowStyle.child as u32)
                   | (WindowStyle.border as u32)
                   | visibleBits
                   | (0x0080 as u32)
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
    EDIT_HWNDS.push(hwnd)
    ON_TEXT_CHANGE.push(noOpText)
    id
}

/// Read the current contents of edit `handle` as a UTF-8 string.
/// Returns "" for an invalid handle or an empty field.
pub fn readEditText(handle: i64): string {
    if handle < 0 || handle >= (EDIT_HWNDS.length as i64) {
        return ""
    }
    let hwnd = EDIT_HWNDS[handle]
    let lenW = GetWindowTextLengthW(hwnd) as i32
    if lenW <= 0 { return "" }
    let heap = GetProcessHeap()
    // (lenW + 1) UTF-16 code units → 2 × bytes for the wide buffer.
    let wideBuf = HeapAlloc(heap, 8 as u32, (lenW as u64 + 1 as u64) * 2 as u64) as *u16
    let _ = GetWindowTextW(hwnd, wideBuf, lenW + 1)
    // Worst-case UTF-8 expansion is 3× per UTF-16 code unit (for
    // BMP code points; surrogate pairs round-trip 1:1 to 4 bytes
    // total, which is also 2 × code units, well below the cap).
    let cap: i32 = (lenW + 1) * 3
    let utf8Buf = HeapAlloc(heap, 8 as u32, cap as u64) as *char
    let _ = WideCharToMultiByte(
        65001 as u32,           // CP_UTF8
        0 as u32,
        wideBuf as *const u16,
        -1 as i32,              // NUL-terminated input
        utf8Buf,
        cap,
        0 as *const char,
        0 as *i32
    )
    let s = stringFromCstr(utf8Buf as *const char)
    HeapFree(heap, 0 as u32, wideBuf as *void)
    HeapFree(heap, 0 as u32, utf8Buf as *void)
    s
}

pub fn setOnEditChange(handle: i64, cb: fn(string)) {
    if handle >= 0 && handle < (ON_TEXT_CHANGE.length as i64) {
        ON_TEXT_CHANGE[handle] = cb
    }
}
pub fn setEditText(handle: i64, text: string) {
    if handle >= 0 && handle < (EDIT_HWNDS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(EDIT_HWNDS[handle], w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setEditFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (EDIT_HWNDS.length as i64) {
        MoveWindow(EDIT_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setEditEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (EDIT_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(EDIT_HWNDS[handle], flag)
    }
}
pub fn setEditVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (EDIT_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(EDIT_HWNDS[handle], cmd)
    }
}

// ─── checkboxes (BUTTON class + BS_AUTOCHECKBOX) ─────────────

pub fn createNativeCheckbox(
    windowHandle: i64,
    text: string, checked: bool,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = CHECKBOX_HWNDS.length as i64
    let classW = toWideUtf8("BUTTON") as *u16
    let textW  = toWideUtf8(text) as *u16
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // BS_AUTOCHECKBOX = 0x0003 — flips state automatically on
    // click, then posts BN_CLICKED so we can forward the new
    // state to user code.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0003 as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        textW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,            // dispatch is by HWND match, not id
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, textW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    if EXPLORER_THEME[0] != 0 as i64 {
        let _ = SetWindowTheme(
            hwnd as i64,
            EXPLORER_THEME[0] as *const u16,
            0 as *const u16
        )
    }
    // BM_SETCHECK = 0x00F1 — BST_CHECKED = 1, BST_UNCHECKED = 0.
    let checkedW: u64 = if checked { 1 as u64 } else { 0 as u64 }
    let _ = SendMessageA(hwnd, 0x00F1 as u32, checkedW, 0 as i64)
    CHECKBOX_HWNDS.push(hwnd)
    ON_TOGGLE.push(noOpBool)
    id
}

pub fn setOnCheckboxToggle(handle: i64, cb: fn(bool)) {
    if handle >= 0 && handle < (ON_TOGGLE.length as i64) {
        ON_TOGGLE[handle] = cb
    }
}
pub fn setCheckboxText(handle: i64, text: string) {
    if handle >= 0 && handle < (CHECKBOX_HWNDS.length as i64) {
        let w = toWideUtf8(text) as *u16
        let _ = SetWindowTextW(CHECKBOX_HWNDS[handle], w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
pub fn setCheckboxChecked(handle: i64, checked: bool) {
    if handle >= 0 && handle < (CHECKBOX_HWNDS.length as i64) {
        let v: u64 = if checked { 1 as u64 } else { 0 as u64 }
        // BM_SETCHECK = 0x00F1
        let _ = SendMessageA(CHECKBOX_HWNDS[handle], 0x00F1 as u32, v, 0 as i64)
    }
}
/// BM_GETCHECK = 0x00F0 — returns BST_CHECKED (1) when ticked.
pub fn readCheckboxChecked(handle: i64): bool {
    if handle < 0 || handle >= (CHECKBOX_HWNDS.length as i64) { return false }
    let r = SendMessageA(CHECKBOX_HWNDS[handle], 0x00F0 as u32, 0 as u64, 0 as i64)
    r == 1 as i64
}
pub fn setCheckboxFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (CHECKBOX_HWNDS.length as i64) {
        MoveWindow(CHECKBOX_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setCheckboxEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (CHECKBOX_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(CHECKBOX_HWNDS[handle], flag)
    }
}
pub fn setCheckboxVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (CHECKBOX_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(CHECKBOX_HWNDS[handle], cmd)
    }
}

// ─── sliders (msctls_trackbar32) ─────────────────────────────
//
// The trackbar takes / reports integer positions; we cache the
// user-facing f64 [min, max] range so the public API can stay
// in f64 even though the native control rounds to int.
//
// Trackbar messages: TBM_SETRANGE = 0x406, TBM_SETPOS = 0x405,
// TBM_GETPOS = 0x400.

fn sliderRangeBits(handle: i64): i64 {
    let mn = SLIDER_MIN[handle] as i32
    let mx = SLIDER_MAX[handle] as i32
    (mn as i64 & 0xFFFF) | ((mx as i64 & 0xFFFF) << 16)
}

pub fn createNativeSlider(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = SLIDER_HWNDS.length as i64
    let classW = toWideUtf8("msctls_trackbar32") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // TBS_HORZ = 0, TBS_AUTOTICKS = 0x0001 — slimmer slider w/
    // automatic tick marks.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | (0x0001 as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)

    SLIDER_HWNDS.push(hwnd)
    ON_SLIDER.push(noOpF64)
    SLIDER_MIN.push(min)
    SLIDER_MAX.push(max)
    // TBM_SETRANGE = 0x406, wParam = redraw flag.
    let _ = SendMessageA(hwnd, 0x0406 as u32, 1 as u64, sliderRangeBits(id))
    // TBM_SETPOS = 0x405, wParam = redraw, lParam = pos.
    let _ = SendMessageA(hwnd, 0x0405 as u32, 1 as u64, value as i64)
    id
}

pub fn setOnSliderChange(handle: i64, cb: fn(f64)) {
    if handle >= 0 && handle < (ON_SLIDER.length as i64) {
        ON_SLIDER[handle] = cb
    }
}
pub fn setSliderRange(handle: i64, min: f64, max: f64) {
    if handle >= 0 && handle < (SLIDER_HWNDS.length as i64) {
        SLIDER_MIN[handle] = min
        SLIDER_MAX[handle] = max
        let _ = SendMessageA(SLIDER_HWNDS[handle], 0x0406 as u32, 1 as u64,
                             sliderRangeBits(handle))
    }
}
pub fn setSliderValue(handle: i64, value: f64) {
    if handle >= 0 && handle < (SLIDER_HWNDS.length as i64) {
        let _ = SendMessageA(SLIDER_HWNDS[handle], 0x0405 as u32, 1 as u64, value as i64)
    }
}
pub fn readSliderValue(handle: i64): f64 {
    if handle < 0 || handle >= (SLIDER_HWNDS.length as i64) { return 0.0 }
    let r = SendMessageA(SLIDER_HWNDS[handle], 0x0400 as u32, 0 as u64, 0 as i64)
    r as f64
}
pub fn setSliderFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (SLIDER_HWNDS.length as i64) {
        MoveWindow(SLIDER_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setSliderEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (SLIDER_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(SLIDER_HWNDS[handle], flag)
    }
}
pub fn setSliderVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (SLIDER_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(SLIDER_HWNDS[handle], cmd)
    }
}

// ─── progress bars (msctls_progress32) ───────────────────────
//
// PBM_SETRANGE32 = 0x0406 (wParam = min, lParam = max), PBM_SETPOS
// = 0x0402 (wParam = pos). The control internally keeps everything
// as i32; we cache the user f64 range and rescale on writes.

pub fn createNativeProgress(
    windowHandle: i64,
    min: f64, max: f64, value: f64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = PROGRESS_HWNDS.length as i64
    let classW = toWideUtf8("msctls_progress32") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)

    PROGRESS_HWNDS.push(hwnd)
    PROGRESS_MIN.push(min)
    PROGRESS_MAX.push(max)
    // PBM_SETRANGE32: wParam = min, lParam = max.
    let _ = SendMessageA(hwnd, 0x0406 as u32, min as i64 as u64, max as i64)
    // PBM_SETPOS: wParam = pos.
    let _ = SendMessageA(hwnd, 0x0402 as u32, value as i64 as u64, 0 as i64)
    id
}

pub fn setProgressRange(handle: i64, min: f64, max: f64) {
    if handle >= 0 && handle < (PROGRESS_HWNDS.length as i64) {
        PROGRESS_MIN[handle] = min
        PROGRESS_MAX[handle] = max
        let _ = SendMessageA(PROGRESS_HWNDS[handle], 0x0406 as u32,
                             min as i64 as u64, max as i64)
    }
}
pub fn setProgressValue(handle: i64, value: f64) {
    if handle >= 0 && handle < (PROGRESS_HWNDS.length as i64) {
        let _ = SendMessageA(PROGRESS_HWNDS[handle], 0x0402 as u32,
                             value as i64 as u64, 0 as i64)
    }
}
pub fn setProgressFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (PROGRESS_HWNDS.length as i64) {
        MoveWindow(PROGRESS_HWNDS[handle],
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setProgressVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (PROGRESS_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(PROGRESS_HWNDS[handle], cmd)
    }
}

// ─── combo boxes (COMBOBOX with CBS_DROPDOWNLIST) ────────────
//
// CBS_DROPDOWNLIST = 0x0003 — non-editable drop-down. The list
// dropping below the box needs vertical room so the combo's
// `height` parameter is the combined (closed) box + dropped-list
// extent; Win32 takes both as one rect, allocates ~24 px to the
// closed box itself, and uses the rest for the popup.

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    if windowHandle < 0 || windowHandle >= (HWNDS.length as i64) {
        return -1
    }
    let parent = HWNDS[windowHandle]
    let id: i64 = COMBO_HWNDS.length as i64
    let classW = toWideUtf8("COMBOBOX") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // CBS_DROPDOWNLIST = 0x3, WS_VSCROLL for tall lists.
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (0x0003 as u32)
                   | (WindowStyle.vScroll as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    // Expand the height so the dropdown list (default ~100 px) is
    // actually usable; callers' `height` is the closed-box height
    // only. We add 120 px of room for the popped list.
    let totalH = (height as i32) + 120
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, totalH,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)

    COMBO_HWNDS.push(hwnd)
    ON_COMBO.push(noOpI32)
    id
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if handle >= 0 && handle < (ON_COMBO.length as i64) {
        ON_COMBO[handle] = cb
    }
}
/// CB_ADDSTRING = 0x143 — append item. Use the wide variant so
/// non-ASCII display strings round-trip.
pub fn addComboItem(handle: i64, item: string) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let w = toWideUtf8(item) as *u16
        let _ = SendMessageW(COMBO_HWNDS[handle], 0x0143 as u32,
                             0 as u64, w as i64)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
/// CB_RESETCONTENT = 0x14B — drop every item.
pub fn clearComboItems(handle: i64) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let _ = SendMessageA(COMBO_HWNDS[handle], 0x014B as u32,
                             0 as u64, 0 as i64)
    }
}
/// CB_SETCURSEL = 0x14E — wParam is the new index (or `-1` = none).
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let _ = SendMessageA(COMBO_HWNDS[handle], 0x014E as u32,
                             index as i64 as u64, 0 as i64)
    }
}
/// CB_GETCURSEL = 0x147 — returns `CB_ERR = -1` when nothing
/// is selected.
pub fn readComboSelectedIndex(handle: i64): i32 {
    if handle < 0 || handle >= (COMBO_HWNDS.length as i64) { return -1 as i32 }
    let r = SendMessageA(COMBO_HWNDS[handle], 0x0147 as u32, 0 as u64, 0 as i64)
    r as i32
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let totalH = (height as i32) + 120
        MoveWindow(COMBO_HWNDS[handle],
                   x as i32, y as i32, width as i32, totalH, 1)
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(COMBO_HWNDS[handle], flag)
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (COMBO_HWNDS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(COMBO_HWNDS[handle], cmd)
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
