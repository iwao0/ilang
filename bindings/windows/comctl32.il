/// comctl32.dll — Common Controls + TaskDialog.
///
/// Bootstrap (any process that uses common controls v6 needs this):
///
///   let icc = INITCOMMONCONTROLSEX {
///       dwSize: 8,
///       dwICC:  Icc.standardClasses as u32 | Icc.tabClasses as u32,
///   }
///   InitCommonControlsEx(&icc)
///
/// Manifests: comctl32 v6 (the visual-styles variant) requires an
/// application manifest naming the v6.0.0.0 assembly. Without it
/// you get the v5 (Win9x-look) controls.

// ─── ICC_* — InitCommonControlsEx class bits ─────────────────────
@flags
pub enum Icc: u32 {
    listViewClasses    = 0x00000001
    treeViewClasses    = 0x00000002
    barClasses         = 0x00000004
    tabClasses         = 0x00000008
    upDownClass        = 0x00000010
    progressClass      = 0x00000020
    hotkeyClass        = 0x00000040
    animateClass       = 0x00000080
    winNtClasses       = 0x00000100
    win95Classes       = 0x000000FF
    dateClasses        = 0x00000100
    userExClasses      = 0x00000400
    coolClasses        = 0x00000400
    internetClasses    = 0x00000800
    pageScrollerClass  = 0x00001000
    nativeFontctlClass = 0x00002000
    standardClasses    = 0x00004000
    linkClass          = 0x00008000
}

// ─── TaskDialog button-set flags (TDCBF_*) ───────────────────────
@flags
pub enum TaskDialogButtons: i32 {
    ok        = 0x0001
    yes       = 0x0002
    no        = 0x0004
    cancel    = 0x0008
    retry     = 0x0010
    close     = 0x0020
}

// ─── TaskDialog return values ────────────────────────────────────
pub enum TaskDialogResult: i32 {
    ok      = 1
    cancel  = 2
    abort   = 3
    retry   = 4
    ignore  = 5
    yes     = 6
    no      = 7
    close   = 8
}

// ─── Common control window class names ───────────────────────────
// Pass these to CreateWindowExA. They live in static C strings
// inside comctl32 but are most easily referenced by literal here.
//   "SysListView32"     ListView
//   "SysTreeView32"     TreeView
//   "SysTabControl32"   TabControl
//   "msctls_progress32" Progress bar
//   "msctls_trackbar32" Slider
//   "msctls_updown32"   Up-down (spin) control
//   "ToolbarWindow32"   Toolbar
//   "tooltips_class32"  Tooltip
//   "SysHeader32"       Header
//   "ReBarWindow32"     Rebar
//   "Static"            Static (label / image)
//   "SysLink"           Syslink (hyperlink)
//   "Button"            Button (built-in)
//   "Edit"              Edit (built-in)
//   "ComboBox"          ComboBox

// ─── ImageList flags (ILC_*) ─────────────────────────────────────
@flags
pub enum ImageListFlags: u32 {
    color    = 0x00000000
    mask     = 0x00000001
    colorDdb = 0x000000FE
    color4   = 0x00000004
    color8   = 0x00000008
    color16  = 0x00000010
    color24  = 0x00000018
    color32  = 0x00000020
    palette  = 0x00000800
    mirror   = 0x00002000
    perItemMirror = 0x00008000
    originalSize = 0x00010000
    highQualityScale = 0x00020000
}

// ─── Tooltip (TOOLTIPS_CLASS) ────────────────────────────────────
// `CreateWindowExW` class name is "tooltips_class32" (UTF-16).
// `TTM_*` messages travel through `SendMessageW`, `lParam` taking
// a `*TOOLINFOW`. `cbSize` must equal sizeof(TOOLINFOW) = 72 on
// 64-bit Windows.
pub const TTS_ALWAYSTIP:    u32 = 0x01 as u32
pub const TTS_NOPREFIX:     u32 = 0x02 as u32
pub const TTS_BALLOON:      u32 = 0x40 as u32
/// `uId` is an HWND (the tool is the full window). Pairs with
/// `TTF_SUBCLASS` to let the tooltip's own SetWindowSubclass
/// register handlers for tracking the cursor.
pub const TTF_IDISHWND:     u32 = 0x0001 as u32
pub const TTF_SUBCLASS:     u32 = 0x0010 as u32
/// Tooltip control messages. `WM_USER (0x0400) + offset`. The
/// `*W` variants take a wide-character `lpszText`.
pub const TTM_ADDTOOLW:        u32 = 0x0432 as u32
pub const TTM_DELTOOLW:        u32 = 0x0433 as u32
pub const TTM_UPDATETIPTEXTW:  u32 = 0x0439 as u32
pub const TTM_ACTIVATE:        u32 = 0x0401 as u32

use windef { HINSTANCE, HWND, RECT }
@extern(C, "comctl32") {
    /// Tooltip control's tool descriptor. Layout (x64):
    ///   cbSize     u32   0..4
    ///   uFlags     u32   4..8
    ///   hwnd       HWND  8..16
    ///   uId        u64  16..24    (UINT_PTR — usually a control HWND)
    ///   rect       RECT 24..40
    ///   hinst      i64  40..48
    ///   lpszText   *u16 48..56
    ///   lParam     i64  56..64
    ///   lpReserved *void 64..72
    /// Total = 72 bytes. Initialise `cbSize = 72`.
    pub struct TOOLINFOW {
        cbSize:     u32
        uFlags:     u32
        hwnd:       HWND
        uId:        u64
        rect:       RECT
        hinst:      HINSTANCE
        lpszText:   *const u16
        lParam:     i64
        lpReserved: *void
    }

    // ── Bootstrap ────────────────────────────────────────────────

    pub struct INITCOMMONCONTROLSEX {
        dwSize: u32
        dwICC:  u32
    }

    /// Loads the Common Controls DLL and registers the classes
    /// named in `dwICC`. Returns non-zero on success.
    @lib pub fn InitCommonControlsEx(picce: *const INITCOMMONCONTROLSEX): i32

    /// Legacy no-arg variant — registers a fixed subset.
    @lib pub fn InitCommonControls()

    // ── TaskDialog ──────────────────────────────────────────────

    /// Modern message box. `pszMainIcon`:
    ///   MAKEINTRESOURCE(-1) warning, (-2) error, (-3) information,
    ///   (-4) shield, NULL no icon. ilang callers typically pass
    ///   `-3 as *const u16` for the information icon etc.
    ///
    /// Returns 0 (S_OK) on success and writes the chosen button id
    /// to `*pnButton` (TaskDialogResult or a custom radio id).
    @lib pub fn TaskDialog(
        hwndOwner:        HWND,
        hInstance:        HINSTANCE,           // for resource ids
        pszWindowTitle:   *const u16,
        pszMainInstruction: *const u16,
        pszContent:       *const u16,
        dwCommonButtons:  i32,                 // TaskDialogButtons
        pszIcon:          *const u16,
        pnButton:         *i32
    ): i32

    /// Full-form variant taking a TASKDIALOGCONFIG. Layout differs
    /// between x86 and x64; we expose the config as `*void` for
    /// callers that build it byte-wise.
    @lib pub fn TaskDialogIndirect(
        pTaskConfig:    *const void,
        pnButton:       *i32,
        pnRadioButton:  *i32,
        pfVerificationFlagChecked: *i32
    ): i32

    // ── Image lists ─────────────────────────────────────────────

    @handle pub struct HIMAGELIST {}

    @lib pub fn ImageList_Create(
        cx:      i32,
        cy:      i32,
        flags:   u32,
        cInitial: i32,
        cGrow:   i32
    ): HIMAGELIST

    @lib pub fn ImageList_Destroy(himl: HIMAGELIST): i32
    @lib pub fn ImageList_Add(himl: HIMAGELIST, hbmImage: i64, hbmMask: i64): i32
    @lib pub fn ImageList_Remove(himl: HIMAGELIST, i: i32): i32
    @lib pub fn ImageList_GetImageCount(himl: HIMAGELIST): i32
    @lib pub fn ImageList_Draw(himl: HIMAGELIST, i: i32, hdcDst: i64, x: i32, y: i32, fStyle: u32): i32
    @lib pub fn ImageList_ReplaceIcon(himl: HIMAGELIST, i: i32, hicon: i64): i32

    // ── Misc helpers ────────────────────────────────────────────

    // ── Tab control (SysTabControl32) ────────────────────────────

    /// Identify which TCITEMW fields are populated. `mask = TCIF_TEXT`
    /// is enough for a label-only tab.
    pub const TCIF_TEXT:      u32 = 0x0001 as u32
    pub const TCIF_IMAGE:     u32 = 0x0002 as u32
    pub const TCIF_RTLREADING: u32 = 0x0004 as u32
    pub const TCIF_PARAM:     u32 = 0x0008 as u32
    pub const TCIF_STATE:     u32 = 0x0010 as u32

    /// SysTabControl32 messages.
    pub const TCM_GETITEMCOUNT: u32 = 0x1304 as u32
    pub const TCM_DELETEITEM:   u32 = 0x1308 as u32
    pub const TCM_GETCURSEL:    u32 = 0x130B as u32
    pub const TCM_SETCURSEL:    u32 = 0x130C as u32
    pub const TCM_SETITEMW:     u32 = 0x133D as u32
    pub const TCM_INSERTITEMW:  u32 = 0x133E as u32

    /// `TCN_*` notification codes arriving via WM_NOTIFY.
    /// `TCN_SELCHANGE = -551` — sent after the selection changes.
    pub const TCN_SELCHANGE: i32 = -551 as i32

    /// `NMHDR` — header that every WM_NOTIFY payload starts with.
    /// 24 bytes on x86_64 (pointers padded to 8).
    pub struct NMHDR {
        hwndFrom: HWND
        idFrom:   u64
        code:     i32
        _pad:     u32
    }

    /// `TCITEMW` — tab item descriptor passed to TCM_INSERTITEMW /
    /// TCM_SETITEMW. Set `mask = TCIF_TEXT` and point `pszText`
    /// at a NUL-terminated wide string for the label-only path.
    pub struct TCITEMW {
        mask:        u32
        dwState:     u32
        dwStateMask: u32
        _pad0:       u32
        pszText:     *const u16
        cchTextMax:  i32
        iImage:      i32
        lParam:      i64
    }

    // ── Tree view (SysTreeView32) ────────────────────────────────

    /// `TVS_*` window-style bits. `TVS_CHECKBOXES` must be set at
    /// creation time on Vista+ to enable the state-image column
    /// — flipping it via `SetWindowLong` later doesn't take.
    pub const TVS_HASBUTTONS:      u32 = 0x0001 as u32
    pub const TVS_HASLINES:        u32 = 0x0002 as u32
    pub const TVS_LINESATROOT:     u32 = 0x0004 as u32
    pub const TVS_EDITLABELS:      u32 = 0x0008 as u32
    pub const TVS_DISABLEDRAGDROP: u32 = 0x0010 as u32
    pub const TVS_SHOWSELALWAYS:   u32 = 0x0020 as u32
    pub const TVS_CHECKBOXES:      u32 = 0x0100 as u32
    pub const TVS_TRACKSELECT:     u32 = 0x0200 as u32
    pub const TVS_SINGLEEXPAND:    u32 = 0x0400 as u32
    pub const TVS_FULLROWSELECT:   u32 = 0x1000 as u32

    /// `TVIF_*` — which fields of `TVITEMW` the call wants the
    /// system to honour. `TVIF_HANDLE` is required by reads;
    /// `TVIF_TEXT | TVIF_STATE | TVIF_CHILDREN` covers the
    /// inserts we make.
    pub const TVIF_TEXT:       u32 = 0x0001 as u32
    pub const TVIF_IMAGE:      u32 = 0x0002 as u32
    pub const TVIF_PARAM:      u32 = 0x0004 as u32
    pub const TVIF_STATE:      u32 = 0x0008 as u32
    pub const TVIF_HANDLE:     u32 = 0x0010 as u32
    pub const TVIF_SELECTEDIMAGE: u32 = 0x0020 as u32
    pub const TVIF_CHILDREN:   u32 = 0x0040 as u32

    /// `TVIS_*` — item state bits used with `TVIF_STATE`. State-
    /// image bits live in the high nibble of the state field
    /// (`>> 12`) and `INDEXTOSTATEIMAGEMASK(n) = n << 12`.
    /// `n = 0` → no state image (hides the checkbox on this row);
    /// `1` → unchecked, `2` → checked.
    pub const TVIS_SELECTED:        u32 = 0x0002 as u32
    pub const TVIS_EXPANDED:        u32 = 0x0020 as u32
    pub const TVIS_EXPANDEDONCE:    u32 = 0x0040 as u32
    pub const TVIS_EXPANDPARTIAL:   u32 = 0x0080 as u32
    pub const TVIS_STATEIMAGEMASK:  u32 = 0xF000 as u32

    pub const TVI_ROOT:  i64 = 0xFFFF0000 as i64
    pub const TVI_FIRST: i64 = 0xFFFF0001 as i64
    pub const TVI_LAST:  i64 = 0xFFFF0002 as i64
    pub const TVI_SORT:  i64 = 0xFFFF0003 as i64

    /// `TVE_*` — verb codes for `TVM_EXPAND`.
    pub const TVE_COLLAPSE: u32 = 0x0001 as u32
    pub const TVE_EXPAND:   u32 = 0x0002 as u32
    pub const TVE_TOGGLE:   u32 = 0x0003 as u32

    /// `TVGN_*` — selectors for `TVM_GETNEXTITEM`.
    pub const TVGN_ROOT:        u32 = 0x0000 as u32
    pub const TVGN_NEXT:        u32 = 0x0001 as u32
    pub const TVGN_PREVIOUS:    u32 = 0x0002 as u32
    pub const TVGN_PARENT:      u32 = 0x0003 as u32
    pub const TVGN_CHILD:       u32 = 0x0004 as u32
    pub const TVGN_CARET:       u32 = 0x0009 as u32

    /// SysTreeView32 messages (TV_FIRST = 0x1100).
    pub const TVM_INSERTITEMW:   u32 = 0x1132 as u32
    pub const TVM_DELETEITEM:    u32 = 0x1101 as u32
    pub const TVM_EXPAND:        u32 = 0x1102 as u32
    pub const TVM_GETNEXTITEM:   u32 = 0x110A as u32
    pub const TVM_SELECTITEM:    u32 = 0x110B as u32
    pub const TVM_GETITEMW:      u32 = 0x113E as u32
    pub const TVM_SETITEMW:      u32 = 0x113F as u32
    pub const TVM_GETITEMSTATE:  u32 = 0x1127 as u32
    pub const TVM_SETIMAGELIST:  u32 = 0x1109 as u32

    /// `TVN_*` notification codes arriving via WM_NOTIFY.
    pub const TVN_SELCHANGEDW:   i32 = -451 as i32
    pub const TVN_ITEMEXPANDEDW: i32 = -456 as i32
    pub const TVN_ITEMCHANGEDW:  i32 = -454 as i32

    /// `TVITEMW` — item descriptor. Layout (x64):
    ///   mask          u32     0..4
    ///   _pad0         u32     4..8
    ///   hItem         HTREEITEM (i64)  8..16
    ///   state         u32    16..20
    ///   stateMask     u32    20..24
    ///   pszText       *u16   24..32
    ///   cchTextMax    i32    32..36
    ///   iImage        i32    36..40
    ///   iSelectedImage i32   40..44
    ///   cChildren     i32    44..48
    ///   lParam        i64    48..56
    /// Total = 56 bytes on x64.
    pub struct TVITEMW {
        mask:           u32
        _pad0:          u32
        hItem:          i64
        state:          u32
        stateMask:      u32
        pszText:        *const u16
        cchTextMax:     i32
        iImage:         i32
        iSelectedImage: i32
        cChildren:      i32
        lParam:         i64
    }

    /// `TVINSERTSTRUCTW` — handed to `TVM_INSERTITEMW`. The
    /// trailing `itemex` is the inline `TVITEMW`. Layout:
    ///   hParent      HTREEITEM (i64)  0..8
    ///   hInsertAfter HTREEITEM (i64)  8..16
    ///   itemex       TVITEMW        16..72
    /// Total = 72 bytes.
    pub struct TVINSERTSTRUCTW {
        hParent:      i64
        hInsertAfter: i64
        itemex:       TVITEMW
    }

    /// `NMTREEVIEWW` — payload of `WM_NOTIFY` for TVN_SELCHANGED /
    /// TVN_ITEMEXPANDED. Carries old + new item structs plus a
    /// point of action. Layout (x64):
    ///   hdr        NMHDR    0..24
    ///   action     u32     24..28
    ///   _pad       u32     28..32
    ///   itemOld    TVITEMW 32..88
    ///   itemNew    TVITEMW 88..144
    ///   ptDrag.x   i32    144..148
    ///   ptDrag.y   i32    148..152
    pub struct NMTREEVIEWW {
        hdr:      NMHDR
        action:   u32
        _pad:     u32
        itemOld:  TVITEMW
        itemNew:  TVITEMW
        ptDragX:  i32
        ptDragY:  i32
    }

    // ── List view (SysListView32, report mode) ─────────────────

    /// `LVS_*` window-style bits. `LVS_REPORT` is the multi-
    /// column "details" view; we always set it for our `Table`.
    pub const LVS_ICON:           u32 = 0x0000 as u32
    pub const LVS_REPORT:         u32 = 0x0001 as u32
    pub const LVS_SMALLICON:      u32 = 0x0002 as u32
    pub const LVS_LIST:           u32 = 0x0003 as u32
    pub const LVS_SINGLESEL:      u32 = 0x0004 as u32
    pub const LVS_SHOWSELALWAYS:  u32 = 0x0008 as u32
    pub const LVS_NOLABELWRAP:    u32 = 0x0080 as u32
    pub const LVS_AUTOARRANGE:    u32 = 0x0100 as u32
    pub const LVS_EDITLABELS:     u32 = 0x0200 as u32

    /// Extended styles — `LVM_SETEXTENDEDLISTVIEWSTYLE` payload.
    pub const LVS_EX_FULLROWSELECT:  u32 = 0x0020 as u32
    pub const LVS_EX_GRIDLINES:      u32 = 0x0001 as u32
    pub const LVS_EX_DOUBLEBUFFER:   u32 = 0x00010000 as u32

    /// `LVCF_*` — LVCOLUMNW.mask bits.
    pub const LVCF_FMT:       u32 = 0x0001 as u32
    pub const LVCF_WIDTH:     u32 = 0x0002 as u32
    pub const LVCF_TEXT:      u32 = 0x0004 as u32
    pub const LVCF_SUBITEM:   u32 = 0x0008 as u32

    pub const LVCFMT_LEFT:    i32 = 0x0000 as i32
    pub const LVCFMT_RIGHT:   i32 = 0x0001 as i32
    pub const LVCFMT_CENTER:  i32 = 0x0002 as i32

    /// `LVIF_*` — LVITEMW.mask bits.
    pub const LVIF_TEXT:      u32 = 0x0001 as u32
    pub const LVIF_IMAGE:     u32 = 0x0002 as u32
    pub const LVIF_PARAM:     u32 = 0x0004 as u32
    pub const LVIF_STATE:     u32 = 0x0008 as u32
    pub const LVIF_INDENT:    u32 = 0x0010 as u32

    /// `LVIS_*` — item state bits.
    pub const LVIS_FOCUSED:        u32 = 0x0001 as u32
    pub const LVIS_SELECTED:       u32 = 0x0002 as u32
    pub const LVIS_CUT:            u32 = 0x0004 as u32
    pub const LVIS_DROPHILITED:    u32 = 0x0008 as u32
    pub const LVIS_STATEIMAGEMASK: u32 = 0xF000 as u32

    /// `LVNI_*` — selectors for `LVM_GETNEXTITEM`.
    pub const LVNI_ALL:        u32 = 0x0000 as u32
    pub const LVNI_FOCUSED:    u32 = 0x0001 as u32
    pub const LVNI_SELECTED:   u32 = 0x0002 as u32

    /// SysListView32 messages (LVM_FIRST = 0x1000).
    pub const LVM_GETITEMCOUNT:    u32 = 0x1004 as u32
    pub const LVM_DELETEITEM:      u32 = 0x1008 as u32
    pub const LVM_DELETEALLITEMS:  u32 = 0x1009 as u32
    pub const LVM_GETNEXTITEM:     u32 = 0x100C as u32
    pub const LVM_INSERTITEMW:     u32 = 0x104D as u32
    pub const LVM_SETITEMW:        u32 = 0x104C as u32
    pub const LVM_GETITEMW:        u32 = 0x104B as u32
    pub const LVM_SETITEMTEXTW:    u32 = 0x1074 as u32
    pub const LVM_SETITEMSTATE:    u32 = 0x102B as u32
    pub const LVM_GETITEMSTATE:    u32 = 0x102C as u32
    pub const LVM_INSERTCOLUMNW:   u32 = 0x1061 as u32
    pub const LVM_DELETECOLUMN:    u32 = 0x101C as u32
    pub const LVM_SETCOLUMNWIDTH:  u32 = 0x101E as u32
    pub const LVM_ENSUREVISIBLE:   u32 = 0x1013 as u32
    pub const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = 0x1036 as u32

    /// `LVN_*` notification codes arriving via WM_NOTIFY. (Subset.)
    pub const LVN_ITEMCHANGED:  i32 = -101 as i32
    pub const LVN_ITEMACTIVATE: i32 = -114 as i32
    /// `NM_DBLCLK` — generic common-control notification for a
    /// double-click on the item area.
    pub const NM_DBLCLK: i32 = -3 as i32

    /// `LVCOLUMNW` — column descriptor for `LVM_INSERTCOLUMNW`.
    /// Layout (x64):
    ///   mask        u32      0..4
    ///   fmt         i32      4..8
    ///   cx          i32      8..12
    ///   _pad        u32     12..16
    ///   pszText     *u16    16..24
    ///   cchTextMax  i32     24..28
    ///   iSubItem    i32     28..32
    ///   iImage      i32     32..36
    ///   iOrder      i32     36..40
    ///   cxMin       i32     40..44
    ///   cxDefault   i32     44..48
    ///   cxIdeal     i32     48..52
    /// Size = 52 (52 → 56 with trailing pad on the struct slot).
    pub struct LVCOLUMNW {
        mask:        u32
        fmt:         i32
        cx:          i32
        _pad0:       u32
        pszText:     *const u16
        cchTextMax:  i32
        iSubItem:    i32
        iImage:      i32
        iOrder:      i32
        cxMin:       i32
        cxDefault:   i32
        cxIdeal:     i32
    }

    /// `LVITEMW` — item descriptor for `LVM_INSERTITEMW` /
    /// `LVM_SETITEMTEXTW`. We expose every field through
    /// `_WIN32_WINNT >= 0x0600` so the size matches what Vista+
    /// `comctl32` writes.
    /// Layout (x64):
    ///   mask        u32      0..4
    ///   iItem       i32      4..8
    ///   iSubItem    i32      8..12
    ///   state       u32     12..16
    ///   stateMask   u32     16..20
    ///   _pad0       u32     20..24
    ///   pszText     *u16    24..32
    ///   cchTextMax  i32     32..36
    ///   iImage      i32     36..40
    ///   lParam      i64     40..48
    ///   iIndent     i32     48..52
    ///   _pad1       i32     52..56
    ///   iGroupId    i32     56..60
    ///   _pad2       i32     60..64
    ///   cColumns    u32     64..68
    ///   _pad3       u32     68..72
    ///   puColumns   *void   72..80
    ///   piColFmt    *void   80..88
    ///   iGroup      i32     88..92
    /// Size = 96 with trailing pad.
    pub struct LVITEMW {
        mask:        u32
        iItem:       i32
        iSubItem:    i32
        state:       u32
        stateMask:   u32
        _pad0:       u32
        pszText:     *const u16
        cchTextMax:  i32
        iImage:      i32
        lParam:      i64
        iIndent:     i32
        _pad1:       i32
        iGroupId:    i32
        _pad2:       i32
        cColumns:    u32
        _pad3:       u32
        puColumns:   *void
        piColFmt:    *void
        iGroup:      i32
        _pad4:       i32
    }

    /// `NMITEMACTIVATE` — payload of `LVN_ITEMACTIVATE` /
    /// `NM_DBLCLK` on a SysListView32 control. We only read
    /// `hdr` + `iItem`; the rest is here for documentation.
    pub struct NMITEMACTIVATE {
        hdr:        NMHDR
        iItem:      i32
        iSubItem:   i32
        uNewState:  u32
        uOldState:  u32
        uChanged:   u32
        ptActionX:  i32
        ptActionY:  i32
        lParam:     i64
        uKeyFlags:  u32
        _pad:       u32
    }

    /// `NMLISTVIEW` — payload of `LVN_ITEMCHANGED`. `iItem` is
    /// the row, `uChanged`/`uNewState`/`uOldState` describe what
    /// changed (we watch `LVIS_SELECTED` bit flipping in
    /// `uNewState`).
    pub struct NMLISTVIEW {
        hdr:        NMHDR
        iItem:      i32
        iSubItem:   i32
        uNewState:  u32
        uOldState:  u32
        uChanged:   u32
        ptActionX:  i32
        ptActionY:  i32
        lParam:     i64
    }

    /// Default window procedure for subclassed controls. Bind via
    /// `MAKELONG(ctrlId, 0)` callbacks etc.
    @lib pub fn DefSubclassProc(hWnd: HWND, uMsg: u32, wParam: u64, lParam: i64): i64

    /// Subclass a window. `uIdSubclass` distinguishes multiple
    /// subclassings on the same HWND. Returns TRUE on success.
    @lib pub fn SetWindowSubclass(
        hWnd:        HWND,
        pfnSubclass: fn(i64, u32, u64, i64, u64, u64): i64,
        uIdSubclass: u64,
        dwRefData:   u64
    ): i32

    @lib pub fn RemoveWindowSubclass(
        hWnd:        HWND,
        pfnSubclass: fn(i64, u32, u64, i64, u64, u64): i64,
        uIdSubclass: u64
    ): i32
}
