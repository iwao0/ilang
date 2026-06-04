/// Status bar — Win32 backend. Uses the `msctls_statusbar32`
/// common control (comctl32) so the bar's frame follows the
/// parent window's WM_SIZE for free. Segment parts are sized
/// equal-width via `SB_SETPARTS`; per-segment text rides
/// `SB_SETTEXTW`.
///
/// Round 1: assumes `InitCommonControlsEx` was already called by
/// the rest of the win32 backend (it is — `progress.il` /
/// `slider.il` already do that on startup).

use windows {
    HWND, HMENU, HINSTANCE, RECT,
    WindowStyle, WindowMessage,
    GetModuleHandleA, CreateWindowExW, SendMessageW, GetClientRect,
}
use core { windowHwnd }

// Common-control class style — CCS_BOTTOM (3) keeps the bar
// docked to the bottom edge and `CCS_NOPARENTALIGN` is implied.
const CCS_BOTTOM: u32 = 0x00000003 as u32

// Message IDs (WM_USER + N). msctls_statusbar32:
//   SB_SETPARTS = WM_USER + 4  (set part count / widths)
//   SB_SETTEXTW = WM_USER + 11 (set part's text)
const SB_SETPARTS: u32 = 0x0404 as u32
const SB_SETTEXTW: u32 = 0x040B as u32

class StatusBarSlot {
    pub hwnd:     HWND
    pub segments: string[]
    pub init(hwnd: HWND, segments: string[]) {
        this.hwnd     = hwnd
        this.segments = segments
    }
}

let STATUS_BARS = new Map<i64, StatusBarSlot>()

@extern(C) {

/// Re-issue SB_SETPARTS + SB_SETTEXTW for every segment. Called
/// at creation time and whenever `setStatusBarSegments` swaps
/// the segment list out.
fn applyParts(slot: StatusBarSlot, segments: string[]) {
    let n = segments.length
    if n == 0 as i64 { return }
    let rc = RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 }
    GetClientRect(slot.hwnd, &rc)
    let totalW = rc.right - rc.left

    // Per-part right edge in pixels — the final part takes -1 so
    // the last segment fills the remaining space exactly.
    let widths: i32[] = []
    let segW = totalW / (n as i32)
    let i = 0 as i64
    while i < n {
        widths.push(segW * ((i as i32) + 1 as i32))
        i = i + 1
    }
    widths[n - 1 as i64] = -1 as i32
    let widthsPtr: *const i32 = widths
    SendMessageW(slot.hwnd, SB_SETPARTS, n as u64, widthsPtr as i64)

    // Now push each segment's text.
    let j = 0 as i64
    while j < n {
        let textW = segments[j].encodeUtf16()
        SendMessageW(slot.hwnd, SB_SETTEXTW, j as u64, textW as i64)
        j = j + 1
    }
}

pub fn createNativeStatusBar(windowHandle: i64, segments: string[]): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }

    let classW = "msctls_statusbar32".encodeUtf16()
    let titleW = "".encodeUtf16()
    let style: u32 = (WindowStyle.child as u32)
                   | (WindowStyle.visible as u32)
                   | CCS_BOTTOM

    let hInstance = GetModuleHandleA(0 as *const char)
    // x/y/w/h are 0 — msctls_statusbar32 sizes itself to the
    // parent's client area on its own WM_SIZE plumbing.
    let hwnd = CreateWindowExW(
        0,
        classW, titleW, style,
        0 as i32, 0 as i32, 0 as i32, 0 as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let handle = hwnd as i64
    let slot = new StatusBarSlot(hwnd, segments)
    STATUS_BARS[handle] = slot
    applyParts(slot, segments)
    handle
}

pub fn setStatusBarSegments(handle: i64, segments: string[]) {
    if let some(slot) = STATUS_BARS.get(handle) {
        slot.segments = segments
        applyParts(slot, segments)
    }
}

pub fn setStatusBarSegmentText(handle: i64, index: i32, text: string) {
    if let some(slot) = STATUS_BARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.segments.length { return }
        slot.segments[i] = text
        let textW = text.encodeUtf16()
        SendMessageW(slot.hwnd, SB_SETTEXTW, index as u64, textW as i64)
    }
}

} // @extern(C)
