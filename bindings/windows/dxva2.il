/// dxva2.dll — Monitor configuration: physical brightness / contrast
/// / colour-temperature for the displays the system reports.
///
/// Pattern:
///   GetNumberOfPhysicalMonitorsFromHMONITOR(hMon, &n)
///   let pms: PHYSICAL_MONITOR[n] = ...
///   GetPhysicalMonitorsFromHMONITOR(hMon, n, pms)
///   for pm in pms {
///       GetMonitorBrightness(pm.hPhysicalMonitor, &min, &cur, &max)
///       SetMonitorBrightness(pm.hPhysicalMonitor, cur + 10)
///   }
///   DestroyPhysicalMonitors(n, pms)

use user32 { HMONITOR }
use kernel32 { HANDLE }

@extern(C, "dxva2") {
    /// 264-byte descriptor — handle + 256-wchar friendly name.
    pub struct PHYSICAL_MONITOR {
        hPhysicalMonitor:           HANDLE
        szPhysicalMonitorDescription: u16[128]
    }

    /// Count the physical monitors behind an HMONITOR. Some
    /// HMONITORs are virtual (RDP / wireless), so this may be 0.
    @lib pub fn GetNumberOfPhysicalMonitorsFromHMONITOR(
        hMonitor: HMONITOR,
        pdwNumberOfPhysicalMonitors: *u32
    ): i32

    @lib pub fn GetPhysicalMonitorsFromHMONITOR(
        hMonitor:           HMONITOR,
        dwPhysicalMonitorArraySize: u32,
        pPhysicalMonitorArray: *PHYSICAL_MONITOR
    ): i32

    @lib pub fn DestroyPhysicalMonitors(
        dwPhysicalMonitorArraySize: u32,
        pPhysicalMonitorArray: *PHYSICAL_MONITOR
    ): i32

    // ── Brightness ───────────────────────────────────────────────

    /// `GetMonitorBrightness(handle, min, current, max)`. All three
    /// values are 0..100; not all monitors support reads.
    @lib pub fn GetMonitorBrightness(
        hMonitor:        HANDLE,
        pdwMinimumBrightness: *u32,
        pdwCurrentBrightness: *u32,
        pdwMaximumBrightness: *u32
    ): i32

    @lib pub fn SetMonitorBrightness(hMonitor: HANDLE, dwNewBrightness: u32): i32

    // ── Contrast ─────────────────────────────────────────────────

    @lib pub fn GetMonitorContrast(
        hMonitor:        HANDLE,
        pdwMinimumContrast: *u32,
        pdwCurrentContrast: *u32,
        pdwMaximumContrast: *u32
    ): i32

    @lib pub fn SetMonitorContrast(hMonitor: HANDLE, dwNewContrast: u32): i32

    // ── Colour temperature ──────────────────────────────────────

    /// Returns the supported colour-temperature presets and the
    /// current one. Values are MC_COLOR_TEMPERATURE enum (4000K,
    /// 5000K, 6500K, 7500K, 8200K, 9300K, 10000K, 11500K).
    @lib pub fn GetMonitorColorTemperature(
        hMonitor:               HANDLE,
        pctCurrentColorTemperature: *i32
    ): i32

    @lib pub fn SetMonitorColorTemperature(
        hMonitor:              HANDLE,
        ctCurrentColorTemperature: i32
    ): i32

    // ── Display power state ─────────────────────────────────────

    @lib pub fn GetMonitorCapabilities(
        hMonitor:               HANDLE,
        pdwMonitorCapabilities: *u32,
        pdwSupportedColorTemperatures: *u32
    ): i32

    /// Restore the factory defaults for the named monitor.
    @lib pub fn RestoreMonitorFactoryDefaults(hMonitor: HANDLE): i32

    @lib pub fn RestoreMonitorFactoryColorDefaults(hMonitor: HANDLE): i32

    /// Power state — `pwrLevelCurrent` is one of:
    ///   1 ON, 2 STANDBY, 3 SUSPEND, 4 OFF, 5 OFF_HARD.
    @lib pub fn GetVCPFeatureAndVCPFeatureReply(
        hMonitor:               HANDLE,
        bVCPCode:               u8,
        pvct:                   *i32,
        pdwCurrentValue:        *u32,
        pdwMaximumValue:        *u32
    ): i32

    @lib pub fn SetVCPFeature(
        hMonitor:               HANDLE,
        bVCPCode:               u8,
        dwNewValue:             u32
    ): i32

    @lib pub fn SaveCurrentSettings(hMonitor: HANDLE): i32
}
