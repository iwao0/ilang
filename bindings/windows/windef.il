/// windef.h — Win32 baseline GDI / window-system declarations.
///
/// Contains the opaque graphical handle types and the small geometry
/// structs (RECT / POINT / POINTS / SIZE) used across user32, gdi32,
/// comdlg32, uxtheme, gdiplus, and dwmapi. Mirrors the Windows SDK
/// header of the same name; everything here is layout / type only —
/// there are no function imports.

// ─── HRESULT — common values from winerror.h ─────────────────────
// HRESULT is a 32-bit status code returned by every COM API and many
// Win32 APIs. High bit set = failure. Exposed as `u32` so the hex
// literals from winerror.h drop in unchanged.
pub enum HResult: u32 {
    ok               = 0x00000000
    /// S_FALSE — call succeeded but returned "no" / "nothing to do".
    /// Distinguished from `ok` by callers that care; both are success.
    sFalse           = 0x00000001
    /// E_NOTIMPL — not implemented.
    eNotImpl         = 0x80004001
    /// E_NOINTERFACE — QueryInterface didn't find the IID.
    eNoInterface     = 0x80004002
    /// E_POINTER — invalid pointer argument.
    ePointer         = 0x80004003
    /// E_ABORT — operation aborted by user.
    eAbort           = 0x80004004
    /// E_FAIL — unspecified failure.
    eFail            = 0x80004005
    /// E_UNEXPECTED — catastrophic failure.
    eUnexpected      = 0x8000FFFF
    /// E_ACCESSDENIED — caller lacks the required rights.
    eAccessDenied    = 0x80070005
    /// E_HANDLE — invalid handle.
    eHandle          = 0x80070006
    /// E_OUTOFMEMORY — allocation failure.
    eOutOfMemory     = 0x8007000E
    /// E_INVALIDARG — at least one argument is invalid.
    eInvalidArg      = 0x80070057
    /// RPC_E_CHANGED_MODE — concurrency model mismatch on
    /// `CoInitializeEx` (caller asked for STA, thread is MTA,
    /// or vice versa).
    eRpcChangedMode  = 0x80010106
    /// REGDB_E_CLASSNOTREG — the CLSID isn't registered.
    eClassNotReg     = 0x80040154
    /// CO_E_NOTINITIALIZED — `CoInitialize`/`CoInitializeEx`
    /// hasn't been called on this thread.
    eNotInitialized  = 0x80040111
}

@extern(C) {
    // ── Module / instance handles (HMODULE, HINSTANCE are aliases) ──

    /// Module-base handle returned by `LoadLibrary*` and
    /// `GetModuleHandle*`. On Win32 `HINSTANCE` is the same value;
    /// we keep them as distinct opaque types so signatures stay
    /// self-documenting.
    @handle pub struct HMODULE {}

    /// Module-base handle used by `WinMain`,
    /// `RegisterClassEx.hInstance`, resource lookups, etc. Same
    /// underlying pointer as `HMODULE`.
    @handle pub struct HINSTANCE {}

    // ── Window-system handles ───────────────────────────────────

    /// Window handle. Returned by `CreateWindowEx`; passed to almost
    /// every user32 API.
    @handle pub struct HWND {}

    /// Menu handle (top-level menu bar, popup, or sub-menu).
    @handle pub struct HMENU {}

    /// Accelerator-table handle (`LoadAccelerators` / `TranslateAccelerator`).
    @handle pub struct HACCEL {}

    /// Resource handle (`FindResource` / `LoadResource`).
    @handle pub struct HRSRC {}

    /// Raw-input handle (`WM_INPUT` lParam, `GetRawInputData`).
    @handle pub struct HRAWINPUT {}

    /// Monitor handle (`MonitorFromWindow` / `EnumDisplayMonitors`).
    @handle pub struct HMONITOR {}

    // ── Cursor / icon ────────────────────────────────────────────

    /// Cursor handle (`LoadCursor`, `SetCursor`).
    @handle pub struct HCURSOR {}

    /// Icon handle (`LoadIcon`, `WNDCLASSEX.hIcon`).
    @handle pub struct HICON {}

    // ── GDI handles ──────────────────────────────────────────────

    /// Device context. Drawing destination — returned by `GetDC`,
    /// `BeginPaint`, `CreateCompatibleDC`, etc.
    @handle pub struct HDC {}

    /// Bitmap handle. Created by `CreateBitmap`, `LoadBitmap`,
    /// `CreateCompatibleBitmap`.
    @handle pub struct HBITMAP {}

    /// Brush handle. Stock brush: `(SysColor.window as i64 + 1) as HBRUSH`.
    @handle pub struct HBRUSH {}

    /// Font handle (`CreateFont`, `CreateFontIndirect`).
    @handle pub struct HFONT {}

    /// Pen handle (`CreatePen`, `CreatePenIndirect`).
    @handle pub struct HPEN {}

    /// Region handle (`CreateRectRgn`, `CreatePolygonRgn`, ...).
    @handle pub struct HRGN {}

    /// Palette handle (`CreatePalette`).
    @handle pub struct HPALETTE {}

    /// Generic GDI object handle. Returned by `SelectObject`; can be
    /// cast back to HBITMAP / HBRUSH / HFONT / HPEN / HRGN / HPALETTE.
    @handle pub struct HGDIOBJ {}

    // ── Geometry structs ────────────────────────────────────────

    /// 8-byte `(x, y)` pair in logical or device units (LONG/i32).
    pub struct POINT {
        x: i32
        y: i32
    }

    /// 4-byte short `(x, y)` pair. Used by `WM_MOUSEMOVE` lParam
    /// (`GET_X_LPARAM` / `GET_Y_LPARAM`) and a handful of legacy APIs.
    pub struct POINTS {
        x: i16
        y: i16
    }

    /// 16-byte axis-aligned rectangle. `right` and `bottom` are
    /// half-open in user32 painting APIs.
    pub struct RECT {
        left:   i32
        top:    i32
        right:  i32
        bottom: i32
    }

    /// 8-byte `(cx, cy)` dimensions.
    pub struct SIZE {
        cx: i32
        cy: i32
    }
}
