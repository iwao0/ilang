/// windef.h — Win32 baseline GDI / window-system declarations.
///
/// Contains the opaque graphical handle types and the small geometry
/// structs (RECT / POINT / SIZE) used across user32, gdi32, comdlg32,
/// uxtheme, gdiplus, and dwmapi. Mirrors the Windows SDK header of
/// the same name; everything here is layout / type only — there are
/// no function imports.

@extern(C) {
    // ── Module / instance handles (HMODULE, HINSTANCE are aliases) ──

    /// Module-base handle returned by `LoadLibrary*` and `GetModuleHandle*`.
    /// On Win32 `HINSTANCE` is the same value; we keep them as distinct
    /// opaque types so signatures stay self-documenting.
    @handle pub struct HMODULE {}

    /// Module-base handle used by `WinMain`, `RegisterClassEx.hInstance`,
    /// resource lookups, etc. Same underlying pointer as `HMODULE`.
    @handle pub struct HINSTANCE {}

    // ── Window-system handles ───────────────────────────────────────

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

    // ── Cursor / icon ────────────────────────────────────────────────

    /// Cursor handle (`LoadCursor`, `SetCursor`).
    @handle pub struct HCURSOR {}

    /// Icon handle (`LoadIcon`, `WNDCLASSEX.hIcon`).
    @handle pub struct HICON {}

    // ── GDI handles ──────────────────────────────────────────────────

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

    // ── Geometry structs ────────────────────────────────────────────

    /// 8-byte `(x, y)` pair in logical or device units.
    pub struct POINT {
        x: i32
        y: i32
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
