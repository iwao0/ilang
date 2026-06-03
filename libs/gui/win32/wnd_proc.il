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
    WindowMessage, VirtualKey, ActivateCode, TrackMouseFlags, TRACKMOUSEEVENT,
    TrackMouseEvent, ScreenToClient,
    LoadCursorA, SetCursor, SystemCursor,
    HDC, RECT, POINT, MINMAXINFO, NMHDR,
    GetClientRect, FillRect, GetSysColorBrush,
}
use std.ffi { readI32, readU64 }
use core {
    dispatchClose, dispatchSize,
    dispatchKeyDown, dispatchKeyUp,
    dispatchMouseMove, dispatchMouseDown, dispatchMouseUp,
    dispatchMouseEnter, dispatchMouseLeave,
    dispatchMouseWheel, dispatchContextMenu,
    windowMouseTracking,
    dispatchFocus, dispatchBlur, dispatchMove,
    contentToFrameSize,
    WINDOWS,
}
use std.ffi { writeI32 }

/// `SC_MOVE` system-command code. Win32 has no native fixed-
/// position flag, so we synthesise `setMovable(false)` by
/// dropping the WM_SYSCOMMAND for SC_MOVE on the floor.
const SC_MOVE_CODE: u64 = 0xF010 as u64
/// `wParam` of WM_SYSCOMMAND has type-bits in the low 4 — the
/// command code lives in the upper 12 of the low 16. Mask to
/// strip the low bits before comparing.
const SC_MASK: u64      = 0xFFF0 as u64
use button   {
    tryButtonClick, tryDefaultEnter,
    tryButtonFocus, tryButtonBlur,
    BN_SETFOCUS, BN_KILLFOCUS,
}
use checkbox { tryCheckboxToggle }
use radio    { tryRadioToggle }
use edit     { tryEditChange, tryEditFocus, tryEditBlur }
use textarea { tryTextAreaChange, tryTextAreaFocus, tryTextAreaBlur }
use combo    { tryComboSelect }
use list     { tryListSelect }
use slider   { trySliderChange }
use tabview  { tryTabViewSelChange }
use menu     { tryMenuClick }
use super.events
use util { vkToString, modCtrl, modShift, modAlt, modMeta }

@extern(C) {

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
            // Default-button routing: this process's message pump
            // does not call `IsDialogMessageW`, so Return wouldn't
            // otherwise activate the button carrying
            // `BS_DEFPUSHBUTTON`. Fire it explicitly. Only matches
            // when the parent window itself has focus (a focused
            // EDIT / BUTTON receives the key on its own HWND first
            // and never bubbles it here).
            let vk = wParam as u32 as VirtualKey
            if vk == VirtualKey.enter {
                if tryDefaultEnter(hwnd) { return 0 }
            }
            dispatchKeyDown(hwnd, vkToString(vk),
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.keyup {
            dispatchKeyUp(hwnd, vkToString(wParam as u32 as VirtualKey),
                          modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mouseMove {
            // First mouseMove since the cursor (re-)entered the
            // window: arm WM_MOUSELEAVE and synthesise enter.
            if !windowMouseTracking(hwnd) {
                let tme = TRACKMOUSEEVENT {
                    cbSize:      24 as u32,
                    dwFlags:     TrackMouseFlags.leave as u32,
                    hwndTrack:   hwnd as HWND,
                    dwHoverTime: 0 as u32,
                }
                TrackMouseEvent(&tme)
                dispatchMouseEnter(hwnd, lo, hi)
            }
            dispatchMouseMove(hwnd, lo, hi, 0,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mouseLeave {
            dispatchMouseLeave(hwnd, lo, hi)
            return 0
        }
        WindowMessage.mouseWheel {
            // HIWORD(wParam) is a signed 16-bit delta in
            // multiples of WHEEL_DELTA (120). Normalise to ±1
            // per detent, matching the cross-platform shape.
            let raw = ((wParam >> 16) & 0xFFFF) as i32
            let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
            let dy = (signed as f64) / 120.0
            // For WM_MOUSEWHEEL lParam holds screen coordinates;
            // ScreenToClient gives us the window-local point the
            // other mouse events use.
            let pt = POINT { x: lo as i32, y: hi as i32 }
            let p: *POINT = &pt
            ScreenToClient(hwnd as HWND, p)
            dispatchMouseWheel(
                hwnd, 0.0, dy, pt.x as f64, pt.y as f64,
                modCtrl(), modShift(), modAlt(), modMeta()
            )
            return 0
        }
        WindowMessage.contextMenu {
            // lParam packs screen-space coords. `0xFFFF` (the
            // sentinel for a keyboard-triggered VK_APPS) skips the
            // dispatch — those have no cursor position.
            let rawX = (lParam & 0xFFFF) as i32
            let rawY = ((lParam >> 16) & 0xFFFF) as i32
            if rawX != 0xFFFF as i32 || rawY != 0xFFFF as i32 {
                let pt = POINT { x: rawX, y: rawY }
                let p: *POINT = &pt
                ScreenToClient(hwnd as HWND, p)
                dispatchContextMenu(
                    hwnd, pt.x as f64, pt.y as f64,
                    modCtrl(), modShift(), modAlt(), modMeta()
                )
            }
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
                // EN_CHANGE — edit / textarea contents changed.
                if tryEditChange(ctrlHwnd) { return 0 }
                if tryTextAreaChange(ctrlHwnd) { return 0 }
            } elif notify == 0x0100 {
                // EN_SETFOCUS — edit / textarea gained focus.
                if tryEditFocus(ctrlHwnd) { return 0 }
                if tryTextAreaFocus(ctrlHwnd) { return 0 }
            } elif notify == 0x0200 {
                // EN_KILLFOCUS — edit / textarea lost focus.
                if tryEditBlur(ctrlHwnd) { return 0 }
                if tryTextAreaBlur(ctrlHwnd) { return 0 }
            } elif notify == 1 {
                // CBN_SELCHANGE / LBN_SELCHANGE share the value.
                if tryComboSelect(ctrlHwnd) { return 0 }
                if tryListSelect(ctrlHwnd)  { return 0 }
            } elif notify == BN_SETFOCUS {
                if tryButtonFocus(ctrlHwnd) { return 0 }
            } elif notify == BN_KILLFOCUS {
                if tryButtonBlur(ctrlHwnd) { return 0 }
            }
            return 0
        }
        WindowMessage.hScroll {
            // Trackbars (msctls_trackbar32) post WM_HSCROLL to
            // their parent for every thumb move; lParam is the
            // slider HWND, LOWORD(wParam) is the trackbar action
            // (TB_THUMBTRACK / TB_ENDTRACK / ...).
            let code = (wParam & 0xFFFF) as i32
            if trySliderChange(lParam as i64, code) { return 0 }
            return 0
        }
        WindowMessage.notify {
            // lParam points to NMHDR; first 8 bytes = hwndFrom,
            // next 8 = idFrom (UINT_PTR), then i32 code.
            let pHdr = lParam as *const void
            let ctrl = readU64(pHdr, 0 as i64) as i64
            let code = readI32(pHdr, 16 as i64)
            if tryTabViewSelChange(ctrl, code) { return 0 }
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
                // MINMAXINFO layout (POINT = 2× i32, 8 B each):
                //   24 ptMinTrackSize.x / 28 .y
                //   32 ptMaxTrackSize.x / 36 .y
                // ilang doesn't parse `(*ptr).field = …` yet, so
                // poke the fields via `writeI32` instead.
                let mmiPtr = lParam as *void
                if slot.minW > 0.0 || slot.minH > 0.0 {
                    let f = contentToFrameSize(hwnd as HWND, slot.minW, slot.minH)
                    writeI32(mmiPtr, 24 as i64, f.fw as i32)
                    writeI32(mmiPtr, 28 as i64, f.fh as i32)
                }
                if slot.hasMaxSize {
                    let f = contentToFrameSize(hwnd as HWND, slot.maxW, slot.maxH)
                    writeI32(mmiPtr, 32 as i64, f.fw as i32)
                    writeI32(mmiPtr, 36 as i64, f.fh as i32)
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
