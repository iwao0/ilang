/// Toolbar — Win32 backend. msctls_toolbar32 has no concept of
/// "flexible space", so we roll our own strip: an outer HWND
/// (`IlangGuiToolbar`) hosts one child per ToolbarItem — a
/// regular `BUTTON` for buttons, a thin `STATIC` for separators,
/// and a virtual zero-width placeholder for flexible spaces.
/// The outer's WndProc lays them out on every WM_SIZE: each
/// flexible-space item absorbs an equal share of whatever width
/// remains after the fixed items + gaps.
///
/// Button clicks come back as WM_COMMAND notifications; the
/// LOWORD of wParam carries the item's index (we assign it as
/// the child HWND's id), so we route directly to
/// `slot.onItemClick`.

use windows {
    HWND, HMENU, HINSTANCE, HBRUSH, HICON, HCURSOR,
    POINT, RECT, WNDCLASSEXA,
    ClassStyle, WindowStyle, WindowMessage, ShowCmd,
    GetModuleHandleA, RegisterClassExW, CreateWindowExW,
    DefWindowProcW, MoveWindow, ShowWindow, SetWindowTextW,
    GetClientRect,
}
use core { windowHwnd }

// BS_PUSHBUTTON = 0x00000000 (default); BS_CENTER = 0x00000300
// keeps the title visually centred even if the style word ends
// up with leftover bits.
const BS_PUSHBUTTON: u32 = 0x00000000 as u32
// SS_ETCHEDVERT = 0x12 (vertical etched divider).
const SS_ETCHEDVERT: u32 = 0x00000012 as u32

const TOOLBAR_HEIGHT:    i32 = 36 as i32
const BUTTON_HEIGHT:     i32 = 28 as i32
const SEPARATOR_WIDTH:   i32 = 1 as i32
const SEPARATOR_HEIGHT:  i32 = 20 as i32
const ITEM_GAP:          i32 = 6 as i32
const BUTTON_PADDING:    i32 = 16 as i32

const KIND_BUTTON:    i32 = 0 as i32
const KIND_SEPARATOR: i32 = 1 as i32
const KIND_FLEXIBLE:  i32 = 2 as i32

class ToolbarSlot {
    pub outer:        HWND
    pub items:        HWND[]   // 0 for flexible space (no HWND)
    pub kinds:        i32[]
    pub naturalW:     i32[]    // fixed-item width; 0 for flexible
    pub w:            i32
    pub h:            i32
    pub onItemClick:  fn(i32)
    pub init(outer: HWND, items: HWND[], kinds: i32[], naturalW: i32[], w: i32, h: i32) {
        this.outer       = outer
        this.items       = items
        this.kinds       = kinds
        this.naturalW    = naturalW
        this.w           = w
        this.h           = h
        this.onItemClick = toolbarNoOp
    }
}

fn toolbarNoOp(idx: i32) {}

let TOOLBARS = new Map<i64, ToolbarSlot>()
let TOOLBAR_CLASS_INITED: bool[] = [false]

@extern(C) {

fn buttonNaturalWidth(text: string): i32 {
    let len = text.length as i32
    let est = len * 7 as i32 + BUTTON_PADDING
    if est < 50 as i32 { 50 as i32 } else { est }
}

fn doLayout(slot: ToolbarSlot) {
    let n = slot.items.length
    if n == 0 as i64 { return }
    let fixedSum = 0 as i32
    let flexCount = 0 as i64
    let i = 0 as i64
    while i < n {
        if slot.kinds[i] == KIND_FLEXIBLE {
            flexCount = flexCount + 1 as i64
        } else {
            fixedSum = fixedSum + slot.naturalW[i]
        }
        i = i + 1
    }
    let gapTotal = ITEM_GAP * ((n - 1 as i64) as i32)
    let leftover = slot.w - fixedSum - gapTotal
    if leftover < 0 as i32 { leftover = 0 as i32 }
    let flexEach = if flexCount > 0 as i64 {
        leftover / (flexCount as i32)
    } else {
        0 as i32
    }

    let cursorX = 0 as i32
    let j = 0 as i64
    while j < n {
        let kind = slot.kinds[j]
        let w = if kind == KIND_FLEXIBLE { flexEach } else { slot.naturalW[j] }
        let h = if kind == KIND_BUTTON {
            BUTTON_HEIGHT
        } elif kind == KIND_SEPARATOR {
            SEPARATOR_HEIGHT
        } else {
            0 as i32
        }
        let y = (slot.h - h) / 2 as i32
        if kind != KIND_FLEXIBLE {
            MoveWindow(slot.items[j], cursorX, y, w, h, 1 as i32)
        }
        cursorX = cursorX + w + ITEM_GAP
        j = j + 1
    }
}

pub fn toolbarWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    match msg as WindowMessage {
        WindowMessage.size {
            if let some(slot) = TOOLBARS.get(hwnd) {
                slot.w = (lParam & 0xFFFF) as i32
                slot.h = ((lParam >> 16) & 0xFFFF) as i32
                doLayout(slot)
            }
            return 0
        }
        WindowMessage.command {
            // LOWORD(wParam) = child control id (= our item index).
            if let some(slot) = TOOLBARS.get(hwnd) {
                let idx = (wParam & 0xFFFF) as i32
                let cb = slot.onItemClick
                cb(idx)
            }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

fn registerToolbarClass() {
    if TOOLBAR_CLASS_INITED[0] { return }
    TOOLBAR_CLASS_INITED[0] = true
    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiToolbar".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   toolbarWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 16 as HBRUSH,   // COLOR_BTNFACE
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

pub fn createNativeToolbar(
    windowHandle: i64, kinds: i32[], texts: string[]
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerToolbarClass()

    let classW = "IlangGuiToolbar".encodeUtf16()
    let titleW = "".encodeUtf16()
    let style: u32 = (WindowStyle.child as u32) | (WindowStyle.visible as u32)

    let hInstance = GetModuleHandleA(0 as *const char)
    // Width is set to the parent's client width post-creation
    // through `GetClientRect` + `MoveWindow`. For Round 1 we
    // start at a fixed initial guess; WM_SIZE on the parent
    // doesn't propagate to children automatically — the gui
    // window's resize callback could fix that, but the demo
    // explicitly calls `setToolbar` on a sized window.
    let outerRC = RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 }
    GetClientRect(parent, &outerRC)
    let initW = outerRC.right - outerRC.left
    let outer = CreateWindowExW(
        0,
        classW, titleW, style,
        0 as i32, 0 as i32, initW, TOOLBAR_HEIGHT,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )

    let items:    HWND[] = []
    let naturalW: i32[]  = []
    let kindsCopy: i32[] = []
    let buttonClassW = "BUTTON".encodeUtf16()
    let staticClassW = "STATIC".encodeUtf16()
    let childStyle: u32 = (WindowStyle.child as u32) | (WindowStyle.visible as u32)

    let n = kinds.length
    let i = 0 as i64
    while i < n {
        let kind = kinds[i]
        kindsCopy.push(kind)
        if kind == KIND_BUTTON {
            let textW = texts[i].encodeUtf16()
            let style2: u32 = childStyle | BS_PUSHBUTTON
            // Child id = item index — WM_COMMAND's LOWORD lands here.
            let btn = CreateWindowExW(
                0,
                buttonClassW, textW, style2,
                0 as i32, 0 as i32, 1 as i32, 1 as i32,
                outer, i as HMENU, hInstance as HINSTANCE, 0 as *void
            )
            items.push(btn)
            naturalW.push(buttonNaturalWidth(texts[i]))
        } elif kind == KIND_SEPARATOR {
            let style2: u32 = childStyle | SS_ETCHEDVERT
            let sep = CreateWindowExW(
                0,
                staticClassW, titleW, style2,
                0 as i32, 0 as i32, 1 as i32, 1 as i32,
                outer, i as HMENU, hInstance as HINSTANCE, 0 as *void
            )
            items.push(sep)
            naturalW.push(SEPARATOR_WIDTH)
        } else {
            items.push(0 as HWND)
            naturalW.push(0 as i32)
        }
        i = i + 1
    }

    let handle = outer as i64
    let slot = new ToolbarSlot(outer, items, kindsCopy, naturalW, initW, TOOLBAR_HEIGHT)
    TOOLBARS[handle] = slot
    doLayout(slot)
    handle
}

pub fn setToolbarItemTitle(handle: i64, index: i32, text: string) {
    if let some(slot) = TOOLBARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.kinds.length { return }
        if slot.kinds[i] != KIND_BUTTON { return }
        let textW = text.encodeUtf16()
        SetWindowTextW(slot.items[i], textW)
        slot.naturalW[i] = buttonNaturalWidth(text)
        doLayout(slot)
    }
}

pub fn setOnToolbarItemClick(handle: i64, cb: fn(i32)) {
    if let some(slot) = TOOLBARS.get(handle) {
        slot.onItemClick = cb
    }
}

} // @extern(C)
