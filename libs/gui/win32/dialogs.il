/// Modal dialogs — `MessageBoxW` for alert / confirm, comdlg32's
/// `GetOpenFileNameW` / `GetSaveFileNameW` for the file pickers.
///
/// All three Win32 APIs are synchronous: they spin a nested
/// message loop and return when the user dismisses the dialog.
/// The public-facing callback is invoked just before the wrapper
/// function returns, so the API shape matches the cocoa / linux
/// sheet-modal flow (`window.alert(text, fn() {...})`) even
/// though Win32 doesn't have a sheet primitive.

use windows {
    HWND, HINSTANCE, HDC,
    MessageBoxType, MessageBoxResult,
    MessageBoxW,
    OPENFILENAMEA, GetOpenFileNameW, GetSaveFileNameW,
    CHOOSECOLORA, ChooseColorW, ChooseColorFlags,
    CHOOSEFONTA, ChooseFontW, ChooseFontFlags,
    LOGFONTA, LOGFONTW,
    TaskDialog, TaskDialogButtons, TaskDialogResult,
    HANDLE, GetProcessHeap, HeapAlloc, HeapFree,
}
use std.ffi { arrayFromCArray, readU16, writeU16 }
use core { windowHwnd }

@extern(C) {

/// Walk a null-terminated UTF-16 buffer to recover its length in
/// `u16` units, then convert back to an ilang string.
fn utf16BufToString(buf: *u16): string {
    let len = 0 as i64
    while readU16(buf as *const void, len * 2 as i64) != 0 as u16 {
        len = len + 1
    }
    let units = arrayFromCArray(buf as *const u16, len as u64)
    string.fromUtf16(units)
}

pub fn showAlertDialog(windowHandle: i64, text: string, callback: fn()) {
    let hwnd = windowHwnd(windowHandle)
    let textW = text.encodeUtf16()
    let titleW = "".encodeUtf16()
    let flags = (MessageBoxType.ok as u32)
              | (MessageBoxType.iconInformation as u32)
    MessageBoxW(hwnd, textW, titleW, flags)
    callback()
}

pub fn showConfirmDialog(windowHandle: i64, text: string, callback: fn(bool)) {
    let hwnd = windowHwnd(windowHandle)
    let textW = text.encodeUtf16()
    let titleW = "".encodeUtf16()
    let flags = (MessageBoxType.okCancel as u32)
              | (MessageBoxType.iconQuestion as u32)
    let r = MessageBoxW(hwnd, textW, titleW, flags)
    callback(r == MessageBoxResult.ok as i32)
}

/// `MAX_PATH` slack including trailing NUL. The win32 dialog
/// writes the chosen path here; 1024 covers the long-path case
/// callers might enable.
const PATH_BUF_LEN: i64 = 1024

fn allocPathBuffer(): *u16 {
    let heap = GetProcessHeap()
    // HEAP_ZERO_MEMORY = 0x08 so the buffer starts NUL — saves
    // a manual write before the dialog runs in case the user
    // cancels before typing anything.
    HeapAlloc(heap, 0x08 as u32, (PATH_BUF_LEN as u64) * 2 as u64) as *u16
}

fn freePathBuffer(buf: *u16) {
    HeapFree(GetProcessHeap(), 0 as u32, buf as *void)
}

fn fillOpenFileName(hwnd: HWND, buf: *u16): OPENFILENAMEA {
    OPENFILENAMEA {
        lStructSize:       88 as u32,
        hwndOwner:         hwnd,
        hInstance:         0 as HINSTANCE,
        lpstrFilter:       0 as *const char,
        lpstrCustomFilter: 0 as *char,
        nMaxCustFilter:    0 as u32,
        nFilterIndex:      0 as u32,
        lpstrFile:         buf as *char,
        nMaxFile:          PATH_BUF_LEN as u32,
        lpstrFileTitle:    0 as *char,
        nMaxFileTitle:     0 as u32,
        lpstrInitialDir:   0 as *const char,
        lpstrTitle:        0 as *const char,
        // OFN_EXPLORER (0x80000) | OFN_HIDEREADONLY (0x4)
        // — modern look + drop the redundant read-only checkbox.
        Flags:             0x80004 as u32,
        nFileOffset:       0 as u16,
        nFileExtension:    0 as u16,
        lpstrDefExt:       0 as *const char,
        lCustData:         0 as i64,
        lpfnHook:          0 as *void,
        lpTemplateName:    0 as *const char,
        pvReserved:        0 as *void,
        dwReserved:        0 as u32,
        FlagsEx:           0 as u32,
    }
}

pub fn showOpenFileDialog(windowHandle: i64, callback: fn(string)) {
    let hwnd = windowHwnd(windowHandle)
    let buf = allocPathBuffer()
    let ofn = fillOpenFileName(hwnd, buf)
    let p: *OPENFILENAMEA = &ofn
    if GetOpenFileNameW(p) == 0 as i32 {
        freePathBuffer(buf)
        callback("")
        return
    }
    let s = utf16BufToString(buf)
    freePathBuffer(buf)
    callback(s)
}

pub fn showSaveFileDialog(
    windowHandle: i64, suggestedName: string, callback: fn(string)
) {
    let hwnd = windowHwnd(windowHandle)
    let buf = allocPathBuffer()
    // Pre-seed the buffer with the suggested name so the dialog
    // opens with that text already in the filename field.
    if suggestedName != "" {
        let seedW = suggestedName.encodeUtf16()
        // Copy until the source NUL or until we run out of room.
        let i = 0 as i64
        while i < PATH_BUF_LEN - 1 as i64 {
            let c = readU16(seedW as *const void, i * 2 as i64)
            if c == 0 as u16 { break }
            // 2 bytes per UTF-16 unit; `*(buf + i*2)` write.
            (buf as *u16 + i as i64)[0] = c
            i = i + 1
        }
    }
    let ofn = fillOpenFileName(hwnd, buf)
    let p: *OPENFILENAMEA = &ofn
    if GetSaveFileNameW(p) == 0 as i32 {
        freePathBuffer(buf)
        callback("")
        return
    }
    let s = utf16BufToString(buf)
    freePathBuffer(buf)
    callback(s)
}

// ─── message dialog ──────────────────────────────────────────────

/// Map a `gui.MessageIcon` int onto the `pszIcon` magic value
/// TaskDialog wants. The argument is a `*const u16` but really
/// holds a `MAKEINTRESOURCE(-N)` small integer; cast it through.
fn iconToTaskIcon(icon: i32): *const u16 {
    if icon == 1 as i32 { return (0xFFFFFFFF as i64 - 0 as i64) as *const u16 }
    if icon == 2 as i32 { return (0xFFFFFFFE as i64) as *const u16 }
    if icon == 3 as i32 { return (0xFFFFFFFF as i64 - 0 as i64) as *const u16 }
    // info — TD_INFORMATION_ICON = MAKEINTRESOURCE(-3) = 0xFFFFFFFD
    (0xFFFFFFFD as i64) as *const u16
}

/// Pick a TaskDialog common-button set that matches `n` buttons.
/// The text labels are baked into the OS dialog and don't reflect
/// `buttons[]` — see the public docstring for the trade-off.
fn buttonCountToFlags(n: i64): i32 {
    if n <= 1 as i64 {
        return TaskDialogButtons.ok as i32
    }
    if n == 2 as i64 {
        return (TaskDialogButtons.ok as i32) | (TaskDialogButtons.cancel as i32)
    }
    if n == 3 as i64 {
        return (TaskDialogButtons.yes as i32) |
               (TaskDialogButtons.no as i32) |
               (TaskDialogButtons.cancel as i32)
    }
    // 4+: throw the whole common-button set in. Anything beyond
    // 4 collapses onto the same set; the resulting dialog still
    // closes with a callback index between 0 and N-1.
    (TaskDialogButtons.yes as i32) |
    (TaskDialogButtons.no as i32) |
    (TaskDialogButtons.retry as i32) |
    (TaskDialogButtons.cancel as i32)
}

/// Translate the TaskDialog result code back into the public 0-
/// based button index, respecting the same layout `buttonCountToFlags`
/// chose. Unknown / unexpected codes collapse to `-1` (dialog
/// dismissed without a real pick).
fn taskResultToIndex(result: i32, n: i64): i32 {
    if n <= 1 as i64 {
        if result == TaskDialogResult.ok as i32 { return 0 as i32 }
        return -1 as i32
    }
    if n == 2 as i64 {
        if result == TaskDialogResult.ok as i32     { return 0 as i32 }
        if result == TaskDialogResult.cancel as i32 { return 1 as i32 }
        return -1 as i32
    }
    if n == 3 as i64 {
        if result == TaskDialogResult.yes as i32    { return 0 as i32 }
        if result == TaskDialogResult.no as i32     { return 1 as i32 }
        if result == TaskDialogResult.cancel as i32 { return 2 as i32 }
        return -1 as i32
    }
    if result == TaskDialogResult.yes as i32    { return 0 as i32 }
    if result == TaskDialogResult.no as i32     { return 1 as i32 }
    if result == TaskDialogResult.retry as i32  { return 2 as i32 }
    if result == TaskDialogResult.cancel as i32 { return 3 as i32 }
    -1 as i32
}

/// NOTE: win32 TaskDialog's simple form takes a `TaskDialogButtons`
/// mask, not a list of custom button strings. The text labels of
/// `buttons[]` are therefore IGNORED on Windows — the OS draws
/// OK / Yes / No / Cancel / Retry labelled buttons regardless of
/// what you passed. The callback still receives the 0-based index
/// of whichever button the user clicked. Round 3 will graduate to
/// `TaskDialogIndirect` + a hand-laid TASKDIALOGCONFIG to honour
/// the custom labels.
pub fn showMessageDialog(
    windowHandle: i64, text: string, buttons: string[], icon: i32,
    callback: fn(i32)
) {
    let hwnd = windowHwnd(windowHandle)
    let textW = text.encodeUtf16()
    let titleW = "".encodeUtf16()
    let n = buttons.length
    let flags = buttonCountToFlags(n)
    let pszIcon = iconToTaskIcon(icon)
    let pressed: i32 = 0 as i32
    let _ = TaskDialog(
        hwnd, 0 as HINSTANCE,
        titleW, textW, 0 as *const u16,
        flags, pszIcon, &pressed
    )
    callback(taskResultToIndex(pressed, n))
}

// ─── color picker ────────────────────────────────────────────────

/// argb (0xAARRGGBB, ilang's canvas convention) → COLORREF
/// (0x00BBGGRR, what comdlg32 wants). Alpha is dropped — GDI's
/// flat colour model has no alpha channel.
fn argbToColorRef(argb: u32): u32 {
    let mask: u32 = 0xFF as u32
    let r = (argb >> 16) & mask
    let g = (argb >>  8) & mask
    let b =  argb        & mask
    r | (g << 8) | (b << 16)
}

fn colorRefToArgb(cr: u32): u32 {
    let mask: u32 = 0xFF as u32
    let r =  cr        & mask
    let g = (cr >>  8) & mask
    let b = (cr >> 16) & mask
    (0xFF as u32 << 24) | (r << 16) | (g << 8) | b
}

pub fn showColorPickerDialog(
    windowHandle: i64, initial: u32, callback: fn(u32, bool)
) {
    let hwnd = windowHwnd(windowHandle)
    // 16-slot scratch the dialog reads / writes for its custom-
    // colour boxes. Zero-init is fine — it just means the boxes
    // start empty.
    let custom: u32[] = [
        0 as u32, 0 as u32, 0 as u32, 0 as u32,
        0 as u32, 0 as u32, 0 as u32, 0 as u32,
        0 as u32, 0 as u32, 0 as u32, 0 as u32,
        0 as u32, 0 as u32, 0 as u32, 0 as u32,
    ]
    let cc = CHOOSECOLORA {
        lStructSize:    36 as u32,
        hwndOwner:      hwnd,
        hInstance:      0 as HINSTANCE,
        rgbResult:      argbToColorRef(initial),
        lpCustColors:   custom,
        Flags:          (ChooseColorFlags.rgbInit as u32) |
                        (ChooseColorFlags.fullOpen as u32),
        lCustData:      0 as i64,
        lpfnHook:       0 as *void,
        lpTemplateName: 0 as *const char,
    }
    let p: *CHOOSECOLORA = &cc
    if ChooseColorW(p) == 0 as i32 {
        callback(initial, false)
        return
    }
    callback(colorRefToArgb(cc.rgbResult), true)
}

// ─── font picker ─────────────────────────────────────────────────

/// Copy a UTF-8 ilang string into a 32-`u16` `lfFaceName` slot,
/// NUL-terminated and truncated to fit. The buffer is passed by
/// pointer because LOGFONTW's `lfFaceName` is an inline array.
fn writeFaceName(lf: *LOGFONTW, family: string) {
    // lfFaceName sits at offset 28 in LOGFONTW (after 5 × i32 +
    // 8 × u8). 32 u16 = 64 bytes total.
    let off = 28 as i64
    let nameW = family.encodeUtf16()
    let i = 0 as i64
    while i < 31 as i64 {
        let c = readU16(nameW as *const void, i * 2 as i64)
        writeU16(lf as *void, off + i * 2 as i64, c)
        if c == 0 as u16 { return }
        i = i + 1
    }
    writeU16(lf as *void, off + 31 * 2 as i64, 0 as u16)
}

/// Read the family name back out of `lfFaceName` into an ilang
/// string. Stops at the first NUL or at 32 units.
fn readFaceName(lf: *LOGFONTW): string {
    let off = 28 as i64
    let len = 0 as i64
    while len < 32 as i64 {
        let c = readU16(lf as *const void, off + len * 2 as i64)
        if c == 0 as u16 { break }
        len = len + 1
    }
    let units = arrayFromCArray((lf as i64 + off) as *const u16, len as u64)
    string.fromUtf16(units)
}

pub fn showFontPickerDialog(
    windowHandle: i64,
    family: string, size: f64, weight: i32, italic: bool,
    callback: fn(string, f64, i32, bool, bool)
) {
    let hwnd = windowHwnd(windowHandle)
    let lf = LOGFONTW {
        lfHeight:         0 as i32 - (size as i32),
        lfWidth:          0 as i32,
        lfEscapement:     0 as i32,
        lfOrientation:    0 as i32,
        lfWeight:         weight,
        lfItalic:         if italic { 1 as u8 } else { 0 as u8 },
        lfUnderline:      0 as u8,
        lfStrikeOut:      0 as u8,
        lfCharSet:        1 as u8,         // DEFAULT_CHARSET
        lfOutPrecision:   0 as u8,
        lfClipPrecision:  0 as u8,
        lfQuality:        0 as u8,
        lfPitchAndFamily: 0 as u8,
        // Zero-fill the face name slot; writeFaceName below
        // overwrites it with the requested family.
        lfFaceName:       [
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
            0 as u16, 0 as u16, 0 as u16, 0 as u16,
        ],
    }
    writeFaceName(&lf, family)
    let cf = CHOOSEFONTA {
        lStructSize:    60 as u32,
        hwndOwner:      hwnd,
        hDC:            0 as i64 as HDC,
        // CHOOSEFONTA's `lpLogFont: *LOGFONTA` is the ANSI form;
        // for ChooseFontW we pass a LOGFONTW pointer in the same
        // slot. The two structs differ in `lfFaceName`'s element
        // type (u8 vs u16), but everything else lines up so the
        // pointer cast is safe.
        lpLogFont:      &lf as *LOGFONTA,
        iPointSize:     (size * 10.0) as i32,
        Flags:          (ChooseFontFlags.inittologfontstruct as u32) |
                        (ChooseFontFlags.screenFonts as u32) |
                        (ChooseFontFlags.forcefontexist as u32),
        rgbColors:      0 as u32,
        lCustData:      0 as i64,
        lpfnHook:       0 as *void,
        lpTemplateName: 0 as *const char,
        hInstance:      0 as HINSTANCE,
        lpszStyle:      0 as *char,
        nFontType:      0 as u16,
        _pad:           0 as u16,
        nSizeMin:       0 as i32,
        nSizeMax:       0 as i32,
    }
    let p: *CHOOSEFONTA = &cf
    if ChooseFontW(p) == 0 as i32 {
        callback(family, size, weight, italic, false)
        return
    }
    let pickedFamily = readFaceName(&lf)
    // lfHeight is the character height (negative for cell coords).
    let pickedSize: f64 = if lf.lfHeight < 0 as i32 {
        (0 as i32 - lf.lfHeight) as f64
    } else {
        lf.lfHeight as f64
    }
    let pickedWeight: i32 = lf.lfWeight
    let pickedItalic: bool = lf.lfItalic != 0 as u8
    callback(pickedFamily, pickedSize, pickedWeight, pickedItalic, true)
}

} // @extern(C)
