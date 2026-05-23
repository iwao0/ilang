/// comdlg32.dll — Common dialogs: file open/save, colour picker,
/// font chooser, and print setup.
///
/// Every struct below must be filled, `lStructSize` set to the
/// declared size, and `hwndOwner` set before calling the dialog.
/// All functions return non-zero on OK, zero on Cancel / error;
/// call `CommDlgExtendedError()` to distinguish Cancel from error.

// ─── GetOpenFileNameA / GetSaveFileNameA flags (OFN_*) ───────────
@flags
pub enum OpenFileNameFlags: u32 {
    readOnly             = 0x00000001
    overwritePrompt      = 0x00000002
    hideReadOnly         = 0x00000004
    noChangeDir          = 0x00000008
    showHelp             = 0x00000010
    enableHook           = 0x00000020
    enableTemplate       = 0x00000040
    enableTemplatHandle  = 0x00000080
    noValidate           = 0x00000100
    allowMultiSelect     = 0x00000200
    extensionDifferent   = 0x00000400
    pathMustExist        = 0x00000800
    fileMustExist        = 0x00001000
    createPrompt         = 0x00002000
    shareAware           = 0x00004000
    noReadOnlyReturn     = 0x00008000
    noTestFileCreate     = 0x00010000
    noDereferenceLinks   = 0x00100000
    longNames            = 0x00200000
    explorer             = 0x00080000   // use Explorer-style dialog
    noNetworkButton      = 0x00020000
    noLongNames          = 0x00040000
    strictFileTypes      = 0x04000000
    noChangeDir2         = 0x08000000
    forceSaveShowExt     = 0x01000000
}

// ─── ChooseColorA flags (CC_*) ───────────────────────────────────
@flags
pub enum ChooseColorFlags: u32 {
    rgbInit          = 0x00000001   // use rgbResult as initial color
    showHelp         = 0x00000008
    enableHook       = 0x00000010
    enableTemplate   = 0x00000020
    enableTemplateHandle = 0x00000040
    fullOpen         = 0x00000002   // show custom-colour part
    preventFullOpen  = 0x00000004   // disable custom-colour button
    solidColor       = 0x00000080
    anyColor         = 0x00000100
}

// ─── ChooseFontA flags (CF_*) ────────────────────────────────────
@flags
pub enum ChooseFontFlags: u32 {
    screenFonts      = 0x00000001
    printerFonts     = 0x00000002
    both             = 0x00000003   // = screenFonts | printerFonts
    showHelp         = 0x00000004
    enableHook       = 0x00000008
    enableTemplate   = 0x00000010
    enableTemplateHandle = 0x00000020
    inittologfontstruct  = 0x00000040   // use lpLogFont as initial value
    usestyle         = 0x00000080
    effects          = 0x00000100   // show strikeout / underline / color
    apply            = 0x00000200
    ansionly         = 0x00000400
    novectorfonts    = 0x00000800   // = nooemfonts
    nosimulations    = 0x00001000
    limitsize        = 0x00002000   // enforce nSizeMin / nSizeMax
    fixedpitchonly   = 0x00004000
    wysiwyg          = 0x00008000
    forcefontexist   = 0x00010000
    scalableonly     = 0x00020000
    ttonly           = 0x00040000   // TrueType fonts only
    nofacesel        = 0x00080000
    nostylesel       = 0x00100000
    nosizesel        = 0x00200000
    selectScript     = 0x00400000
    noscriptsel      = 0x00800000
    novertfonts      = 0x01000000
    inactivefonts    = 0x02000000
}

// ─── CommDlgExtendedError return codes ───────────────────────────
pub enum CommDlgError: u32 {
    noError           = 0x0000
    generalCodes      = 0xFFFF
    structSize        = 0x0001
    initialization    = 0x0002
    noTemplate        = 0x0003
    noHinstance       = 0x0004
    loadStrFailure    = 0x0005
    findResFail       = 0x0006
    loadResFail       = 0x0007
    lockResFail       = 0x0008
    memAllocFail      = 0x0009
    memLockFail       = 0x000A
    noHook            = 0x000B
    registerMsgFail   = 0x000C
    invalidTemplateName = 0x0027
    invalidTemplateHandle = 0x0028
    buffertoosmall    = 0x0003   // FNERR_BUFFERTOOSMALL
    invalidfilename   = 0x0005   // FNERR_INVALIDFILENAME
    subtaskfailure    = 0x0006   // FNERR_SUBCLASSFAILURE
}

use user32 { HWND, HINSTANCE, HDC }
use gdi32 { LOGFONTA }

@extern(C, "comdlg32") {
    // ── OPENFILENAMEA — 88 bytes on x64 ──────────────────────────

    /// File open/save dialog descriptor.
    /// `lpstrFilter`: pairs of NUL-terminated strings, e.g.
    ///   "Text Files\0*.txt\0All Files\0*.*\0\0"
    /// `lpstrFile`: output buffer for the selected path(s).
    /// `nMaxFile`: size of that buffer (use ≥ MAX_PATH = 260).
    /// Set `lStructSize` = 88 before calling.
    pub struct OPENFILENAMEA {
        lStructSize:       u32
        hwndOwner:         HWND
        hInstance:         HINSTANCE   // template resource
        lpstrFilter:       *const char
        lpstrCustomFilter: *char
        nMaxCustFilter:    u32
        nFilterIndex:      u32
        lpstrFile:         *char
        nMaxFile:          u32
        lpstrFileTitle:    *char
        nMaxFileTitle:     u32
        lpstrInitialDir:   *const char
        lpstrTitle:        *const char
        Flags:             u32         // OpenFileNameFlags
        nFileOffset:       u16
        nFileExtension:    u16
        lpstrDefExt:       *const char
        lCustData:         i64
        lpfnHook:          *void       // LPOFNHOOKPROC
        lpTemplateName:    *const char
        // Vista+ reserved fields (pvReserved, dwReserved, FlagsEx)
        pvReserved:        *void
        dwReserved:        u32
        FlagsEx:           u32
    }

    // ── CHOOSECOLORA — 36 bytes on x64 ───────────────────────────

    /// Colour-picker dialog descriptor.
    /// `lpCustColors`: pointer to an array of 16 COLORREF (u32) values
    /// for the custom-colour boxes; these are read and written back.
    /// Set `lStructSize` = 36 before calling.
    pub struct CHOOSECOLORA {
        lStructSize:    u32
        hwndOwner:      HWND
        hInstance:      HINSTANCE  // template parent, rarely used
        rgbResult:      u32     // COLORREF — initial and returned color
        lpCustColors:   *u32    // [16] custom colours (caller-allocated)
        Flags:          u32     // ChooseColorFlags
        lCustData:      i64
        lpfnHook:       *void   // LPCCHOOKPROC
        lpTemplateName: *const char
    }

    // ── CHOOSEFONTA — 60 bytes on x64 ────────────────────────────

    /// Font-chooser dialog descriptor.
    /// `lpLogFont` must point to a caller-allocated LOGFONTA.
    /// Set `lStructSize` = 60 before calling.
    pub struct CHOOSEFONTA {
        lStructSize:      u32
        hwndOwner:        HWND
        hDC:              HDC   // printer DC, or NULL for screen
        lpLogFont:        *LOGFONTA
        iPointSize:       i32   // selected size × 10
        Flags:            u32   // ChooseFontFlags
        rgbColors:        u32   // COLORREF — selected colour
        lCustData:        i64
        lpfnHook:         *void // LPCFHOOKPROC
        lpTemplateName:   *const char
        hInstance:        HINSTANCE
        lpszStyle:        *char // style name buffer (CF_USESTYLE)
        nFontType:        u16   // SCREEN_FONTTYPE etc.
        _pad:             u16
        nSizeMin:         i32
        nSizeMax:         i32
    }

    // ── File dialogs ──────────────────────────────────────────────

    /// Shows the "Open" file dialog. Returns non-zero on OK.
    @lib pub fn GetOpenFileNameA(lpofn: *OPENFILENAMEA): i32
    @lib pub fn GetOpenFileNameW(lpofn: *OPENFILENAMEA): i32

    /// Shows the "Save As" file dialog. Returns non-zero on OK.
    @lib pub fn GetSaveFileNameA(lpofn: *OPENFILENAMEA): i32
    @lib pub fn GetSaveFileNameW(lpofn: *OPENFILENAMEA): i32

    // ── Colour picker ─────────────────────────────────────────────

    /// Shows the colour-picker dialog. Returns non-zero on OK;
    /// the selected COLORREF is written to `lpcc.rgbResult`.
    @lib pub fn ChooseColorA(lpcc: *CHOOSECOLORA): i32
    @lib pub fn ChooseColorW(lpcc: *CHOOSECOLORA): i32

    // ── Font chooser ──────────────────────────────────────────────

    /// Shows the font-chooser dialog. Returns non-zero on OK;
    /// the selected font is written back to `*lpLogFont`.
    @lib pub fn ChooseFontA(lpcf: *CHOOSEFONTA): i32
    @lib pub fn ChooseFontW(lpcf: *CHOOSEFONTA): i32

    // ── Error reporting ───────────────────────────────────────────

    /// Returns the extended error code after a dialog function
    /// returns zero. Use to distinguish user-cancel from errors.
    @lib pub fn CommDlgExtendedError(): u32
}
