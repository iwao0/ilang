/// `shobjidl.h` — shell object IDs and the `ITaskbarList3` COM
/// interface used to drive the Windows 7+ taskbar button progress
/// affordance, overlay icon, and thumbnail metadata.
///
///   CoInitializeEx(NULL, COINIT_APARTMENTTHREADED)
///   CoCreateInstance(&CLSID_TaskbarList, NULL, CLSCTX_INPROC_SERVER,
///                    &IID_ITaskbarList3, &raw)
///   let tb = raw as ITaskbarList3
///   tb.HrInit()
///   tb.SetProgressState(hwnd, TBPFLAG.normal as u32)
///   tb.SetProgressValue(hwnd, 33 as u64, 100 as u64)
///
/// Only the subset needed for `libs/gui`'s `setProgressBar` is
/// declared in detail — `Thumbbar*` / `SetOverlayIcon` / tab-
/// preview slots are present as placeholders so the vtable
/// offsets after them stay correct, but their argument shapes
/// use raw pointers rather than the higher-fidelity Windows types
/// since callers wanting those should reach for a dedicated
/// binding.

use ole32 { IUnknown }
use winnt { GUID }
use windef { HWND }

// ─── GUIDs ─────────────────────────────────────────────────────

/// `CLSID_TaskbarList = 56FDF344-FD6D-11D0-958A-006097C9A090`.
pub const CLSID_TaskbarList: GUID = GUID {
    Data1: 0x56fdf344, Data2: 0xfd6d, Data3: 0x11d0,
    Data4lo: 0x60008a95, Data4hi: 0x90a0c997,
}

/// `IID_ITaskbarList3 = EA1AFB91-9E28-4B86-90E9-9E9F8A5EEFAF`.
pub const IID_ITaskbarList3: GUID = GUID {
    Data1: 0xea1afb91, Data2: 0x9e28, Data3: 0x4b86,
    Data4lo: 0x9f9ee990, Data4hi: 0xafef5e8a,
}

// ─── TBPFLAG (SetProgressState states) ─────────────────────────

/// `TBPFLAG` — values for `ITaskbarList3.SetProgressState`. Use
/// `noProgress` to remove the bar entirely, `normal` for an
/// ordinary fractional bar driven by `SetProgressValue`, and
/// `indeterminate` to spin without a known total.
pub enum TBPFLAG: u32 {
    noProgress     = 0
    indeterminate  = 1
    normal         = 2
    error          = 4
    paused         = 8
}

@extern(C) {
    /// `ITaskbarList` — Windows 95+ tab-management interface. We
    /// declare the inherited methods here so the vtable offsets
    /// of `ITaskbarList2` / `ITaskbarList3` line up.
    @com pub interface ITaskbarList : IUnknown {
        HrInit(): i32
        AddTab(hwnd: HWND): i32
        DeleteTab(hwnd: HWND): i32
        ActivateTab(hwnd: HWND): i32
        SetActiveAlt(hwnd: HWND): i32
    }

    /// `ITaskbarList2` — adds the full-screen marker.
    @com pub interface ITaskbarList2 : ITaskbarList {
        MarkFullscreenWindow(hwnd: HWND, fFullscreen: i32): i32
    }

    /// `ITaskbarList3` — Windows 7+ taskbar customisation entry
    /// point. CoCreate against `CLSID_TaskbarList` with
    /// `IID_ITaskbarList3`, call `HrInit` once, then drive
    /// `SetProgressState` / `SetProgressValue` per window.
    @com pub interface ITaskbarList3 : ITaskbarList2 {
        SetProgressValue(hwnd: HWND, ullCompleted: u64, ullTotal: u64): i32
        SetProgressState(hwnd: HWND, tbpFlags: u32): i32
        RegisterTab(hwndTab: HWND, hwndMDI: HWND): i32
        UnregisterTab(hwndTab: HWND): i32
        SetTabOrder(hwndTab: HWND, hwndInsertBefore: HWND): i32
        SetTabActive(hwndTab: HWND, hwndMDI: HWND, dwReserved: u32): i32
        ThumbBarAddButtons(hwnd: HWND, cButtons: u32, pButton: *void): i32
        ThumbBarUpdateButtons(hwnd: HWND, cButtons: u32, pButton: *void): i32
        ThumbBarSetImageList(hwnd: HWND, himl: *void): i32
        SetOverlayIcon(hwnd: HWND, hIcon: *void, pszDescription: *const u16): i32
        SetThumbnailTooltip(hwnd: HWND, pszTip: *const u16): i32
        SetThumbnailClip(hwnd: HWND, prcClip: *void): i32
    }
}
