/// Multi-column table — `SysListView32` in `LVS_REPORT` mode.
/// Columns are inserted via `LVM_INSERTCOLUMNW`, rows via
/// `LVM_INSERTITEMW` (column 0 only) + `LVM_SETITEMTEXTW`
/// (columns 1..N). Selection / activation arrive through
/// `WM_NOTIFY` on the parent, dispatched here via
/// `tryListViewNotify` from `wnd_proc.il`.

use std.ffi { readI32, readU64, readI64 }
use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd, WindowMessage,
    NMHDR, LVCOLUMNW, LVITEMW, NMLISTVIEW,
    LVS_REPORT, LVS_SINGLESEL, LVS_SHOWSELALWAYS,
    LVS_EX_FULLROWSELECT, LVS_EX_GRIDLINES, LVS_EX_DOUBLEBUFFER,
    LVCF_TEXT, LVCF_WIDTH, LVCF_SUBITEM,
    LVIF_TEXT, LVIF_STATE,
    LVIS_SELECTED, LVIS_FOCUSED,
    LVNI_SELECTED,
    LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETITEMTEXTW,
    LVM_DELETEALLITEMS, LVM_GETITEMCOUNT, LVM_GETNEXTITEM,
    LVM_SETITEMSTATE, LVM_ENSUREVISIBLE,
    LVM_SETEXTENDEDLISTVIEWSTYLE,
    LVN_ITEMCHANGED, LVN_ITEMACTIVATE, NM_DBLCLK,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageW, SendMessageA,
}
use util { applyGuiFont }
use core { windowHwnd, noOp1 }

class TableSlot {
    pub hwnd:      HWND
    pub winHandle: i64
    pub colCount:  i32
    pub onSelect:   fn(i32)
    pub onActivate: fn(i32)
    pub onFocus:    fn()
    pub onBlur:     fn()
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd      = hwnd
        this.winHandle = winHandle
        this.colCount  = 0 as i32
        this.onSelect   = tblNoOpI32
        this.onActivate = tblNoOpI32
        this.onFocus    = noOp1
        this.onBlur     = noOp1
    }
}

fn tblNoOpI32(i: i32) {}

let TABLES = new Map<i64, TableSlot>()

/// Dispatch routine called from `wnd_proc` on every WM_NOTIFY
/// whose `hwndFrom` matches one of our tables.
@extern(C) {
pub fn tryListViewNotify(hwnd: i64, code: i32, lParam: i64): bool {
    match TABLES.get(hwnd) {
        some(slot) {
            if code == NM_DBLCLK {
                // NMITEMACTIVATE.iItem at offset 24 (after NMHDR).
                let p = lParam as *const void
                let row = readI32(p, 24 as i64)
                if row >= 0 as i32 {
                    slot.onActivate(row)
                }
                return true
            }
            if code == LVN_ITEMACTIVATE {
                let p = lParam as *const void
                let row = readI32(p, 24 as i64)
                if row >= 0 as i32 {
                    slot.onActivate(row)
                }
                return true
            }
            if code == LVN_ITEMCHANGED {
                // NMLISTVIEW.uNewState at offset 32 (after NMHDR+
                // iItem+iSubItem). Watch the LVIS_SELECTED bit
                // flipping in uNewState to surface selection.
                let p = lParam as *const void
                let row = readI32(p, 24 as i64)
                let newState = readI32(p, 32 as i64) as u32
                let oldState = readI32(p, 36 as i64) as u32
                let nowSel = (newState & LVIS_SELECTED) != 0 as u32
                let wasSel = (oldState & LVIS_SELECTED) != 0 as u32
                if nowSel && !wasSel {
                    slot.onSelect(row)
                }
                return true
            }
            false
        }
        none { false }
    }
}

pub fn createNativeTable(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "SysListView32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (WindowStyle.border as u32)
                   | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS
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
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd)
    // Full-row selection + double-buffer keeps the row reads
    // legible under modern Windows themes.
    SendMessageW(
        hwnd, LVM_SETEXTENDEDLISTVIEWSTYLE,
        0 as u64,
        (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as i64
    )
    let handle = hwnd as i64
    TABLES.set(handle, new TableSlot(hwnd, windowHandle))
    handle
}

pub fn tableAddColumn(tableKey: i64, title: string, width: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        let textW = title.encodeUtf16()
        let pText: *const u16 = textW
        let col = LVCOLUMNW {
            mask:       LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM,
            fmt:        0 as i32,
            cx:         width as i32,
            _pad0:      0 as u32,
            pszText:    pText,
            cchTextMax: 0 as i32,
            iSubItem:   slot.colCount,
            iImage:     0 as i32,
            iOrder:     0 as i32,
            cxMin:      0 as i32,
            cxDefault:  0 as i32,
            cxIdeal:    0 as i32,
        }
        let p: *const LVCOLUMNW = &col
        SendMessageW(
            slot.hwnd, LVM_INSERTCOLUMNW,
            slot.colCount as u64, p as i64
        )
        slot.colCount = slot.colCount + (1 as i32)
    }
}

pub fn tableAddRow(tableKey: i64, cells: string[]) {
    if let some(slot) = TABLES.get(tableKey) {
        let count = SendMessageA(slot.hwnd, LVM_GETITEMCOUNT, 0 as u64, 0 as i64) as i32
        let n = cells.length
        if n == 0 as i64 { return }
        let textW0 = cells[0].encodeUtf16()
        let pText0: *const u16 = textW0
        let lvi = LVITEMW {
            mask:       LVIF_TEXT,
            iItem:      count,
            iSubItem:   0 as i32,
            state:      0 as u32,
            stateMask:  0 as u32,
            _pad0:      0 as u32,
            pszText:    pText0,
            cchTextMax: 0 as i32,
            iImage:     0 as i32,
            lParam:     0 as i64,
            iIndent:    0 as i32,
            _pad1:      0 as i32,
            iGroupId:   0 as i32,
            _pad2:      0 as i32,
            cColumns:   0 as u32,
            _pad3:      0 as u32,
            puColumns:  0 as *void,
            piColFmt:   0 as *void,
            iGroup:     0 as i32,
            _pad4:      0 as i32,
        }
        let p: *const LVITEMW = &lvi
        SendMessageW(slot.hwnd, LVM_INSERTITEMW, 0 as u64, p as i64)
        // SubItems: LVM_SETITEMTEXTW expects a fully-populated
        // LVITEMW with pszText and (via wParam) the row index.
        let i: i64 = 1
        while i < n {
            let textWi = cells[i].encodeUtf16()
            let pTextI: *const u16 = textWi
            let lvi2 = LVITEMW {
                mask:       LVIF_TEXT,
                iItem:      count,
                iSubItem:   i as i32,
                state:      0 as u32,
                stateMask:  0 as u32,
                _pad0:      0 as u32,
                pszText:    pTextI,
                cchTextMax: 0 as i32,
                iImage:     0 as i32,
                lParam:     0 as i64,
                iIndent:    0 as i32,
                _pad1:      0 as i32,
                iGroupId:   0 as i32,
                _pad2:      0 as i32,
                cColumns:   0 as u32,
                _pad3:      0 as u32,
                puColumns:  0 as *void,
                piColFmt:   0 as *void,
                iGroup:     0 as i32,
                _pad4:      0 as i32,
            }
            let p2: *const LVITEMW = &lvi2
            SendMessageW(slot.hwnd, LVM_SETITEMTEXTW, count as u64, p2 as i64)
            i = i + 1
        }
    }
}

pub fn clearTableRows(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        SendMessageA(slot.hwnd, LVM_DELETEALLITEMS, 0 as u64, 0 as i64)
    }
}

pub fn readTableSelectedRow(tableKey: i64): i32 {
    if let some(slot) = TABLES.get(tableKey) {
        let r = SendMessageA(
            slot.hwnd, LVM_GETNEXTITEM,
            (-1 as i64) as u64, LVNI_SELECTED as i64
        )
        r as i32
    } else {
        -1 as i32
    }
}

pub fn setTableSelectedRow(tableKey: i64, row: i32) {
    if let some(slot) = TABLES.get(tableKey) {
        if row < 0 as i32 {
            // Clear selection by setting state on all rows is
            // expensive; skipping the per-row sweep is fine for
            // Round 1.
            return
        }
        let lvi = LVITEMW {
            mask:       LVIF_STATE,
            iItem:      row,
            iSubItem:   0 as i32,
            state:      LVIS_SELECTED | LVIS_FOCUSED,
            stateMask:  LVIS_SELECTED | LVIS_FOCUSED,
            _pad0:      0 as u32,
            pszText:    0 as *const u16,
            cchTextMax: 0 as i32,
            iImage:     0 as i32,
            lParam:     0 as i64,
            iIndent:    0 as i32,
            _pad1:      0 as i32,
            iGroupId:   0 as i32,
            _pad2:      0 as i32,
            cColumns:   0 as u32,
            _pad3:      0 as u32,
            puColumns:  0 as *void,
            piColFmt:   0 as *void,
            iGroup:     0 as i32,
            _pad4:      0 as i32,
        }
        let p: *const LVITEMW = &lvi
        SendMessageW(slot.hwnd, LVM_SETITEMSTATE, row as u64, p as i64)
        SendMessageA(slot.hwnd, LVM_ENSUREVISIBLE, row as u64, 0 as i64)
    }
}

pub fn setTableFrame(tableKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TABLES.get(tableKey) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}

pub fn setTableEnabled(tableKey: i64, enabled: bool) {
    if let some(slot) = TABLES.get(tableKey) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}

pub fn setTableVisible(tableKey: i64, visible: bool) {
    if let some(slot) = TABLES.get(tableKey) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setTableTooltip(tableKey: i64, text: string) {
    let _ = tableKey
    let _ = text
    // SysListView32 manages cell tooltips internally; per-row
    // truncation overflow tips appear without our help. Public
    // setTableTooltip is a no-op on Round 1.
}

pub fn setOnTableSelect(tableKey: i64, cb: fn(i32)) {
    if let some(slot) = TABLES.get(tableKey) { slot.onSelect = cb }
}
pub fn setOnTableActivate(tableKey: i64, cb: fn(i32)) {
    if let some(slot) = TABLES.get(tableKey) { slot.onActivate = cb }
}
pub fn setOnTableFocus(tableKey: i64, cb: fn()) {
    if let some(slot) = TABLES.get(tableKey) { slot.onFocus = cb }
}
pub fn setOnTableBlur(tableKey: i64, cb: fn()) {
    if let some(slot) = TABLES.get(tableKey) { slot.onBlur = cb }
}

// Mouse / key hooks — same Round 1 gap as cocoa/linux.
pub fn setOnTableMouseEnter(tableKey: i64, cb: fn(f64, f64)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableMouseLeave(tableKey: i64, cb: fn(f64, f64)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableMouseDown(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableMouseUp(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableContextMenu(
    tableKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableMouseWheel(
    tableKey: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = tableKey; let _ = cb }
pub fn setOnTableKeyDown(tableKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = tableKey; let _ = cb
}
pub fn setOnTableKeyUp(tableKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = tableKey; let _ = cb
}

pub fn focusTable(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) { SetFocus(slot.hwnd) }
}
pub fn blurTable(tableKey: i64) {
    if let some(slot) = TABLES.get(tableKey) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

} // @extern(C)
