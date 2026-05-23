/// shcore.dll — Shell Core: per-monitor DPI awareness, scale factor,
/// modern command-line / image handling helpers.  Windows 8.1+.

// ─── PROCESS_DPI_AWARENESS values ────────────────────────────────
pub enum DpiAwareness: i32 {
    /// Process is not DPI-aware. The system bitmap-stretches windows
    /// when the DPI is greater than 96.
    unaware       = 0
    /// Process is DPI-aware. Reports the primary monitor's DPI.
    systemAware   = 1
    /// Process is per-monitor DPI aware. Each window reports the
    /// DPI of the monitor it is currently on (Win 8.1+).
    perMonitor    = 2
}

// ─── DPI awareness context (Win 10 1607+; opaque pointer values) ─
// These constants are used as HANDLE values cast to *void.
//   DPI_AWARENESS_CONTEXT_UNAWARE              = -1
//   DPI_AWARENESS_CONTEXT_SYSTEM_AWARE         = -2
//   DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE    = -3
//   DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
//   DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED    = -5

// ─── MONITOR_DPI_TYPE values ─────────────────────────────────────
pub enum MonitorDpiType: i32 {
    effectiveDpi = 0   // for layout (default for desktop apps)
    angularDpi   = 1   // for rotation
    rawDpi       = 2   // physical DPI of the panel
}

// ─── DEVICE_SCALE_FACTOR values (percent × 100) ──────────────────
pub enum ScaleFactor: i32 {
    invalidScale = 0
    scale100     = 100
    scale120     = 120
    scale125     = 125
    scale140     = 140
    scale150     = 150
    scale160     = 160
    scale175     = 175
    scale180     = 180
    scale200     = 200
    scale225     = 225
    scale250     = 250
    scale300     = 300
    scale350     = 350
    scale400     = 400
    scale450     = 450
    scale500     = 500
}

use winnt { HANDLE }
use windef { HMONITOR }
@extern(C, "shcore") {
    // ── Process-level DPI awareness ──────────────────────────────

    /// Sets the process-wide DPI awareness mode. Must be called
    /// before any HWND is created or the system caches the
    /// pre-existing mode.  Returns S_OK (0) on success, E_ACCESSDENIED
    /// (0x80070005) if the mode is already set.
    @lib pub fn SetProcessDpiAwareness(value: i32): i32

    @lib pub fn GetProcessDpiAwareness(
        hProcess: HANDLE,   // 0 for current process
        value:    *i32      // out: DpiAwareness.*
    ): i32

    // ── Per-monitor DPI ──────────────────────────────────────────

    /// Returns the DPI of a monitor in `dpiX` / `dpiY` (typically 96 at 100%).
    /// `dpiType`: MonitorDpiType.*.
    @lib pub fn GetDpiForMonitor(
        hmonitor: HMONITOR,
        dpiType:  i32,
        dpiX:     *u32,
        dpiY:     *u32
    ): i32

    /// Returns the scale factor of a monitor (e.g. 100, 125, 150).
    @lib pub fn GetScaleFactorForMonitor(
        hMon:    HMONITOR,
        pScale:  *i32         // out: ScaleFactor.*
    ): i32

    /// Registers a callback for scale-factor change notifications.
    @lib pub fn RegisterScaleChangeEvent(
        hEvent:  HANDLE,
        pdwCookie: *u32
    ): i32

    @lib pub fn UnregisterScaleChangeEvent(dwCookie: u32): i32

    // Note: SetThreadDpiAwarenessContext / GetDpiForWindow live in
    // user32.dll on Windows 10+, not shcore. Bind those via user32.
    // CreateMemStream lives in shlwapi.dll.
}
