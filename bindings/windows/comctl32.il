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

use user32 { HWND, HINSTANCE }

@extern(C, "comctl32") {
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
