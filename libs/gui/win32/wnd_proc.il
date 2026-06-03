/// Shared window procedure for every `IlangGuiWindow`-class HWND.
///
/// On WM_* arrival this routes to the matching `dispatchXxx` /
/// `tryHandleXxx` entry point exposed by each per-widget module.
/// Keeping the dispatcher in its own file means no single module
/// has to import every other widget's state — `window.il` owns
/// the per-window callback bundle, `button.il` owns BN_CLICKED
/// fan-out, etc.
///
/// The Win32 → ilang routing convention:
/// - WindowMessage.close / size / keydown / ... — read the
///   slot index out of `GWLP_USERDATA`, forward to `window`.
/// - WindowMessage.command — try each notify-code dispatcher
///   in turn (button click / checkbox / radio / edit change /
///   combo / list); first match wins.
/// - WindowMessage.hScroll — trackbar position changes only;
///   route to `slider`.

use windows {
    HWND, HINSTANCE,
    DefWindowProcW, PostQuitMessage, DestroyWindow,
    WindowMessage, VirtualKey, ActivateCode,
    GetKeyState,
    LoadCursorA, SetCursor, SystemCursor,
    HDC, RECT, POINT, MINMAXINFO,
    GetClientRect, FillRect, GetSysColorBrush,
}
use core {
    dispatchClose, dispatchSize,
    dispatchKeyDown, dispatchKeyUp,
    dispatchMouseMove, dispatchMouseDown, dispatchMouseUp,
    dispatchFocus, dispatchBlur, dispatchMove,
    contentToFrameSize,
    WINDOWS,
}

/// `SC_MOVE` system-command code. Win32 has no native fixed-
/// position flag, so we synthesise `setMovable(false)` by
/// dropping the WM_SYSCOMMAND for SC_MOVE on the floor.
const SC_MOVE_CODE: u64 = 0xF010 as u64
/// `wParam` of WM_SYSCOMMAND has type-bits in the low 4 — the
/// command code lives in the upper 12 of the low 16. Mask to
/// strip the low bits before comparing.
const SC_MASK: u64      = 0xFFF0 as u64
use button   { tryButtonClick }
use checkbox { tryCheckboxToggle }
use radio    { tryRadioToggle }
use edit     { tryEditChange }
use combo    { tryComboSelect }
use list     { tryListSelect }
use slider   { trySliderChange }
use menu     { tryMenuClick }
use super.events
// ─── VirtualKey → W3C KeyboardEvent.code ──────────────────────

pub fn vkToString(vk: VirtualKey): string {
    vkToEnum(vk) as string
}

/// Win32 virtual key codes (`windows.VirtualKey`) → the parent
/// package's `events.KeyCode` (W3C `KeyboardEvent.code`). Unmapped
/// keys fall through to `events.KeyCode.unknown`; the caller then
/// serialises that into the discriminant string for the
/// cross-package callback ABI.
fn vkToEnum(vk: VirtualKey): events.KeyCode {
    match vk {
        VirtualKey.a       { events.KeyCode.keyA }
        VirtualKey.b       { events.KeyCode.keyB }
        VirtualKey.c       { events.KeyCode.keyC }
        VirtualKey.d       { events.KeyCode.keyD }
        VirtualKey.e       { events.KeyCode.keyE }
        VirtualKey.f       { events.KeyCode.keyF }
        VirtualKey.g       { events.KeyCode.keyG }
        VirtualKey.h       { events.KeyCode.keyH }
        VirtualKey.i       { events.KeyCode.keyI }
        VirtualKey.j       { events.KeyCode.keyJ }
        VirtualKey.k       { events.KeyCode.keyK }
        VirtualKey.l       { events.KeyCode.keyL }
        VirtualKey.m       { events.KeyCode.keyM }
        VirtualKey.n       { events.KeyCode.keyN }
        VirtualKey.o       { events.KeyCode.keyO }
        VirtualKey.p       { events.KeyCode.keyP }
        VirtualKey.q       { events.KeyCode.keyQ }
        VirtualKey.r       { events.KeyCode.keyR }
        VirtualKey.s       { events.KeyCode.keyS }
        VirtualKey.t       { events.KeyCode.keyT }
        VirtualKey.u       { events.KeyCode.keyU }
        VirtualKey.v       { events.KeyCode.keyV }
        VirtualKey.w       { events.KeyCode.keyW }
        VirtualKey.x       { events.KeyCode.keyX }
        VirtualKey.y       { events.KeyCode.keyY }
        VirtualKey.z       { events.KeyCode.keyZ }
        VirtualKey.d0      { events.KeyCode.digit0 }
        VirtualKey.d1      { events.KeyCode.digit1 }
        VirtualKey.d2      { events.KeyCode.digit2 }
        VirtualKey.d3      { events.KeyCode.digit3 }
        VirtualKey.d4      { events.KeyCode.digit4 }
        VirtualKey.d5      { events.KeyCode.digit5 }
        VirtualKey.d6      { events.KeyCode.digit6 }
        VirtualKey.d7      { events.KeyCode.digit7 }
        VirtualKey.d8      { events.KeyCode.digit8 }
        VirtualKey.d9      { events.KeyCode.digit9 }
        VirtualKey.enter   { events.KeyCode.enter }
        VirtualKey.escape  { events.KeyCode.escape }
        VirtualKey.back    { events.KeyCode.backspace }
        VirtualKey.tab     { events.KeyCode.tab }
        VirtualKey.space   { events.KeyCode.space }
        VirtualKey.left    { events.KeyCode.arrowLeft }
        VirtualKey.right   { events.KeyCode.arrowRight }
        VirtualKey.down    { events.KeyCode.arrowDown }
        VirtualKey.up      { events.KeyCode.arrowUp }
        _                  { events.KeyCode.unknown }
    }
}

@extern(C) {

// ─── modifier helpers (GetKeyState high-bit set ⇒ pressed) ─────

fn keyDown(vk: i32): bool {
    (GetKeyState(vk) as i32 & 0x8000) != 0
}
fn modShift(): bool { keyDown(VirtualKey.shift as i32) }
fn modCtrl():  bool { keyDown(VirtualKey.control as i32) }
fn modAlt():   bool { keyDown(VirtualKey.alt as i32) }
fn modMeta():  bool { keyDown(VirtualKey.lWin as i32) || keyDown(VirtualKey.rWin as i32) }

/// Window procedure shared by every gui-win32 window. Per-instance
/// state is keyed by the window's HWND in `core.WINDOWS`; the
/// `dispatchXxx` helpers do the Map lookup themselves.
pub fn guiWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    // lParam packs (x, y) for mouse messages and (width, height)
    // for WM_SIZE — same low/high u16 split, pulled here once.
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.close {
            // Fire the user's onClose, then DestroyWindow so
            // WM_DESTROY (below) can PostQuitMessage *after* the
            // window is gone. The old "PostQuitMessage from
            // WM_CLOSE without destroying" path left a live HWND
            // that the OS tore down forcibly at process exit.
            dispatchClose(hwnd)
            DestroyWindow(hwnd as HWND)
            return 0
        }
        WindowMessage.destroy {
            // Modal windows ride a nested message pump in
            // `runNativeWindowModal`; signalling it via
            // `modalLoopExit` lets the surrounding non-modal
            // event loop keep running. Non-modal windows still
            // post `WM_QUIT` which ends the outer pump.
            if let some(slot) = WINDOWS.get(hwnd) {
                if slot.isModal {
                    slot.modalLoopExit = true
                    return 0
                }
            }
            PostQuitMessage(0)
            return 0
        }
        WindowMessage.eraseBkgnd {
            // Paint the client area with COLOR_BTNFACE ourselves.
            // Relying on WNDCLASSEX.hbrBackground works for the
            // initial WM_PAINT but DWM sometimes skips it during
            // maximise / per-monitor DPI transitions, leaving
            // newly-uncovered regions as the compositor's
            // uninitialised surface (= black).
            let hdc = wParam as i64 as HDC
            let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
            GetClientRect(hwnd as HWND, &rc)
            let brush = GetSysColorBrush(15 as i32)   // COLOR_BTNFACE
            FillRect(hdc, &rc, brush)
            return 1
        }
        WindowMessage.setCursor {
            // HTCLIENT (1) → force the standard arrow so the
            // resize-border cursor doesn't leak back in.
            let ht = (lParam & 0xFFFF) as i32
            if ht == 1 {
                let arrow = LoadCursorA(
                    0 as HINSTANCE,
                    SystemCursor.arrow as i64 as *const char
                )
                SetCursor(arrow)
                return 1
            }
        }
        WindowMessage.size {
            dispatchSize(hwnd, wParam as i32, lo, hi)
            return 0
        }
        WindowMessage.move {
            // `lParam` packs the new client-area top-left in
            // screen coordinates (lo = x, hi = y) — same shape
            // `lo` / `hi` already split.
            dispatchMove(hwnd, lo, hi)
            return 0
        }
        WindowMessage.keydown {
            dispatchKeyDown(hwnd, vkToString(wParam as u32 as VirtualKey),
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.keyup {
            dispatchKeyUp(hwnd, vkToString(wParam as u32 as VirtualKey),
                          modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mouseMove {
            dispatchMouseMove(hwnd, lo, hi, 0,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.lbuttondown {
            dispatchMouseDown(hwnd, lo, hi, 0,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.lbuttonup {
            dispatchMouseUp(hwnd, lo, hi, 0,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mButtonDown {
            dispatchMouseDown(hwnd, lo, hi, 1,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mButtonUp {
            dispatchMouseUp(hwnd, lo, hi, 1,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.rButtonDown {
            dispatchMouseDown(hwnd, lo, hi, 2,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.rButtonUp {
            dispatchMouseUp(hwnd, lo, hi, 2,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.command {
            // lParam = control HWND, HIWORD(wParam) = notify code.
            // Route by notify code, resolve the control by HWND
            // equality so each widget's id space stays private.
            let ctrlHwnd = lParam as i64
            let notify = ((wParam >> 16) & 0xFFFF) as i32
            if notify == 0 && ctrlHwnd == 0 {
                // Menu item: lParam is 0, LOWORD(wParam) is the item id.
                let id = (wParam & 0xFFFF) as i32
                if tryMenuClick(id) { return 0 }
            }
            if notify == 0 {
                // BN_CLICKED — button / checkbox / radio share
                // the same notify value; first match wins.
                if tryButtonClick(ctrlHwnd)   { return 0 }
                if tryCheckboxToggle(ctrlHwnd) { return 0 }
                if tryRadioToggle(ctrlHwnd)    { return 0 }
            } elif notify == 0x0300 {
                // EN_CHANGE — edit control contents changed.
                if tryEditChange(ctrlHwnd) { return 0 }
            } elif notify == 1 {
                // CBN_SELCHANGE / LBN_SELCHANGE share the value.
                if tryComboSelect(ctrlHwnd) { return 0 }
                if tryListSelect(ctrlHwnd)  { return 0 }
            }
            return 0
        }
        WindowMessage.hScroll {
            // Trackbars (msctls_trackbar32) post WM_HSCROLL to
            // their parent for every thumb move; lParam is the
            // slider HWND.
            if trySliderChange(lParam as i64) { return 0 }
            return 0
        }
        WindowMessage.sysCommand {
            // `setMovable(false)` synthesised by dropping SC_MOVE
            // on the floor — blocks both the title-bar drag and
            // the Alt+Space → Move keyboard path.
            if (wParam & SC_MASK) == SC_MOVE_CODE {
                if let some(slot) = WINDOWS.get(hwnd) {
                    if !slot.isMovable { return 0 }
                }
            }
        }
        WindowMessage.getMinMaxInfo {
            // `lParam` is a `MINMAXINFO*` Win32 expects us to write
            // tracking bounds into. The user's `setMinimumSize` /
            // `setMaximumSize` are CONTENT sizes; translate via
            // `contentToFrameSize` before stamping the struct so the
            // OS clamps the drag against the right number.
            if let some(slot) = WINDOWS.get(hwnd) {
                let mmi = lParam as *MINMAXINFO
                if slot.minW > 0.0 || slot.minH > 0.0 {
                    let f = contentToFrameSize(hwnd as HWND, slot.minW, slot.minH)
                    (*mmi).ptMinTrackSize = POINT { x: f.fw as i32, y: f.fh as i32 }
                }
                if slot.hasMaxSize {
                    let f = contentToFrameSize(hwnd as HWND, slot.maxW, slot.maxH)
                    (*mmi).ptMaxTrackSize = POINT { x: f.fw as i32, y: f.fh as i32 }
                }
            }
            return 0
        }
        WindowMessage.activate {
            // wParam low-word: ActivateCode (`inactive` / `active`
            // / `clickActive`). Synthesise the focus / blur events
            // so the public `onFocus` / `onBlur` Signals fire
            // alongside the AppKit / GTK paths.
            let code = (wParam & 0xFFFF) as u32 as ActivateCode
            match code {
                inactive    { dispatchBlur(hwnd) }
                active      { dispatchFocus(hwnd) }
                clickActive { dispatchFocus(hwnd) }
                _ {}
            }
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

} // @extern(C)
