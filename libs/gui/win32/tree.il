/// Tree (hierarchical list) — `SysTreeView32` common control.
/// Each ilang `TreeNode` maps to an `HTREEITEM` (a unique
/// pointer cast to `i64` on the public surface).
///
/// Checkboxes: `TVS_CHECKBOXES` is set at creation time (the
/// only moment the style takes), so the state-image column is
/// always reserved. Per-row visibility is then achieved by
/// stamping the row's state-image index — `0` hides the
/// checkbox for nodes that opt out (`checkable = false`).
///
/// Notifications flow through `WM_NOTIFY` on the parent window;
/// `wnd_proc.il` calls `tryTreeViewNotify` from its WM_NOTIFY
/// arm to route TVN_SELCHANGED / TVN_ITEMEXPANDED /
/// TVN_ITEMCHANGED.

use std.ffi { readI32, readU64, readI64 }
use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage,
    NMHDR, TVITEMW, TVINSERTSTRUCTW, NMTREEVIEWW,
    TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS,
    TVS_CHECKBOXES,
    TVIF_TEXT, TVIF_STATE, TVIF_HANDLE, TVIF_CHILDREN,
    TVIS_SELECTED, TVIS_EXPANDED, TVIS_STATEIMAGEMASK,
    TVI_ROOT, TVI_LAST,
    TVE_COLLAPSE, TVE_EXPAND,
    TVGN_CARET,
    TVM_INSERTITEMW, TVM_DELETEITEM, TVM_EXPAND, TVM_GETNEXTITEM,
    TVM_SELECTITEM, TVM_GETITEMW, TVM_SETITEMW, TVM_GETITEMSTATE,
    TVN_SELCHANGEDW, TVN_ITEMEXPANDEDW, TVN_ITEMCHANGEDW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageW, SendMessageA,
}
use util { applyGuiFont }
use core { windowHwnd, noOp1 }

class TreeSlot {
    pub hwnd:      HWND
    pub winHandle: i64
    pub onSelect:   fn(i64)
    pub onExpand:   fn(i64)
    pub onCollapse: fn(i64)
    pub onCheck:    fn(i64, bool)
    pub onFocus:    fn()
    pub onBlur:     fn()
    /// Per-node tracked check state, keyed by HTREEITEM. Lets us
    /// detect a state-bit flip on TVN_ITEMCHANGED without polling
    /// the previous value via TVM_GETITEMSTATE every time.
    pub checks: Map<i64, bool>
    /// Per-node `checkable` flag (true → render checkbox).
    pub checkable: Map<i64, bool>
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd      = hwnd
        this.winHandle = winHandle
        this.onSelect   = noOpI64
        this.onExpand   = noOpI64
        this.onCollapse = noOpI64
        this.onCheck    = noOpI64Bool
        this.onFocus    = noOp1
        this.onBlur     = noOp1
        this.checks    = new Map<i64, bool>()
        this.checkable = new Map<i64, bool>()
    }
}

fn noOpI64(h: i64) {}
fn noOpI64Bool(h: i64, v: bool) {}

let TREES = new Map<i64, TreeSlot>()

/// Dispatch routine called from `wnd_proc` on every WM_NOTIFY
/// whose hwndFrom matches a TreeView we know about.
@extern(C) {
pub fn tryTreeViewNotify(hwnd: i64, code: i32, lParam: i64): bool {
    match TREES.get(hwnd) {
        some(slot) {
            if code == TVN_SELCHANGEDW {
                // NMTREEVIEWW.itemNew.hItem at offset 32+8 = 40
                // (NMHDR=24 + action=4 + _pad=4 + itemOld=56 → itemNew at 88; hItem field at +8).
                let pNew = (lParam + 88 as i64) as *const void
                let h = readI64(pNew, 8 as i64)
                slot.onSelect(h)
                return true
            }
            if code == TVN_ITEMEXPANDEDW {
                let pNew = (lParam + 88 as i64) as *const void
                let h = readI64(pNew, 8 as i64)
                let state = readI64(pNew, 16 as i64) as u32
                if (state & TVIS_EXPANDED) != 0 as u32 {
                    slot.onExpand(h)
                } else {
                    slot.onCollapse(h)
                }
                return true
            }
            if code == TVN_ITEMCHANGEDW {
                // NMTVITEMCHANGE layout: NMHDR (24) + uChanged (u32, 4) +
                // _pad (u32, 4) + hItem (HTREEITEM, 8) + uStateNew (u32, 4) +
                // uStateOld (u32, 4).
                let p = lParam as *const void
                let h = readI64(p, 32 as i64)
                let newState = readI32(p, 40 as i64) as u32
                let masked_i: i64 = (newState as i64) & (TVIS_STATEIMAGEMASK as i64)
                let stateImage_i: i64 = masked_i / (0x1000 as i64)
                let nowChecked: bool = stateImage_i == (2 as i64)
                let prev = match slot.checks.get(h) {
                    some(b) { b }
                    none    { false }
                }
                if nowChecked != prev {
                    slot.checks.set(h, nowChecked)
                    slot.onCheck(h, nowChecked)
                }
                return true
            }
            false
        }
        none { false }
    }
}
}

/// Indexed-state-image helper (no checkbox = 0, unchecked = 1,
/// checked = 2). Returns the `state` value pre-shifted into the
/// `INDEXTOSTATEIMAGEMASK` slot.
fn stateImage(idx: u32): u32 {
    (idx << 12 as u32) & TVIS_STATEIMAGEMASK
}

@extern(C) {

pub fn createNativeTree(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "SysTreeView32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // TVS_CHECKBOXES must be set at creation time. Per-row
    // visibility is then governed by the state-image index.
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (WindowStyle.border as u32)
                   | TVS_HASBUTTONS | TVS_HASLINES | TVS_LINESATROOT
                   | TVS_SHOWSELALWAYS | TVS_CHECKBOXES
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
    let handle = hwnd as i64
    TREES.set(handle, new TreeSlot(hwnd, windowHandle))
    handle
}

pub fn addTreeNode(
    treeKey: i64, parentNodeHandle: i64, label: string, checkable: bool
): i64 {
    insertTreeNodeAt(treeKey, parentNodeHandle, -1 as i32, label, checkable)
}

pub fn insertTreeNodeAt(
    treeKey: i64, parentNodeHandle: i64, index: i32,
    label: string, checkable: bool
): i64 {
    match TREES.get(treeKey) {
        some(slot) {
            let textW = label.encodeUtf16()
            let pText: *const u16 = textW
            let hParent: i64 = if parentNodeHandle < 0 as i64 {
                TVI_ROOT
            } else {
                parentNodeHandle
            }
            // index translation: TreeView only exposes
            // "insert after sibling H" — for an MVP we map
            // out-of-range to TVI_LAST (append) and skip
            // arbitrary-position inserts. Mid-list inserts
            // would walk the sibling chain via TVM_GETNEXTITEM.
            let hAfter: i64 = TVI_LAST
            let _ = index
            // No checkbox = state-image 0; unchecked = 1.
            let stImg: u32 = if checkable {
                stateImage(1 as u32)
            } else {
                stateImage(0 as u32)
            }
            let ins = TVINSERTSTRUCTW {
                hParent:      hParent,
                hInsertAfter: hAfter,
                itemex: TVITEMW {
                    mask:           TVIF_TEXT | TVIF_STATE,
                    _pad0:          0 as u32,
                    hItem:          0 as i64,
                    state:          stImg,
                    stateMask:      TVIS_STATEIMAGEMASK,
                    pszText:        pText,
                    cchTextMax:     0 as i32,
                    iImage:         0 as i32,
                    iSelectedImage: 0 as i32,
                    cChildren:      0 as i32,
                    lParam:         0 as i64,
                },
            }
            let p: *const TVINSERTSTRUCTW = &ins
            let h = SendMessageW(slot.hwnd, TVM_INSERTITEMW, 0 as u64, p as i64)
            slot.checks.set(h, false)
            slot.checkable.set(h, checkable)
            h
        }
        none { -1 as i64 }
    }
}

pub fn removeTreeNode(treeKey: i64, nodeHandle: i64) {
    if let some(slot) = TREES.get(treeKey) {
        SendMessageW(slot.hwnd, TVM_DELETEITEM, 0 as u64, nodeHandle)
        let _ = slot.checks.delete(nodeHandle)
        let _ = slot.checkable.delete(nodeHandle)
    }
}

pub fn clearTreeNodes(treeKey: i64) {
    if let some(slot) = TREES.get(treeKey) {
        // TVI_ROOT = 0xFFFF0000 deletes every item.
        SendMessageW(slot.hwnd, TVM_DELETEITEM, 0 as u64, TVI_ROOT)
        slot.checks    = new Map<i64, bool>()
        slot.checkable = new Map<i64, bool>()
    }
}

pub fn setTreeNodeLabel(treeKey: i64, nodeHandle: i64, label: string) {
    if let some(slot) = TREES.get(treeKey) {
        let textW = label.encodeUtf16()
        let pText: *const u16 = textW
        let it = TVITEMW {
            mask:           TVIF_HANDLE | TVIF_TEXT,
            _pad0:          0 as u32,
            hItem:          nodeHandle,
            state:          0 as u32,
            stateMask:      0 as u32,
            pszText:        pText,
            cchTextMax:     0 as i32,
            iImage:         0 as i32,
            iSelectedImage: 0 as i32,
            cChildren:      0 as i32,
            lParam:         0 as i64,
        }
        let p: *const TVITEMW = &it
        SendMessageW(slot.hwnd, TVM_SETITEMW, 0 as u64, p as i64)
    }
}

pub fn setTreeNodeExpanded(treeKey: i64, nodeHandle: i64, expanded: bool) {
    if let some(slot) = TREES.get(treeKey) {
        let verb: u32 = if expanded { TVE_EXPAND } else { TVE_COLLAPSE }
        SendMessageW(slot.hwnd, TVM_EXPAND, verb as u64, nodeHandle)
    }
}

pub fn setTreeNodeChecked(treeKey: i64, nodeHandle: i64, checked: bool) {
    if let some(slot) = TREES.get(treeKey) {
        let checkable: bool = match slot.checkable.get(nodeHandle) {
            some(b) { b }
            none    { false }
        }
        if !checkable { return }
        let stImg = if checked { stateImage(2 as u32) } else { stateImage(1 as u32) }
        let it = TVITEMW {
            mask:           TVIF_HANDLE | TVIF_STATE,
            _pad0:          0 as u32,
            hItem:          nodeHandle,
            state:          stImg,
            stateMask:      TVIS_STATEIMAGEMASK,
            pszText:        0 as *const u16,
            cchTextMax:     0 as i32,
            iImage:         0 as i32,
            iSelectedImage: 0 as i32,
            cChildren:      0 as i32,
            lParam:         0 as i64,
        }
        let p: *const TVITEMW = &it
        SendMessageW(slot.hwnd, TVM_SETITEMW, 0 as u64, p as i64)
        slot.checks.set(nodeHandle, checked)
    }
}

pub fn readTreeSelectedNode(treeKey: i64): i64 {
    match TREES.get(treeKey) {
        some(slot) {
            SendMessageW(slot.hwnd, TVM_GETNEXTITEM, TVGN_CARET as u64, 0 as i64)
        }
        none { -1 as i64 }
    }
}

pub fn setTreeSelectedNode(treeKey: i64, nodeHandle: i64) {
    if let some(slot) = TREES.get(treeKey) {
        SendMessageW(slot.hwnd, TVM_SELECTITEM, TVGN_CARET as u64, nodeHandle)
    }
}

pub fn treeExpandAll(treeKey: i64) {
    let _ = treeKey
    // Recursive expand needs walking the sibling chain via
    // TVM_GETNEXTITEM. Deferred past Round 1.
}

pub fn treeCollapseAll(treeKey: i64) {
    let _ = treeKey
}

pub fn setTreeFrame(treeKey: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TREES.get(treeKey) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}

pub fn setTreeEnabled(treeKey: i64, enabled: bool) {
    if let some(slot) = TREES.get(treeKey) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}

pub fn setTreeVisible(treeKey: i64, visible: bool) {
    if let some(slot) = TREES.get(treeKey) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setTreeTooltip(treeKey: i64, text: string) {
    let _ = treeKey
    let _ = text
    // SysTreeView32 manages an internal tooltip for the item
    // labels; the public tooltip surface is a no-op on Round 1.
}

pub fn setOnTreeSelect(treeKey: i64, cb: fn(i64)) {
    if let some(slot) = TREES.get(treeKey) { slot.onSelect = cb }
}
pub fn setOnTreeExpand(treeKey: i64, cb: fn(i64)) {
    if let some(slot) = TREES.get(treeKey) { slot.onExpand = cb }
}
pub fn setOnTreeCollapse(treeKey: i64, cb: fn(i64)) {
    if let some(slot) = TREES.get(treeKey) { slot.onCollapse = cb }
}
pub fn setOnTreeCheck(treeKey: i64, cb: fn(i64, bool)) {
    if let some(slot) = TREES.get(treeKey) { slot.onCheck = cb }
}
pub fn setOnTreeFocus(treeKey: i64, cb: fn()) {
    if let some(slot) = TREES.get(treeKey) { slot.onFocus = cb }
}
pub fn setOnTreeBlur(treeKey: i64, cb: fn()) {
    if let some(slot) = TREES.get(treeKey) { slot.onBlur = cb }
}

// Mouse/key hooks — same Round 1 gap as the other backends.
pub fn setOnTreeMouseEnter(treeKey: i64, cb: fn(f64, f64)) {
    let _ = treeKey; let _ = cb
}
pub fn setOnTreeMouseLeave(treeKey: i64, cb: fn(f64, f64)) {
    let _ = treeKey; let _ = cb
}
pub fn setOnTreeMouseDown(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeMouseUp(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeContextMenu(
    treeKey: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeMouseWheel(
    treeKey: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = treeKey; let _ = cb }
pub fn setOnTreeKeyDown(treeKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = treeKey; let _ = cb
}
pub fn setOnTreeKeyUp(treeKey: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = treeKey; let _ = cb
}

pub fn focusTree(treeKey: i64) {
    if let some(slot) = TREES.get(treeKey) { SetFocus(slot.hwnd) }
}
pub fn blurTree(treeKey: i64) {
    if let some(slot) = TREES.get(treeKey) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

} // @extern(C)
