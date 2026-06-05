/// Split view — Win32 backend. AppKit / GTK ship native splitter
/// widgets, but Win32 doesn't, so we roll our own from three
/// custom window classes:
///
///   - `IlangGuiSplitView`         — outer container (the public
///                                   handle). WS_CHILD; doesn't
///                                   draw anything itself, just
///                                   re-positions panes / splitters
///                                   inside on WM_SIZE.
///   - `IlangGuiSplitViewPane`     — one per pane. WS_CHILD. Child
///                                   widgets parent under this.
///   - `IlangGuiSplitViewSplitter` — one between every pair of
///                                   adjacent panes. WS_CHILD;
///                                   WndProc handles WM_SETCURSOR
///                                   / WM_LBUTTONDOWN / WM_MOUSEMOVE
///                                   / WM_LBUTTONUP to drag the
///                                   splitter.
///
/// `SplitOrientation.horizontal` (= 0) lays panes left-to-right
/// with vertical splitters; `.vertical` (= 1) stacks them top-to-
/// bottom with horizontal splitters.

use std.math as math
use windows {
    HWND, HMENU, HINSTANCE, HBRUSH, HICON, HCURSOR,
    POINT, RECT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd, SystemCursor,
    GetModuleHandleA, RegisterClassExW, CreateWindowExW,
    DefWindowProcW, MoveWindow, ShowWindow, EnableWindow,
    SetCapture, ReleaseCapture, SetCursor, LoadCursorW,
    GetClientRect, GetCursorPos, ScreenToClient,
}
use core { windowHwnd }

const SPLITTER_W: i32 = 4 as i32

class SplitViewSlot {
    pub outer:       HWND
    pub orientation: i32   // 0 = horizontal, 1 = vertical
    pub panes:       HWND[]
    pub splitters:   HWND[]
    /// Position (in pixels along the main axis) of each splitter's
    /// LEADING edge. `positions.length == splitters.length`.
    pub positions:   i32[]
    pub w:           i32
    pub h:           i32
    pub init(outer: HWND, orientation: i32, w: i32, h: i32) {
        this.outer       = outer
        this.orientation = orientation
        let p: HWND[]  = []
        this.panes  = p
        let s: HWND[]  = []
        this.splitters = s
        let q: i32[]   = []
        this.positions = q
        this.w = w
        this.h = h
    }
}

class SplitterSlot {
    pub splitViewHandle: i64
    pub index:           i32
    pub dragging:        bool
    pub init(splitViewHandle: i64, index: i32) {
        this.splitViewHandle = splitViewHandle
        this.index           = index
        this.dragging        = false
    }
}

let SPLIT_VIEWS = new Map<i64, SplitViewSlot>()
let SPLITTERS   = new Map<i64, SplitterSlot>()
let SPLIT_VIEW_CLASSES_INITED: bool = false

@extern(C) {

/// Re-layout every pane and splitter against the current
/// `positions[]`. Called on every WM_SIZE, every splitter drag,
/// and every `splitViewBeginNewPane`.
fn layoutPanes(slot: SplitViewSlot) {
    let n = slot.panes.length
    if n == 0 as i64 { return }
    let mainExtent = if slot.orientation == 0 as i32 { slot.w } else { slot.h }
    let crossExtent = if slot.orientation == 0 as i32 { slot.h } else { slot.w }

    let prev = 0 as i32
    let i = 0 as i64
    while i < n {
        let next = if i < n - 1 as i64 {
            slot.positions[i]
        } else {
            mainExtent
        }
        let paneExtent = next - prev
        if slot.orientation == 0 as i32 {
            MoveWindow(slot.panes[i], prev, 0 as i32, paneExtent, crossExtent, 1 as i32)
            if i < n - 1 as i64 {
                MoveWindow(slot.splitters[i], next, 0 as i32, SPLITTER_W, crossExtent, 1 as i32)
            }
        } else {
            MoveWindow(slot.panes[i], 0 as i32, prev, crossExtent, paneExtent, 1 as i32)
            if i < n - 1 as i64 {
                MoveWindow(slot.splitters[i], 0 as i32, next, crossExtent, SPLITTER_W, 1 as i32)
            }
        }
        prev = next + SPLITTER_W
        i = i + 1
    }
}

pub fn splitViewWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.size {
            if let some(slot) = SPLIT_VIEWS.get(hwnd) {
                let lo = (lParam & 0xFFFF) as i32
                let hi = ((lParam >> 16) & 0xFFFF) as i32
                slot.w = lo
                slot.h = hi
                layoutPanes(slot)
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

/// Read the splitter's current screen-mouse position back into
/// the parent split view's local coordinates and update the
/// corresponding `positions[i]`, then re-layout.
fn dragSplitter(slot: SplitViewSlot, idx: i32) {
    let pt = POINT { x: 0 as i32, y: 0 as i32 }
    GetCursorPos(&pt)
    ScreenToClient(slot.outer, &pt)
    let raw = if slot.orientation == 0 as i32 { pt.x } else { pt.y }
    // Clamp between the previous splitter (or 0) and the next
    // (or the SplitView's main extent), with SPLITTER_W slack so
    // neighbouring panes can't collapse to negative width.
    let i = idx as i64
    let mainExtent = if slot.orientation == 0 as i32 { slot.w } else { slot.h }
    let minPos = if i == 0 as i64 { 0 as i32 } else { slot.positions[i - 1 as i64] + SPLITTER_W }
    let maxPos = if i == slot.positions.length - 1 as i64 {
        mainExtent - SPLITTER_W
    } else {
        slot.positions[i + 1 as i64] - SPLITTER_W
    }
    let clamped = if raw < minPos { minPos } elif raw > maxPos { maxPos } else { raw }
    slot.positions[i] = clamped
    layoutPanes(slot)
}

pub fn splitterWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.setCursor {
            if let some(splitterSlot) = SPLITTERS.get(hwnd) {
                if let some(slot) = SPLIT_VIEWS.get(splitterSlot.splitViewHandle) {
                    let cursorId: i64 = if slot.orientation == 0 as i32 {
                        SystemCursor.sizeWE as i64
                    } else {
                        SystemCursor.sizeNS as i64
                    }
                    let cursor = LoadCursorW(0 as HINSTANCE, cursorId as *const u16)
                    SetCursor(cursor)
                    return 1 as i64
                }
            }
        }
        WindowMessage.lbuttondown {
            if let some(splitterSlot) = SPLITTERS.get(hwnd) {
                SetCapture(hwnd as HWND)
                splitterSlot.dragging = true
            }
            return 0
        }
        WindowMessage.mouseMove {
            if let some(splitterSlot) = SPLITTERS.get(hwnd) {
                if splitterSlot.dragging {
                    if let some(slot) = SPLIT_VIEWS.get(splitterSlot.splitViewHandle) {
                        dragSplitter(slot, splitterSlot.index)
                    }
                }
            }
            return 0
        }
        WindowMessage.lbuttonup {
            if let some(splitterSlot) = SPLITTERS.get(hwnd) {
                ReleaseCapture()
                splitterSlot.dragging = false
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn splitDefWndProcTrampoline(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerSplitViewClasses() {
    if SPLIT_VIEW_CLASSES_INITED { return }
    SPLIT_VIEW_CLASSES_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)

    // Outer container.
    let outerNameW = "IlangGuiSplitView".encodeUtf16()
    let outerNamePtr: *const u16 = outerNameW
    let wcOuter = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   splitViewWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 6 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (outerNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wcOuter)

    // Pane.
    let paneNameW = "IlangGuiSplitViewPane".encodeUtf16()
    let paneNamePtr: *const u16 = paneNameW
    let wcPane = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   splitDefWndProcTrampoline,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 6 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (paneNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wcPane)

    // Splitter.
    let splNameW = "IlangGuiSplitViewSplitter".encodeUtf16()
    let splNamePtr: *const u16 = splNameW
    let wcSpl = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   splitterWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        // Slightly darker than the pane background so the
        // splitter is visible against it.
        hbrBackground: 16 as HBRUSH,   // COLOR_BTNFACE
        lpszMenuName:  0 as *const char,
        lpszClassName: (splNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wcSpl)
}

pub fn createNativeSplitView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, orientation: i32
): i64 {
    let parent = windowHwnd(parentHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerSplitViewClasses()

    let visibleBits:  u32 = if visible  { WindowStyle.visible  as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits

    let hInstance = GetModuleHandleA(0 as *const char)
    let outerClassW = "IlangGuiSplitView".encodeUtf16()
    let titleW      = "".encodeUtf16()
    let outer = CreateWindowExW(
        0,
        outerClassW, titleW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let handle = outer as i64
    SPLIT_VIEWS[handle] = new SplitViewSlot(
        outer, orientation, width as i32, height as i32
    )
    handle
}

pub fn splitViewBeginNewPane(handle: i64): i64 {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let hInstance = GetModuleHandleA(0 as *const char)
        let titleW = "".encodeUtf16()
        let paneClassW = "IlangGuiSplitViewPane".encodeUtf16()
        let style: u32 = (WindowStyle.child as u32) | (WindowStyle.visible as u32)

        // Create a fresh pane HWND. Its position / size will be
        // set by `layoutPanes` below.
        let pane = CreateWindowExW(
            0,
            paneClassW, titleW, style,
            0 as i32, 0 as i32, 1 as i32, 1 as i32,
            slot.outer, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
        )
        slot.panes.push(pane)

        // From the second pane onward, also create a splitter at
        // the (previous) end of the content rectangle. Its
        // initial position is the midpoint between the existing
        // last splitter (or 0) and the new pane's start.
        let nPanes = slot.panes.length
        if nPanes >= 2 as i64 {
            let mainExtent = if slot.orientation == 0 as i32 { slot.w } else { slot.h }
            let prevPos = if slot.positions.length == 0 as i64 {
                0 as i32
            } else {
                slot.positions[slot.positions.length - 1 as i64] + SPLITTER_W
            }
            // Distribute remaining space evenly.
            let newPos = prevPos + (mainExtent - prevPos) / 2 as i32

            let splClassW = "IlangGuiSplitViewSplitter".encodeUtf16()
            let splitter = CreateWindowExW(
                0,
                splClassW, titleW, style,
                0 as i32, 0 as i32, 1 as i32, 1 as i32,
                slot.outer, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
            )
            slot.splitters.push(splitter)
            slot.positions.push(newPos)
            SPLITTERS[splitter as i64] = new SplitterSlot(
                handle, (slot.splitters.length - 1 as i64) as i32
            )
        }
        layoutPanes(slot)
        return pane as i64
    }
    return -1 as i64
}

pub fn splitViewDocumentHandle(handle: i64): i64 {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let n = slot.panes.length
        if n > 0 as i64 {
            return slot.panes[n - 1 as i64] as i64
        }
        return slot.outer as i64
    }
    return -1 as i64
}

pub fn setSplitViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        // WM_SIZE fired by MoveWindow updates `slot.w / slot.h`
        // and re-runs `layoutPanes` on its own.
        MoveWindow(slot.outer, x as i32, y as i32, w as i32, h as i32, 1 as i32)
    }
}

pub fn setSplitViewVisible(handle: i64, visible: bool) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.outer, cmd)
    }
}

pub fn setSplitViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.outer, flag)
    }
}

pub fn setSplitViewOrientation(handle: i64, orientation: i32) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        slot.orientation = orientation
        layoutPanes(slot)
    }
}

pub fn setSplitViewPosition(handle: i64, dividerIndex: i32, px: f64) {
    if let some(slot) = SPLIT_VIEWS.get(handle) {
        let i = dividerIndex as i64
        if i < 0 as i64 || i >= slot.positions.length { return }
        slot.positions[i] = px as i32
        layoutPanes(slot)
    }
}

} // @extern(C)
